//! athena-deploy core — the Athena value-stream DEPLOY verb (model → live graph).
//!
//! Atomic-verb contract (peer of werk-*, ADR-032/037):
//! - Zero-dep std-only; curl/riot/chorus-log invoked as subprocesses.
//! - Idempotent: same TTL → same graph (delete-staged-subjects-then-insert).
//! - ADDITIVE per-domain: a deploy replaces only the deploying domain's OWN subjects;
//!   sibling domains + live-loaded instance data survive (fixes the #3540/#3496 clobber).
//! - Never a whole-graph COPY/clear (dodges #3496's NodeTableTRDF large-clear failure);
//!   stages via GSP POST, merges via one SPARQL transaction.
//! - Emits spine events athena.deployed / athena.deploy.failed.
//! - Exit 0 deployed+verified; 1 on any failure (thin main maps Result→exit code).

use std::path::Path;
use std::process::Command;

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// The model SET to deploy: an explicit `TTL` override (single member) else the
/// default set (chorus.ttl + werk-domains.ttl). Pure — unit-tested.
// #4186 — the one home for the model/seed predicates; `athena-deploy scope` prints them.
pub mod model_scope { include!("../../shared/model_scope.rs"); }

/// `athena-deploy scope <root> <git range>` — list the model and seed sources a diff
/// touched, one per line as `model|<path>` / `seed|<path>`. Exit 0 with no lines when
/// the range carries neither. Exit 2 when git cannot read the range (never a silent
/// empty: an unreadable range must not read as "nothing to deploy").
pub fn scope(root: &str, range: &str) -> Result<String, String> {
    let out = Command::new("git").args(["-C", root, "diff", "--name-only", range]).output()
        .map_err(|e| format!("scope: git: {e}"))?;
    if !out.status.success() {
        return Err(format!("scope: git diff --name-only {range} failed in {root}: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let diff = String::from_utf8_lossy(&out.stdout);
    let mut lines: Vec<String> = model_scope::changed_model_sources(&diff).into_iter().map(|p| format!("model|{p}")).collect();
    lines.extend(model_scope::changed_seed_sources(&diff).into_iter().map(|p| format!("seed|{p}")));
    Ok(lines.join("\n"))
}

/// #4195 — the verdict on a run's own legibility: `count` spine lines carry its trace,
/// `want` is the floor for the legs that ran. Pure; the negative proofs are the tests.
pub fn trace_verdict(trace: &str, count: usize, want: usize) -> Result<String, String> {
    if trace.trim().is_empty() { return Err("prove-trace: empty trace id — the run never minted one".into()); }
    if count < want {
        return Err(format!("prove-trace: {} event(s) on trace {} but {} leg(s) ran — a step ran without leaving a record; the run is unreconstructable from the spine", count, trace, want));
    }
    Ok(format!("traceable: {} event(s) on {} (floor {}) — the run can be replayed from the spine", count, trace, want))
}

/// `athena-deploy prove-trace <trace> <want> [--spine <path>]`: count the spine lines
/// that carry the trace (as `"trace":"…"`, `"trace_id":"…"` or `trace_id=…`) and refuse
/// below the floor. An unreadable spine is a refusal, never a skip.
pub fn prove_trace(trace: &str, want: usize, spine: &str) -> Result<String, String> {
    let (count, read) = count_trace_from_tail(trace, spine, 4 << 20, 512 << 20)?;
    trace_verdict(trace, count, want).map(|s| format!("{} ({} bytes read from the tail)", s, read))
}

/// #4177 — count the spine lines carrying `trace`, reading the file BACKWARDS in
/// `chunk`-byte pieces and stopping early: a run's events sit at the tail of the spine,
/// so once matches have been seen and three whole chunks pass with none, the run's
/// start is behind us. `cap` bounds the read no matter what. The whole-file read this
/// replaces took 12m50 on a 2.56 GB spine (2026-09-17 11:17). Returns (count, bytes read).
pub fn count_trace_from_tail(trace: &str, spine: &str, chunk: usize, cap: usize) -> Result<(usize, usize), String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(spine).map_err(|e| format!("prove-trace: spine {} not readable ({}) — this run left no legible record", spine, e))?;
    let len = f.metadata().map_err(|e| format!("prove-trace: {}", e))?.len() as usize;
    let needles = [format!("\"trace\":\"{}\"", trace), format!("\"trace_id\":\"{}\"", trace), format!("trace_id={}", trace)];
    let mut end = len; let mut count = 0usize; let mut read = 0usize; let mut carry: Vec<u8> = Vec::new();
    let mut seen_any = false; let mut empty_since_match = 0usize;
    while end > 0 && read < cap {
        let start = end.saturating_sub(chunk);
        let mut buf = vec![0u8; end - start];
        f.seek(SeekFrom::Start(start as u64)).map_err(|e| format!("prove-trace: {}", e))?;
        f.read_exact(&mut buf).map_err(|e| format!("prove-trace: {}", e))?;
        read += buf.len();
        buf.extend_from_slice(&carry);
        // the first line of this chunk may be cut; keep it as carry for the next (earlier) chunk
        let cut = if start > 0 { buf.iter().position(|&b| b == b'\n').map(|i| i + 1).unwrap_or(buf.len()) } else { 0 };
        let (head, body) = buf.split_at(cut);
        let text = String::from_utf8_lossy(body);
        let here = text.lines().filter(|l| needles.iter().any(|n| l.contains(n.as_str()))).count();
        count += here;
        if here > 0 { seen_any = true; empty_since_match = 0; } else if seen_any { empty_since_match += 1; }
        carry = head.to_vec();
        end = start;
        if seen_any && empty_since_match >= 3 { break; }
    }
    Ok((count, read))
}

pub fn model_set(root: &str, ttl_override: Option<String>) -> Vec<String> {
    match ttl_override {
        Some(t) if !t.is_empty() => vec![t],
        // #4229 — the FULL set the bash deploys, not the two-member stub. The
        // stub is how this verb came to load 2 files where the bash loads 28
        // into the ontology graph, and report success either way.
        _ => vec![
            format!("{root}/roles/silas/ontology/chorus.ttl"),
            format!("{root}/roles/kade/ontology/werk-domains.ttl"),
            format!("{root}/roles/wren/ontology/domains-wren-silas.ttl"),
            format!("{root}/roles/kade/ontology/domains-kade-3581.ttl"),
            format!("{root}/roles/kade/ontology/domains-builds-decisions-rcas-4022.ttl"),
            format!("{root}/designing/data/product-instances.ttl"),
            format!("{root}/roles/silas/ontology/alerts-4085.ttl"),
            format!("{root}/roles/silas/ontology/cmdb-layers-4293.ttl"),
            format!("{root}/roles/wren/ontology/clearing-domains-3860.ttl"),
            format!("{root}/roles/wren/ontology/memory-4010.ttl"),
            format!("{root}/roles/wren/ontology/board-3654.ttl"),
            format!("{root}/roles/wren/ontology/priorities-3686.ttl"),
            format!("{root}/roles/wren/ontology/policies-4077.ttl"),
            format!("{root}/roles/silas/ontology/governance-checks-3846.ttl"),
            format!("{root}/roles/silas/ontology/security-model-3618.ttl"),
            format!("{root}/roles/silas/ontology/security-3619-surfaces.ttl"),
            format!("{root}/roles/silas/ontology/security-3619-surfaces-cards.ttl"),
            format!("{root}/roles/silas/ontology/security-3619-surfaces-jobs.ttl"),
            format!("{root}/roles/silas/ontology/security-3619-surfaces-wave2.ttl"),
            format!("{root}/roles/silas/ontology/security-3619-surfaces-final.ttl"),
            format!("{root}/roles/silas/ontology/nostr-credential-shape-3691.ttl"),
            format!("{root}/roles/silas/ontology/session-4202.ttl"),
            format!("{root}/roles/silas/ontology/graph-status-3733.ttl"),
            format!("{root}/roles/wren/ontology/principles-3749.ttl"),
            format!("{root}/roles/wren/ontology/values-shape-4006.ttl"),
            format!("{root}/roles/kade/ontology/practices-3754.ttl"),
            format!("{root}/designing/data/model-version.ttl"),
            format!("{root}/roles/kade/ontology/pipelines-4040.ttl"),
            format!("{root}/roles/wren/ontology/hats-4175.ttl"),
        ],
    }
}

/// #3752 — one staged retirement, as the JSONL carries it. The verb that
/// stages these never touches the store; this is the write boundary.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Retirement {
    pub subject_domain: String,
    pub object_class: String,
    pub retire_subject: String,
    pub graph: String,
    pub retire_graph: String,
    pub retire_class: String,
    pub status: String,
}

/// What a retirement line asks the deploy to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetireAction {
    /// #3788 — not `staged`. Until this existed every deploy re-executed every
    /// line, so `status` was decoration: ten entries fired as intended on
    /// 2026-08-06, the records were restored from backup, and seven fired
    /// AGAIN during that card's land, removing every person and agent from the
    /// allow-set the doors read. Only `staged` executes.
    Skip { status: String, target: String },
    /// One definesVocabulary triple, re-checked against the live served routes.
    Claim { domain: String, class: String, graph: String },
    /// Every triple of one subject.
    Subject { iri: String, graph: String },
    /// Every row of one class in one graph (#4187).
    Class { class: String, graph: String },
    /// A whole graph, backed up and verified first (#3732).
    WholeGraph { graph: String },
}

/// Read one JSONL line. `Ok(None)` is a blank line. A line that is present but
/// unreadable is an ERROR, never a skip: fail-closed, #3752. The bash pays for
/// this with a python subprocess per line and a \x1f separator, because TAB
/// collapsed empty fields and turned a claim entry into a subject retirement
/// of its own graph name on the first test run.
pub fn parse_retirement(line: &str) -> Result<Option<Retirement>, String> {
    let t = line.trim();
    if t.is_empty() {
        return Ok(None);
    }
    let field = |key: &str| -> Result<String, String> {
        let pat = format!("\"{key}\"");
        let Some(k) = t.find(&pat) else { return Ok(String::new()) };
        let rest = &t[k + pat.len()..];
        let Some(colon) = rest.find(':') else {
            return Err(format!("key {key:?} has no value"));
        };
        let after = rest[colon + 1..].trim_start();
        if !after.starts_with('"') {
            return Err(format!("key {key:?} is not a string"));
        }
        let body = &after[1..];
        let end = body.find('"').ok_or_else(|| format!("key {key:?} is unterminated"))?;
        Ok(body[..end].to_string())
    };
    if !t.starts_with('{') || !t.ends_with('}') {
        return Err("line is not a JSON object".to_string());
    }
    let status = field("status")?;
    Ok(Some(Retirement {
        subject_domain: field("subject_domain")?,
        object_class: field("object_class")?,
        retire_subject: field("retire_subject")?,
        graph: field("graph")?,
        retire_graph: field("retire_graph")?,
        retire_class: field("retire_class")?,
        status: if status.is_empty() { "staged".into() } else { status },
    }))
}

/// Decide what one entry does. Pure, so the #3788 lockout has a test.
pub fn retirement_action(r: &Retirement, default_graph: &str) -> RetireAction {
    if r.status != "staged" {
        let target = if !r.retire_subject.is_empty() {
            r.retire_subject.clone()
        } else if !r.retire_graph.is_empty() {
            r.retire_graph.clone()
        } else {
            "claim".to_string()
        };
        return RetireAction::Skip { status: r.status.clone(), target };
    }
    let graph = if r.graph.is_empty() { default_graph.to_string() } else { r.graph.clone() };
    if !r.retire_graph.is_empty() {
        return RetireAction::WholeGraph { graph: r.retire_graph.clone() };
    }
    if !r.retire_class.is_empty() {
        return RetireAction::Class { class: r.retire_class.clone(), graph };
    }
    if !r.retire_subject.is_empty() {
        return RetireAction::Subject { iri: r.retire_subject.clone(), graph };
    }
    RetireAction::Claim {
        domain: r.subject_domain.clone(),
        class: r.object_class.clone(),
        graph,
    }
}

/// AC3 — may this delete proceed? Every delete path in the deploy unloads
/// first and REFUSES if the dump failed or landed short.
///
/// The bash applies this to whole-graph retirement only (#3732). Subject and
/// class retirement — the leg #4216's 226 rows went through — issue their
/// DELETE with no backup at all. One rule, all three.
///
/// `live` is what the store says is there; `dumped` is what the CONSTRUCT
/// actually wrote. Nothing live is idempotent, not an error: a retirement that
/// already ran has nothing to delete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeleteGuard {
    /// Nothing to delete — already retired.
    AlreadyAbsent,
    /// Backed up and verified; the delete may run.
    Proceed { backed_up: usize },
    /// Refuse, with the reason to put on the spine.
    Refuse(String),
}

pub fn delete_guard(live: Option<usize>, dumped: Option<usize>, target: &str) -> DeleteGuard {
    let Some(live) = live else {
        // The store did not answer the count. A blind delete is never allowed
        // — the same rule as a blind verify (#3726/#3732).
        return DeleteGuard::Refuse(format!(
            "could not count {target} — refusing a blind delete"
        ));
    };
    if live == 0 {
        return DeleteGuard::AlreadyAbsent;
    }
    let dumped = dumped.unwrap_or(0);
    if dumped < live {
        return DeleteGuard::Refuse(format!(
            "backup holds {dumped} line(s) for {live} live triple(s) in {target}; \
             no verified restore path, NOT deleting"
        ));
    }
    DeleteGuard::Proceed { backed_up: dumped }
}

/// The collection a class is served at, from its name. Pure.
pub fn route_for_class(class: &str) -> String {
    let c = class.trim().to_lowercase();
    match c.as_str() {
        "property" => "properties".to_string(),
        "propertykey" => "propertykeys".to_string(),
        _ if c.ends_with('y') => format!("{}ies", &c[..c.len() - 1]),
        _ => format!("{c}s"),
    }
}

/// Is that collection served RIGHT NOW, per athena-make's route list?
///
/// Matched anywhere in a collection path, never as a bare `/name`. Routes
/// carry a version and a domain — Credential is at `/v1/security/credentials`
/// and the discovery document holds no bare `/credentials` at all — so the old
/// needle could never match and no staged claim could be refused for being
/// served. The guard that exists to stop you retiring a live surface had gone
/// vacuous, and this suite was red about it from #4166 while it read as noise.
pub fn route_is_served(served_resp: &str, route: &str) -> bool {
    let needle = format!("/{route}\"");
    served_resp.match_indices(&needle).any(|(i, _)| {
        served_resp[..i].rfind('"').map(|q| !served_resp[q + 1..i].contains(' ')).unwrap_or(false)
    })
}

/// Did athena-make answer the route question at all? An unanswerable door must
/// never read as "nothing is served".
pub fn serve_check_answered(served_resp: &str) -> bool {
    served_resp.contains("\"served\"")
}

/// #4080 — a bare run from INSIDE A WERK must not default to prod.
///
/// On 2026-09-03 a hand run from a werk wrote the live ontology graph because
/// these defaults are prod. The pipeline's env-up passes the werk store and the
/// canonical land runs from canonical; a role at a werk shell gets neither and
/// lands on prod silently. Refuse: name the store, or say canonical on purpose.
///
/// A test that names its own throwaway graph is not a prod write and passes —
/// only the LIVE graph is refused, or the guard would block every fixture and
/// be switched off. Pure: the four inputs are the whole decision.
pub fn werk_target_refused(
    in_werk: bool,
    fuseki_gsp_set: bool,
    deploy_target: Option<&str>,
    ontology_graph: &str,
) -> bool {
    in_werk
        && !fuseki_gsp_set
        && deploy_target != Some("canonical")
        && ontology_graph == "urn:chorus:ontology"
}

/// Is this path inside a werk?
pub fn path_in_werk(path: &str) -> bool {
    path.contains("/chorus-werk/")
}

/// #4029 — the blank-node cleanup, and it is the whole reason the graph grew.
///
/// The merge deletes a staged subject's OWN triples before re-inserting, but a
/// shape body is a blank-node tree (`sh:property [ … ]`) whose nodes get a
/// fresh identity on every load: nothing in staging ever matched the old ones,
/// so every deploy left the previous bodies behind and added new ones — 92
/// deploys, roughly +880 triples each, 5,230 to 77,770.
///
/// Two parts. First delete the blank-node trees hanging off staged subjects,
/// deepest first — a leaf must go before the node pointing at it, or the next
/// level's pattern no longer matches. Then sweep blank nodes nothing points
/// at: bodies orphaned by EARLIER deploys are unreachable from any subject, so
/// the walk cannot find them, and a blank node with no parent is garbage here
/// by definition. Six passes each, because removing a parent orphans children.
pub fn bnode_cleanup(staging: &str, ontology: &str, depth: usize, sweeps: usize) -> String {
    let mut out = String::new();
    for d in (1..=depth).rev() {
        let mut chain = String::from("?s ?p0 ?b1 .");
        let mut filt = String::from("isBlank(?b1)");
        for i in 2..=d {
            chain.push_str(&format!(" ?b{} ?p{} ?b{} .", i - 1, i - 1, i));
            filt.push_str(&format!(" && isBlank(?b{i})"));
        }
        out.push_str(&format!(
            "DELETE {{ GRAPH <{ontology}> {{ ?b{d} ?pl ?ol }} }} WHERE {{ GRAPH <{staging}> \
             {{ ?s ?sp ?so }} GRAPH <{ontology}> {{ {chain} ?b{d} ?pl ?ol FILTER({filt}) }} }} ; "
        ));
    }
    for _ in 0..sweeps {
        out.push_str(&format!(
            "DELETE {{ GRAPH <{ontology}> {{ ?ob ?op ?oo }} }} WHERE {{ GRAPH <{ontology}> \
             {{ ?ob ?op ?oo FILTER(isBlank(?ob)) FILTER NOT EXISTS {{ GRAPH <{ontology}> \
             {{ ?ox ?oy ?ob }} }} }} }} ; "
        ));
    }
    out
}

/// #3536 — RETIRE_ABSENT. Deploys never truncate by default.
///
/// "Stop truncating our data" (Jeff, 2026-06-30), after a deploy whose staging
/// lacked the 34 live domains retired every one of them — the 2026-06-26 wipe.
/// It is opt-in, and even opted in the empty-staging guard below is the last
/// backstop.
pub fn retire_absent_on(env_value: Option<&str>) -> bool {
    matches!(env_value.map(str::trim), Some("1"))
}

/// #3536 — the empty-staging guard. Retire DELETEs live domain subjects absent
/// from staging, so ZERO domains in staging would delete EVERY live domain.
///
/// A count against LIVE cannot be used: retiring N domains legitimately makes
/// staging = live − N, so "staging < live" wrongly blocks all retirement — TDD
/// caught that. The only safe question is whether staging has any domains at
/// all, and an unanswered count is a refusal, never a zero.
pub fn retire_guard_allows(staged_domains: Option<usize>) -> Result<(), String> {
    match staged_domains {
        None => Err(
            "REFUSING retire — could not count staging's domain subjects; \
             an unanswered count is not zero and not a licence to delete"
                .to_string(),
        ),
        Some(0) => Err(
            "REFUSING retire — staging has 0 domain subjects (empty/incomplete \
             staging would delete ALL live domains; #3536 guard)"
                .to_string(),
        ),
        Some(_) => Ok(()),
    }
}

/// #4125 — a subject deleted from source is NAMED, not silently kept.
///
/// The merge is per-subject additive: it deletes a STAGED subject's triples
/// and re-inserts them. A subject removed from a source file is never in
/// staging, so nothing touches it and it lives forever — nothing has ever left
/// the graph by absence.
///
/// The obvious fix, deleting whatever is absent from staging, is the
/// 2026-06-26 graph wipe: that is what RETIRE_ABSENT does, and it was turned
/// off by default after a deploy whose staging lacked the 34 live domains
/// retired them all. So absence drives a REFUSAL a human resolves by staging a
/// retirement, never a delete.
///
/// A file's DECLARED subjects: `chorus:<local> a …` at the start of a line.
/// A subject that appears only as an OBJECT is not this file's to retire.
pub fn declared_subjects(ttl: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in ttl.lines() {
        let Some(rest) = line.strip_prefix("chorus:") else { continue };
        let mut it = rest.split_whitespace();
        let Some(name) = it.next() else { continue };
        if it.next() != Some("a") {
            continue;
        }
        if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            continue;
        }
        if !out.iter().any(|n| n == name) {
            out.push(name.to_string());
        }
    }
    out.sort();
    out
}

/// Subjects present in the previously deployed version of a file and absent
/// from the working copy. Pure, so the refusal has a test that does not need
/// a store or a git history.
pub fn vanished_subjects(previous: &str, current: &str) -> Vec<String> {
    let now = declared_subjects(current);
    declared_subjects(previous)
        .into_iter()
        .filter(|n| !now.contains(n))
        .collect()
}

/// #3536 AC2 / #3731 — the SHACL report. Report-only, never a gate: the model
/// is mid-migration, so a hard gate would refuse every deploy.
///
/// Its three states must stay distinguishable. A crashed validator used to
/// report "0 violation(s)" — migration-complete-by-crash, the could-not-ask
/// class — and because of that, plus two parsing bugs (an extensionless temp
/// file and a mid-stream BOM), this leg had never actually validated the model
/// on any full deploy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShaclReport {
    Ran { violations: usize },
    Crashed,
    ValidatorAbsent,
}

impl ShaclReport {
    /// What goes on the spine. `unknown` is NOT zero, and the two must never
    /// print the same way.
    pub fn violations_field(&self) -> String {
        match self {
            ShaclReport::Ran { violations } => violations.to_string(),
            _ => "unknown".to_string(),
        }
    }
    pub fn status_field(&self) -> &'static str {
        match self {
            ShaclReport::Ran { .. } => "ran",
            ShaclReport::Crashed => "crashed",
            ShaclReport::ValidatorAbsent => "absent",
        }
    }
}

/// Count violations in a validator report. Pure.
pub fn shacl_violations(report: &str) -> usize {
    report.lines().filter(|l| l.contains("sh:resultSeverity")).count()
}

/// Strip a UTF-8 BOM from the START of one member before it joins the union.
/// Jena tolerates a BOM at the start of a file, so riot validated the offending
/// file alone and nobody noticed; inside a concatenation the same three bytes
/// land mid-stream and the whole union dies. Stripping per file, rather than
/// fixing the one file that had it, is the difference between this recurring
/// on the next authored .ttl and not.
pub fn strip_bom(member: &str) -> &str {
    member.strip_prefix('\u{feff}').unwrap_or(member)
}

/// #4125 — a source file may not author a Role as an owner.
///
/// `chorus:ownedBy` is `a owl:ObjectProperty ; rdfs:range chorus:Principal`.
/// A store fix cannot hold while the source authors the violation: the bad
/// owners were taken to 0 on 2026-09-18 and the next three deploys put 208
/// back, because a deploy re-creates whatever the source says. So the refusal
/// belongs where the source enters the store.
///
/// Scoped to the `ownedBy` predicate ONLY. `chorus:role-<name>` is legitimate
/// elsewhere — holdsRole, appointedHat — and a guard that cannot tell those
/// two apart is the #3734 shape again. Returns one line per offending file.
pub fn role_owner_offences(file_label: &str, ttl: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (i, line) in ttl.lines().enumerate() {
        let code = line.split('#').next().unwrap_or("");
        if let Some(pos) = code.find("ownedBy") {
            let rest = code[pos + "ownedBy".len()..].trim_start();
            if rest.starts_with("chorus:role-") {
                out.push(format!("{file_label}:{}: {}", i + 1, code.trim()));
            }
        }
    }
    out
}

/// #4250 — one property, one declaration. A property declared twice is not an
/// override: RDF reads two `rdfs:domain` statements as an intersection, so
/// `chorus:filePath` declared on CodeFile and again on File came to mean "only
/// on something that is both", and the writeOwner sat on whichever copy you
/// did not read.
///
/// It never surfaced as itself. It surfaced as four unrelated reds in three
/// suites: the hydration validator said a predicate had no writeOwner,
/// athena-make refused to generate EmitContract, Metric, Property and
/// PropertyKey ("two shapes disagree about one property"), and the
/// instances-graph suite then 404'd on those four routes. Nine properties were
/// in this state before this card. The tenth must fail here, at the deploy,
/// named — not next week in a suite that cannot say why.
///
/// Takes every file in the set at once, because the two declarations are
/// usually in two different roles' files. Comments are stripped first so a
/// commented-out declaration is not an offence. Returns one line per property
/// that is declared more than once, naming every file and line.
pub fn duplicate_property_declarations(files: &[(String, String)]) -> Vec<String> {
    use std::collections::BTreeMap;
    let mut seen: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (label, text) in files {
        for (i, line) in text.lines().enumerate() {
            let code = line.split('#').next().unwrap_or("").trim_end();
            let Some(rest) = code.strip_prefix("chorus:") else { continue };
            let Some((name, tail)) = rest.split_once(' ') else { continue };
            let tail = tail.trim_start();
            if !(tail.starts_with("a owl:DatatypeProperty")
                || tail.starts_with("a owl:ObjectProperty"))
            {
                continue;
            }
            if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                continue;
            }
            seen.entry(name.to_string())
                .or_default()
                .push(format!("{label}:{}", i + 1));
        }
    }
    seen.into_iter()
        .filter(|(_, at)| at.len() > 1)
        .map(|(name, at)| format!("chorus:{name} declared {} times — {}", at.len(), at.join(", ")))
        .collect()
}

/// Does this run deploy the eight domain sets? The bash gates every one of
/// them behind `[ -z "${TTL:-}" ]`: a single-file partial run deploys ONLY the
/// file it was handed, and must not quietly re-stage thirteen others. Pure.
pub fn sets_run(ttl_override: Option<&str>) -> bool {
    !matches!(ttl_override.map(str::trim), Some(t) if !t.is_empty())
}

/// One TTL set staged into one domain graph. #4229 — the eight copies of this
/// leg in athena-deploy-model.sh become eight rows of data and one
/// implementation. Four of those copies were one verify and two refusals
/// weaker than the others, because later fixes only reached some of them;
/// data cannot drift from itself that way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainSet {
    pub name: String,
    pub graph: String,
    /// CHORUS_ROOT-relative, in staging order.
    pub files: Vec<String>,
    /// #4254 — replace the live graph instead of merging into it.
    ///
    /// Merge is the default and must stay the default: every other set shares
    /// its graph with rows nobody in that set authored, so delete-by-absence
    /// there would silently retire another role's work.
    ///
    /// A set opts in only when it OWNS its graph outright. The vocabulary graph
    /// is written by exactly two files and nothing else, and for a graph like
    /// that delete-by-absence is safe by construction — and is the only way a
    /// term removed from source actually leaves the STORE, since the merge
    /// keeps siblings and a concept would survive after its class is gone (the
    /// #4250 ghost). Wren's call, 2026-09-21.
    ///
    /// Guarded at parse time: flagging a graph that another set also targets is
    /// a refusal, because that is the 2026-08-28 wipe with a config switch.
    pub replace: bool,
}

/// Parse the domain-set manifest. Pure.
///
/// A malformed line is a REFUSAL, never a skip: a deploy that quietly drops a
/// row from its own manifest is the silent-miss class this card exists to end
/// (the Rust verb deployed 2 files while the bash deployed 41, and said
/// success). Blank lines and `#` comments are the only things ignored.
pub fn parse_domain_sets(text: &str) -> Result<Vec<DomainSet>, String> {
    let mut out: Vec<DomainSet> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('|').map(str::trim).collect();
        if !(3..=4).contains(&parts.len()) || parts.iter().take(3).any(|p| p.is_empty()) {
            return Err(format!(
                "domain-set-manifest line {}: expected <set>|<graph>|<path>[|replace], got {:?}",
                i + 1,
                line
            ));
        }
        // An unknown 4th column is a REFUSAL, never a shrug. A typo silently
        // read as merge would leave delete-by-absence off on a set that asked
        // for it, and nothing anywhere would say so.
        let replace = match parts.get(3).copied() {
            None | Some("") => false,
            Some("replace") => true,
            Some(other) => {
                return Err(format!(
                    "domain-set-manifest line {}: 4th column must be `replace` or absent, got {:?}",
                    i + 1,
                    other
                ))
            }
        };
        let (name, graph, path) = (parts[0], parts[1], parts[2]);
        if !graph.starts_with("urn:") {
            return Err(format!(
                "domain-set-manifest line {}: {:?} is not a graph IRI",
                i + 1,
                graph
            ));
        }
        match out.iter_mut().find(|s| s.name == name) {
            Some(existing) => {
                if existing.graph != graph {
                    return Err(format!(
                        "domain-set-manifest line {}: set {:?} already targets <{}>, cannot also target <{}>",
                        i + 1,
                        name,
                        existing.graph,
                        graph
                    ));
                }
                existing.files.push(path.to_string());
                // Every line of a set must agree about replace. One line saying
                // replace and another not is not a preference, it is two
                // contradictory claims about whether the graph may be emptied.
                if existing.replace != replace {
                    return Err(format!(
                        "domain-set-manifest line {}: set {:?} disagrees with itself about \
                         `replace` — every line of one set must say the same thing",
                        i + 1,
                        name
                    ));
                }
            }
            None => out.push(DomainSet {
                name: name.to_string(),
                graph: graph.to_string(),
                files: vec![path.to_string()],
                replace,
            }),
        }
    }

    // #4254 — replace is only safe when ONE set writes that graph. Flagging a
    // shared graph would drop every co-tenant's rows on the next deploy, which
    // is the 2026-08-28 ontology wipe with a config switch in front of it. The
    // guard is here, at parse, so the refusal happens before any write.
    for s in &out {
        if !s.replace {
            continue;
        }
        let co: Vec<&str> = out
            .iter()
            .filter(|o| o.graph == s.graph && o.name != s.name)
            .map(|o| o.name.as_str())
            .collect();
        if !co.is_empty() {
            return Err(format!(
                "domain-set-manifest: set {:?} asks to REPLACE <{}>, but {} also target(s) it — \
                 replacing a shared graph deletes the other set(s)' rows. Drop the flag, or give \
                 this set a graph nothing else writes.",
                s.name,
                s.graph,
                co.join(", ")
            ));
        }
    }

    Ok(out)
}

/// The additive-merge update: DELETE only the triples whose SUBJECT is (re)defined
/// in staging, then INSERT staging — one transaction. Touches only the deploying
/// domain's own subjects; leaves every sibling's triples intact. Pure — unit-tested.
pub fn merge_sparql(staging: &str, ontology: &str) -> String {
    format!(
        "DELETE {{ GRAPH <{ont}> {{ ?s ?p ?o }} }} \
         WHERE {{ GRAPH <{stg}> {{ ?s ?sp ?so }} GRAPH <{ont}> {{ ?s ?p ?o }} }} ; \
         INSERT {{ GRAPH <{ont}> {{ ?s ?p ?o }} }} \
         WHERE {{ GRAPH <{stg}> {{ ?s ?p ?o }} }}",
        ont = ontology,
        stg = staging,
    )
}

fn ok_http(code: &str) -> bool {
    matches!(code, "200" | "201" | "204")
}

/// The Fuseki write credential as curl args. Empty when unset — unauthenticated,
/// which is the pre-Shiro behaviour `fuseki-auth.sh` is careful to preserve
/// (#3566 "deploy the credential before requiring it").
///
/// #3561: the bash script this verb replaced sourced `fuseki-auth.sh`; the Rust
/// port did not, so every write went out anonymous and Shiro answered 401. The
/// verb then reported `staging-load-http-401` — true, and useless: a refusal that
/// names the store's answer cannot name a missing credential.
fn fuseki_auth() -> Vec<String> {
    match std::env::var("FUSEKI_ADMIN_PASSWORD") {
        Ok(pw) if !pw.is_empty() => {
            let user = env_or("FUSEKI_ADMIN_USER", "admin");
            vec!["-u".to_string(), format!("{user}:{pw}")]
        }
        _ => Vec::new(),
    }
}

/// Run a curl invocation, returning its stdout (trimmed) or a failure message.
/// The write credential is prepended HERE, at the one door, so no call site can
/// forget it — the same "one place owns the credential" rule `fuseki-auth.sh`
/// states for the bash writers.
fn curl(args: &[&str]) -> Result<String, String> {
    // #4175 — EVERY Fuseki call carries a timeout, applied here at the one door
    // so no call site can forget it. On 2026-09-14 a compaction held the write
    // lock from 15:01 and a bare DELETE sat for 32 minutes, taking a test run
    // with it; reads answered in 0.011s the whole time. An unbounded write
    // turns "someone else holds the lock" into a hang nobody can read, instead
    // of a failure in seconds that names itself. Overridable, never absent.
    let timeout = env_or("FUSEKI_WRITE_TIMEOUT", "120");
    let out = Command::new("curl")
        .args(["--max-time", &timeout])
        .args(fuseki_auth())
        .args(args)
        .output()
        .map_err(|e| format!("curl spawn failed: {e}"))?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Best-effort spine emit (never blocks the deploy result).
fn emit_spine(chorus_log: &str, event: &str, role: &str, fields: &[(&str, String)]) {
    let mut args: Vec<String> = vec![event.to_string(), role.to_string()];
    for (k, v) in fields {
        args.push(format!("{k}={v}"));
    }
    let _ = Command::new(chorus_log).args(&args).output();
}

pub fn run_athena_deploy() -> Result<String, String> {
    let root = env_or("CHORUS_ROOT", "/Users/jeffbridwell/CascadeProjects/chorus");
    let ontology = env_or("ONTOLOGY_GRAPH", "urn:chorus:ontology");
    let gsp = env_or("FUSEKI_GSP", "http://localhost:3030/pods/data");
    let query = env_or("FUSEKI_QUERY", "http://localhost:3030/pods/query");
    let update = env_or("FUSEKI_UPDATE", "http://localhost:3030/pods/update");
    let chorus_log = env_or("CHORUS_LOG", &format!("{root}/platform/scripts/chorus-log"));
    let role = std::env::var("DEPLOY_ROLE")
        .or_else(|_| std::env::var("CHORUS_ROLE"))
        .unwrap_or_else(|_| "system".to_string());
    // #4080 — refuse a bare werk run before anything else touches the store.
    let cwd = std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default();
    let in_werk = path_in_werk(&root) || path_in_werk(&cwd);
    if werk_target_refused(
        in_werk,
        std::env::var("FUSEKI_GSP").is_ok(),
        std::env::var("DEPLOY_TARGET").ok().as_deref(),
        &ontology,
    ) {
        eprintln!(
            "athena-deploy: REFUSED — running inside a werk with no FUSEKI_GSP set; the default \
             is PROD (localhost:3030/pods). Set FUSEKI_GSP/FUSEKI_QUERY/FUSEKI_UPDATE to the werk \
             store (werk-<role>), or DEPLOY_TARGET=canonical to write prod on purpose (#4080)."
        );
        std::process::exit(78);
    }
    if std::env::var("ATHENA_DEPLOY_TARGET_CHECK_ONLY").as_deref() == Ok("1") {
        println!("target-check: ok gsp={}", std::env::var("FUSEKI_GSP").unwrap_or_else(|_| "<default pods>".into()));
        std::process::exit(0);
    }

    let ttl_override = std::env::var("TTL").ok();
    let set = model_set(&root, ttl_override);
    let staging = format!("{ontology}-staging-deploy");

    let fail = |reason: &str| -> String {
        emit_spine(&chorus_log, "athena.deploy.failed", &role,
            &[("graph", ontology.clone()), ("reason", reason.to_string())]);
        format!("athena-deploy: {reason}")
    };

    // #4125 — refuse before any write if a source authors a Role as an owner.
    let mut offences: Vec<String> = Vec::new();
    for ttl in &set {
        if let Ok(text) = std::fs::read_to_string(ttl) {
            let label = ttl.strip_prefix(&format!("{root}/")).unwrap_or(ttl);
            offences.extend(role_owner_offences(label, &text));
        }
    }
    if !offences.is_empty() {
        eprintln!(
            "athena-deploy: REFUSED — a source file authors a Role as an owner (#4125). \
             chorus:ownedBy ranges over chorus:Principal:"
        );
        for o in offences.iter().take(5) {
            eprintln!("  {o}");
        }
        eprintln!(
            "  -> change chorus:role-<name> to chorus:principal-<name> on the ownedBy line only; \
             other chorus:role-* uses (holdsRole, appointedHat) are fine and untouched."
        );
        return Err(fail(&format!("source-authors-role-owner:{}", offences.len())));
    }

    // #4250 — one property, one declaration. Cross-file, so it runs over the
    // whole set rather than per file.
    let mut loaded: Vec<(String, String)> = Vec::new();
    for ttl in &set {
        if let Ok(text) = std::fs::read_to_string(ttl) {
            let label = ttl.strip_prefix(&format!("{root}/")).unwrap_or(ttl).to_string();
            loaded.push((label, text));
        }
    }
    let dupes = duplicate_property_declarations(&loaded);
    if !dupes.is_empty() {
        eprintln!(
            "athena-deploy: REFUSED — a property is declared more than once (#4250). \
             Two rdfs:domain statements intersect, they do not override, and athena-make \
             will refuse to generate the class rather than pick a winner:"
        );
        for d in dupes.iter().take(10) {
            eprintln!("  {d}");
        }
        eprintln!(
            "  -> keep ONE declaration. Drop rdfs:domain when the property is genuinely \
             shared across classes, and put its range and writeOwner on that one. If the \
             two mean different things, they are a name collision: rename one."
        );
        return Err(fail(&format!("duplicate-property-declaration:{}", dupes.len())));
    }


    // #4125 — a subject deleted from source is named, not silently kept.
    // Compared against the commit the STORE says it was deployed from, not
    // HEAD~1: the question is what this store has lost since it was last
    // written, and only the store can answer it.
    let prev = curl(&["-s", "--data-urlencode",
        &format!("query=SELECT ?c WHERE {{ GRAPH <{ontology}> {{ <urn:chorus:model-deploy> \
                  <urn:chorus:vocab#deployedFromCommit> ?c }} }}"),
        "-H", "Accept: text/csv", &query])
        .ok()
        .and_then(|csv| csv.lines().next_back().map(|l| {
            l.chars().filter(|c| c.is_ascii_hexdigit()).collect::<String>()
        }))
        .filter(|c| c.len() >= 7);
    // DEPLOY_SOURCE_DELETE_CHECK=0 disables the guard — only so the negative
    // proof can show what it catches: a green deploy with the row still live.
    let prev = if env_or("DEPLOY_SOURCE_DELETE_CHECK", "1") == "1" { prev } else { None };
    match prev {
        None => println!(
            "athena-deploy: no deployedFromCommit stamp in <{ontology}> — \
             source-delete check SKIPPED (first deploy into this store)"
        ),
        Some(prev) if run_git(&root, &["cat-file", "-e", &format!("{prev}^{{commit}}")]).is_none() => {
            eprintln!(
                "athena-deploy: stamped commit {prev} is not in this tree — source-delete \
                 check SKIPPED (shallow clone or rewritten history)"
            );
        }
        Some(prev) => {
            let staged_for_retirement: Vec<String> = std::fs::read_to_string(env_or(
                "RETIREMENTS_FILE",
                &format!("{root}/designing/schemas/model-retirements.jsonl"),
            ))
            .map(|t| {
                t.lines()
                    .filter_map(|l| parse_retirement(l).ok().flatten())
                    .filter(|r| r.status == "staged")
                    .map(|r| r.retire_subject)
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();
            // #4256 — the question is "is this subject still DECLARED anywhere
            // in the model set", not "is it still in the file it used to be in".
            // Per-file comparison cannot tell a deletion from a move: #4250
            // moved nine property declarations between files and this guard
            // read all nine as deleted, refused the land's deploy, and left the
            // store on the pre-#4250 model for a day. Build the union first.
            let mut declared_now: Vec<String> = Vec::new();
            for ttl in &set {
                if let Ok(now) = std::fs::read_to_string(ttl) {
                    declared_now.extend(declared_subjects(&now));
                }
            }
            let mut gone: Vec<(String, String)> = Vec::new();
            for ttl in &set {
                let rel = ttl.strip_prefix(&format!("{root}/")).unwrap_or(ttl).to_string();
                let Some(before) = run_git(&root, &["show", &format!("{prev}:{rel}")]) else {
                    continue; // file is new since the stamp
                };
                let Ok(now) = std::fs::read_to_string(ttl) else { continue };
                for name in vanished_subjects(&before, &now) {
                    // Moved to a sibling file in the same set, not deleted.
                    if declared_now.contains(&name) {
                        continue;
                    }
                    // Already staged for retirement? The retirement leg above
                    // removes it; reporting it here would refuse a deploy for
                    // work that is already done, and the guard would be
                    // switched off rather than obeyed.
                    let iri = format!("https://jeffbridwell.com/chorus#{name}");
                    if staged_for_retirement.contains(&iri) {
                        continue;
                    }
                    gone.push((name, rel.clone()));
                }
            }
            if !gone.is_empty() {
                eprintln!(
                    "athena-deploy: REFUSED — {} subject(s) were deleted from source since \
                     {prev} (#4125). Absence never deletes here; stage a retirement:",
                    gone.len()
                );
                for (name, file) in gone.iter().take(5) {
                    eprintln!("  chorus:{name}  (was declared in {file})");
                }
                eprintln!(
                    "  -> stage it: athena-model retire-subject <iri> --card <id>, then re-run \
                     this deploy; or restore the subject to its file if the removal was \
                     accidental."
                );
                return Err(fail(&format!("source-deleted-subjects:{}", gone.len())));
            }
        }
    }

    // #3731 — an ABSENT validator used to mean every file deployed unvalidated
    // with output identical to a clean run (fail-open hole 3). Refuse loudly,
    // with one explicit escape whose output cannot be mistaken for a clean run.
    let riot_here = riot_available();
    if !riot_here {
        if std::env::var("ALLOW_UNVALIDATED").as_deref() == Ok("1") {
            eprintln!(
                "athena-deploy: WARNING — riot NOT INSTALLED, deploying UNVALIDATED TTL \
                 (ALLOW_UNVALIDATED=1 set; #3731). This is not a clean run."
            );
            emit_spine(&chorus_log, "model.deploy.unvalidated", &role,
                &[("graph", ontology.clone()), ("reason", "riot-absent-allowed".to_string())]);
        } else {
            eprintln!(
                "athena-deploy: REFUSING — riot (Jena) not installed; cannot validate the model \
                 before deploy. Install jena, or set ALLOW_UNVALIDATED=1 to proceed loudly \
                 (#3731 fail-closed)."
            );
            return Err(fail("riot-absent"));
        }
    }

    // Validate every member exists + is riot-valid (don't deploy a broken model).
    for ttl in &set {
        if !Path::new(ttl).exists() {
            return Err(fail(&format!("ttl-not-found:{ttl}")));
        }
        if riot_here {
            let riot_bin = env_or("RIOT_BIN", "riot");
            let status = Command::new(&riot_bin).arg("--validate").arg(ttl)
                .output().map(|o| o.status.success()).unwrap_or(false);
            if !status {
                return Err(fail(&format!("riot-invalid:{ttl}")));
            }
        }
    }

    // Step 1: stage the SET into a fresh staging graph (GSP POST merges).
    let _ = curl(&["-s", "-X", "DELETE", "-o", "/dev/null",
        &format!("{gsp}?graph={staging}")]);
    for ttl in &set {
        let code = curl(&["-s", "-o", "/dev/null", "-w", "%{http_code}", "-X", "POST",
            "-H", "Content-Type: text/turtle", "--data-binary", &format!("@{ttl}"),
            &format!("{gsp}?graph={staging}")])?;
        if !ok_http(&code) {
            // Name the credential state, never just the store's answer. A bare
            // 401 sent two roles looking at Fuseki when the cause was an empty
            // FUSEKI_ADMIN_PASSWORD in the calling environment.
            if code == "401" && fuseki_auth().is_empty() {
                return Err(fail("staging-load-http-401-no-credential"));
            }
            return Err(fail(&format!("staging-load-http-{code}")));
        }
    }

    // #3536 — RETIRE_ABSENT. Opt-in only, and even opted in the empty-staging
    // guard runs first: retire deletes live domain subjects ABSENT from
    // staging, so zero domains in staging would delete every live domain.
    // That is the 2026-06-26 wipe, and this is its last backstop.
    let mut retire_clause = String::new();
    if retire_absent_on(std::env::var("RETIRE_ABSENT").ok().as_deref()) {
        let c = "https://jeffbridwell.com/chorus#";
        let staged = curl(&["-s", "--data-urlencode",
            &format!("query=SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE {{ GRAPH <{staging}> \
                      {{ ?s a ?t . FILTER(?t IN (<{c}Domain>, <{c}SubDomain>)) }} }}"),
            "-H", "Accept: text/csv", &query]).unwrap_or_default();
        if let Err(why) = retire_guard_allows(verify_missing(&staged)) {
            eprintln!("athena-deploy: {why}");
            let _ = curl(&["-s", "-X", "DELETE", "-o", "/dev/null", &format!("{gsp}?graph={staging}")]);
            return Err(fail("retire-guard-empty-staging"));
        }
        retire_clause = format!(
            "DELETE {{ GRAPH <{ontology}> {{ ?s ?p ?o }} }} WHERE {{ GRAPH <{ontology}> \
             {{ ?s a <{c}Domain> ; ?p ?o }} FILTER NOT EXISTS {{ GRAPH <{staging}> {{ ?s ?q ?r }} }} }} ; "
        );
    }

    // Step 2: additive merge (delete-staged-subjects-then-insert), one transaction.
    // DEPLOY_BNODE_CLEANUP=0 disables it — only so the negative proof can
    // show the growth it prevents.
    let bnodes = if env_or("DEPLOY_BNODE_CLEANUP", "1") == "1" {
        bnode_cleanup(&staging, &ontology, 6, 6)
    } else {
        String::new()
    };
    let sparql = format!("{retire_clause}{bnodes}{}", merge_sparql(&staging, &ontology));
    let mcode = curl(&["-s", "-o", "/dev/null", "-w", "%{http_code}", "-X", "POST",
        "-H", "Content-Type: application/sparql-update", "--data-binary", &sparql, &update])?;
    if !ok_http(&mcode) {
        let _ = curl(&["-s", "-X", "DELETE", "-o", "/dev/null", &format!("{gsp}?graph={staging}")]);
        return Err(fail(&format!("merge-http-{mcode}")));
    }

    // Step 3 — verify. NOT "is the graph non-empty": that passes even when the
    // merge dropped every staged subject, and it cannot fail against a dead
    // query endpoint either. Ask how many staged subjects are ABSENT after the
    // merge, before staging is dropped, and refuse on an unanswered question
    // (#3726 single-request-truth, #3731 fail-closed).
    let vresp = curl(&["-s", "--data-urlencode",
        &format!("query=SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE {{ GRAPH <{staging}> {{ ?s ?p ?o }} \
                  FILTER NOT EXISTS {{ GRAPH <{ontology}> {{ ?s ?q ?r }} }} }}"),
        "-H", "Accept: text/csv", &query]).unwrap_or_default();
    let verdict = verify_missing(&vresp);
    let _ = curl(&["-s", "-X", "DELETE", "-o", "/dev/null", &format!("{gsp}?graph={staging}")]);
    match verdict {
        None => {
            eprintln!(
                "athena-deploy: VERIFY could not ask <{ontology}> — refusing to pass a blind \
                 verify (#3726 single-request-truth)"
            );
            return Err(fail("verify-unanswered"));
        }
        Some(0) => {}
        Some(n) => return Err(fail(&format!("verify-missing-{n}"))),
    }

    emit_spine(&chorus_log, "athena.deployed", &role,
        &[("graph", ontology.clone()), ("members", set.len().to_string())]);
    // #3561 compatibility — everything watching the deploy reads the bash's
    // names. Both pairs are emitted until every reader is repointed; dropping
    // one at the swap is how a dashboard goes blind without anyone noticing.
    emit_spine(&chorus_log, "model.deployed", &role,
        &[("graph", ontology.clone()), ("members", set.len().to_string())]);

    // #3736 — stamp which commit this store was deployed from. Not decoration:
    // the source-delete guard (#4125) reads deployedFromCommit to know what to
    // diff the working tree against, so without the stamp that guard can never
    // run. Caught by the graph diff against the bash, not by reading the code
    // — it was the only real difference between the two, two triples.
    let stamp_sha = run_git(&root, &["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".into());
    let stamp_ts = Command::new("date").args(["-u", "+%Y-%m-%dT%H:%M:%SZ"]).output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    let stamp = format!(
        "DELETE WHERE {{ GRAPH <{ontology}> {{ <urn:chorus:model-deploy> ?p ?o }} }};\n\
         INSERT DATA {{ GRAPH <{ontology}> {{ \
         <urn:chorus:model-deploy> <urn:chorus:vocab#deployedFromCommit> \"{stamp_sha}\" ; \
         <urn:chorus:vocab#deployedAt> \"{stamp_ts}\" . }} }}"
    );
    let scode = curl(&["-s", "-o", "/dev/null", "-w", "%{http_code}", "-X", "POST",
        "-H", "Content-Type: application/sparql-update", "--data-binary", &stamp, &update])?;
    if !ok_http(&scode) {
        // #3736 — a deploy nobody can tie to a commit is unverifiable, so this
        // fails loud rather than passing with the model already applied.
        return Err(fail(&format!("stamp-write-failed-http-{scode}")));
    }
    println!("athena-deploy: stamped deployedFromCommit={stamp_sha}");

    // #3752 — execute what `athena-model retire-claim` staged. Fail-closed at
    // every ask, idempotent on absent, and every delete backed up first (AC3).
    let retirements_file = env_or(
        "RETIREMENTS_FILE",
        &format!("{root}/designing/schemas/model-retirements.jsonl"),
    );
    if Path::new(&retirements_file).is_file() {
        let text = std::fs::read_to_string(&retirements_file)
            .map_err(|e| fail(&format!("retirements-unreadable:{e}")))?;
        let ctx = StoreCtx {
            gsp: gsp.clone(), query: query.clone(), update: update.clone(),
            chorus_log: chorus_log.clone(), role: role.clone(),
        };
        for (i, line) in text.lines().enumerate() {
            let n = i + 1;
            let entry = match parse_retirement(line) {
                Ok(Some(e)) => e,
                Ok(None) => continue,
                Err(why) => {
                    eprintln!(
                        "athena-deploy: RETIREMENTS line {n} is MALFORMED — refusing the deploy \
                         (fail-closed, #3752): {why}"
                    );
                    return Err(fail(&format!("retirement-staging-malformed:line-{n}")));
                }
            };
            run_retirement(&retirement_action(&entry, &ontology), n, &ctx)?;
        }
    }

    // #3536 AC2 / #3731 — the SHACL report. Non-gating, three distinguishable
    // states, full deploys only (SHACL_REPORT=1 is the test seam).
    if sets_run(std::env::var("TTL").ok().as_deref())
        || std::env::var("SHACL_REPORT").as_deref() == Ok("1")
    {
        let report = run_shacl_report(&root, &set);
        match &report {
            ShaclReport::Ran { violations } => println!(
                "athena-deploy: SHACL report (V2 shapes, non-gating) — {violations} violation(s) \
                 [migration-progress signal, not a gate]"
            ),
            ShaclReport::Crashed => eprintln!(
                "athena-deploy: SHACL validator CRASHED — violations UNKNOWN, not 0 \
                 (#3731; report-only, deploy continues)"
            ),
            ShaclReport::ValidatorAbsent => eprintln!(
                "athena-deploy: SHACL report SKIPPED — validator not installed; model deployed \
                 WITHOUT the V2-shape report (#3731; not a clean-run signal)"
            ),
        }
        emit_spine(&chorus_log, "model.deploy.shacl", &role, &[
            ("graph", ontology.clone()),
            ("violations", report.violations_field()),
            ("gating", "false".to_string()),
            ("status", report.status_field().to_string()),
        ]);
    }

    // #4229 — then the eight domain sets, from the manifest. A TTL= partial
    // run deploys ONLY what it was given, the same gate the bash applies.
    let mut deployed: Vec<String> = Vec::new();
    if sets_run(std::env::var("TTL").ok().as_deref()) {
        let manifest_path = env_or(
            "DOMAIN_SET_MANIFEST",
            &format!("{root}/platform/config/domain-set-manifest.txt"),
        );
        // A missing manifest is a REFUSAL. Treating it as "no sets" is how a
        // run deploys 2 files where it was asked for 41 and reports success.
        let text = std::fs::read_to_string(&manifest_path)
            .map_err(|e| fail(&format!("domain-set-manifest-unreadable:{manifest_path}:{e}")))?;
        let sets = parse_domain_sets(&text).map_err(|e| fail(&e))?;

        // #4254 — one word, one concept, per scheme. Runs over every file in
        // every set, because the two concepts claiming one word usually sit in
        // two roles' files, the way #4250's duplicate properties did.
        //
        // REPORTS, never refuses. Jeff, 2026-09-21: non-blocking until the
        // vocabulary stabilises — we start from colliding words, so a refusal
        // on day one would stop every deploy and take the nightly with it. The
        // count it prints IS the trigger for the card that flips it to a
        // refusal: when the count reaches zero, the guard can bite.
        let mut vocab_files: Vec<(String, String)> = Vec::new();
        for ds in &sets {
            for f in &ds.files {
                let path = format!("{root}/{f}");
                if let Ok(text) = std::fs::read_to_string(&path) {
                    vocab_files.push((f.clone(), text));
                }
            }
        }
        let collisions = duplicate_concept_labels(&vocab_files);
        eprintln!(
            "athena-deploy: vocabulary label collisions: {} (#4254, reporting only)",
            collisions.len()
        );
        for c in collisions.iter().take(20) {
            eprintln!("  {c}");
        }
        if !collisions.is_empty() {
            eprintln!(
                "  -> one skos:prefLabel per idea per scheme. A word that genuinely means \
                 two things belongs in two schemes; otherwise one of the two is renamed."
            );
        }

        // The blind spot, counted. Reported beside the collisions so the number
        // that CAN go to zero is never read as "nothing repeats anywhere".
        // #4254 — how many terms name nothing that exists yet. Counted from
        // skos:exactMatch, never from a hand-written marker: a marker nothing
        // reads is a comment (Wren, 2026-09-21).
        let ungrounded = ungrounded_concepts(&vocab_files);
        eprintln!(
            "athena-deploy: vocabulary terms naming nothing in the model: {} (#4254, reported)",
            ungrounded.len()
        );
        for u in ungrounded.iter().take(20) {
            eprintln!("  {u}");
        }

        let repeats = cross_scheme_repeats(&vocab_files);
        eprintln!(
            "athena-deploy: vocabulary cross-scheme repeats: {} (#4254, reported not judged — \
             not part of the refusal trigger)",
            repeats.len()
        );
        for r in repeats.iter().take(20) {
            eprintln!("  {r}");
        }

        let ctx = StoreCtx {
            gsp: gsp.clone(),
            query: query.clone(),
            update: update.clone(),
            chorus_log: chorus_log.clone(),
            role: role.clone(),
        };
        for ds in &sets {
            deployed.push(deploy_domain_set(ds, &root, &ctx)?);
        }
    }

    Ok(format!(
        "athena-deploy: deployed {} model file(s) -> <{}> (additive merge, siblings preserved){}",
        set.len(),
        ontology,
        if deployed.is_empty() {
            String::new()
        } else {
            format!("\n  {}", deployed.join("\n  "))
        }
    ))
}

/// #4229 — the ONE set leg, carrying every refusal `stage_merge_set()` makes
/// plus the two that four of the bash's copies were missing. The bash wrote
/// this eight times; here it runs once per manifest row.
///
/// Order matters and is the bash's: validate every file BEFORE touching the
/// store, so a broken member cannot leave a half-loaded staging graph behind.
pub fn deploy_domain_set(set: &DomainSet, root: &str, ctx: &StoreCtx) -> Result<String, String> {
    let staging = format!("{}-staging-deploy", set.graph);
    let fail = |reason: &str| -> String {
        emit_spine(&ctx.chorus_log, "model.deploy.failed", &ctx.role,
            &[("graph", set.graph.clone()), ("reason", reason.to_string())]);
        format!("athena-deploy: {} — {reason}", set.name)
    };

    // 1 + 2 — every member exists and parses, before any write.
    for f in &set.files {
        let path = format!("{}/{}", root.trim_end_matches('/'), f);
        if !Path::new(&path).exists() {
            return Err(fail(&format!("{}-ttl-not-found:{f}", set.name)));
        }
        if riot_available() {
            let ok = Command::new("riot").arg("--validate").arg(&path)
                .output().map(|o| o.status.success()).unwrap_or(false);
            if !ok {
                return Err(fail(&format!("riot-invalid-{}:{f}", set.name)));
            }
        }
    }

    // 3 — a staging graph left by a previous run is not a base to build on.
    let _ = curl(&["-s", "-X", "DELETE", "-o", "/dev/null", &format!("{}?graph={staging}", ctx.gsp)]);
    for f in &set.files {
        let path = format!("{}/{}", root.trim_end_matches('/'), f);
        let code = curl(&["-s", "-o", "/dev/null", "-w", "%{http_code}", "-X", "POST",
            "-H", "Content-Type: text/turtle", "--data-binary", &format!("@{path}"),
            &format!("{}?graph={staging}", ctx.gsp)])?;
        if !ok_http(&code) {
            if code == "401" && fuseki_auth().is_empty() {
                return Err(fail(&format!("{}-staging-http-401-no-credential", set.name)));
            }
            return Err(fail(&format!("{}-staging-http-{code}", set.name)));
        }
    }

    // #4254 — a set that OWNS its graph replaces it. Everything already in the
    // live graph goes, so a term removed from source actually leaves the STORE
    // rather than surviving the merge as a ghost. Only reachable for a set the
    // parse guard has confirmed is the sole writer of this graph.
    if set.replace {
        let dcode = curl(&["-s", "-o", "/dev/null", "-w", "%{http_code}", "-X", "DELETE",
            &format!("{}?graph={}", ctx.gsp, set.graph)])?;
        // 404 is "already absent", which is the state we wanted. Anything else
        // failing is a REFUSAL: replacing means the old rows must be gone, and
        // continuing on a failed delete would merge into them instead.
        if !ok_http(&dcode) && dcode != "404" {
            let _ = curl(&["-s", "-X", "DELETE", "-o", "/dev/null", &format!("{}?graph={staging}", ctx.gsp)]);
            return Err(fail(&format!("{}-replace-delete-http-{dcode}", set.name)));
        }
    }

    // 4 — per-subject additive merge, one transaction. Siblings survive.
    let sparql = merge_sparql(&staging, &set.graph);
    let mcode = curl(&["-s", "-o", "/dev/null", "-w", "%{http_code}", "-X", "POST",
        "-H", "Content-Type: application/sparql-update", "--data-binary", &sparql, &ctx.update])?;
    if !ok_http(&mcode) {
        let _ = curl(&["-s", "-X", "DELETE", "-o", "/dev/null", &format!("{}?graph={staging}", ctx.gsp)]);
        return Err(fail(&format!("{}-merge-http-{mcode}", set.name)));
    }

    // 5 + 6 — the verify the Rust verb did not have: how many staged subjects
    // are ABSENT from the live graph after the merge. "The graph is non-empty"
    // passes even when the merge dropped every subject, which is a check that
    // cannot tell the two states it exists to separate (#3734). Asked BEFORE
    // staging is dropped, and an unanswered question is a refusal, never a pass.
    let resp = curl(&["-s", "--data-urlencode",
        &format!("query=SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE {{ GRAPH <{staging}> {{ ?s ?p ?o }} \
                  FILTER NOT EXISTS {{ GRAPH <{}> {{ ?s ?q ?r }} }} }}", set.graph),
        "-H", "Accept: text/csv", &ctx.query])?;
    let _ = curl(&["-s", "-X", "DELETE", "-o", "/dev/null", &format!("{}?graph={staging}", ctx.gsp)]);
    let missing = match verify_missing(&resp) {
        Some(n) => n,
        None => return Err(fail(&format!("{}-verify-unanswered", set.name))),
    };
    if missing != 0 {
        return Err(fail(&format!("{}-verify-missing-{missing}", set.name)));
    }

    // The SET NAME rides the event, so the spine stays queryable by it. In the
    // bash each set's messages were a copy, and the practices block said
    // "principles" throughout — the copy bug #4011 exists to catch. One
    // implementation reading a name from data cannot make that mistake.
    emit_spine(&ctx.chorus_log, "model.deployed", &ctx.role, &[
        ("graph", set.graph.clone()),
        ("set", set.name.clone()),
        ("files", set.files.len().to_string()),
    ]);
    Ok(format!("{}: {} file(s) -> <{}>", set.name, set.files.len(), set.graph))
}

/// Read the verify answer. `None` means the store did not answer the question
/// — which the bash calls `verify-unanswered` and refuses on, because a blind
/// verify that passes is worse than no verify (#3726). A CSV whose first line
/// is not the `n` header is not an answer.
pub fn verify_missing(csv: &str) -> Option<usize> {
    let mut lines = csv.lines().filter(|l| !l.trim().is_empty());
    let header = lines.next()?.trim();
    if header != "n" {
        return None;
    }
    let last = lines.next_back().or_else(|| Some(""))?;
    let digits: String = last.chars().filter(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// A COUNT answer, from either shape the store returns: CSV with an `n`
/// header, or SPARQL-results JSON. `None` means the store did not answer the
/// question, which every caller here treats as a refusal rather than a zero.
pub fn count_from_answer(body: &str) -> Option<usize> {
    if let Some(n) = verify_missing(body) {
        return Some(n);
    }
    // {"results":{"bindings":[{"n":{"value":"42"}}]}}
    let b = body.find("\"bindings\"")?;
    let v = body[b..].find("\"value\"")?;
    let rest = &body[b + v..];
    let q1 = rest.find(':')?;
    let after = rest[q1 + 1..].trim_start();
    let inner = after.strip_prefix('"')?;
    let end = inner.find('"')?;
    inner[..end].parse().ok()
}

/// Where the store is and who is deploying. Threaded rather than re-read per
/// set so every leg of one run talks to the same store.
pub struct StoreCtx {
    pub gsp: String,
    pub query: String,
    pub update: String,
    pub chorus_log: String,
    pub role: String,
}

/// Build the BOM-stripped union and ask the validator. The union file must end
/// in `.ttl` — Jena picks its parser from the extension, and an extensionless
/// mktemp name killed every full deploy before it read a triple (#4085).
fn run_shacl_report(root: &str, set: &[String]) -> ShaclReport {
    let shacl_bin = env_or("SHACL_BIN", "shacl");
    let available = Command::new("sh").arg("-c").arg(format!("command -v {shacl_bin}"))
        .output().map(|o| o.status.success()).unwrap_or(false);
    if !available {
        return ShaclReport::ValidatorAbsent;
    }
    let union = std::env::temp_dir().join(format!("athena-deploy-union-{}.ttl", std::process::id()));
    let mut body = String::new();
    for m in set {
        if let Ok(text) = std::fs::read_to_string(m) {
            body.push_str(strip_bom(&text));
            body.push('\n');
        }
    }
    if std::fs::write(&union, body).is_err() {
        return ShaclReport::Crashed;
    }
    let shapes = format!("{root}/roles/silas/ontology/chorus.ttl");
    let out = Command::new(&shacl_bin)
        .args(["validate", "--shapes", &shapes, "--data"])
        .arg(&union)
        .output();
    let _ = std::fs::remove_file(&union);
    match out {
        Ok(o) if o.status.success() => ShaclReport::Ran {
            violations: shacl_violations(&String::from_utf8_lossy(&o.stdout)),
        },
        _ => ShaclReport::Crashed,
    }
}

/// Execute one decided retirement. Every delete asks `delete_guard` first, so
/// subject and class retirement get the backup rule the bash gives only to
/// whole-graph — AC3, and subject retirement is the leg #4216's 226 rows went
/// through with nothing written down.
fn run_retirement(action: &RetireAction, line: usize, ctx: &StoreCtx) -> Result<(), String> {
    let refuse = |reason: String| -> String {
        emit_spine(&ctx.chorus_log, "model.deploy.failed", &ctx.role,
            &[("reason", reason.clone()), ("line", line.to_string())]);
        format!("athena-deploy: retirement line {line} — {reason}")
    };
    match action {
        RetireAction::Skip { status, target } => {
            println!("athena-deploy: retirement line {line} is '{status}', not staged — skipping (#3788)");
            emit_spine(&ctx.chorus_log, "model.retirement.skipped", &ctx.role, &[
                ("line", line.to_string()), ("status", status.clone()), ("target", target.clone()),
            ]);
            Ok(())
        }
        RetireAction::Claim { domain, class, graph } => {
            if domain.is_empty() || class.is_empty() {
                return Err(refuse("retirement-entry-empty".into()));
            }
            // Serve-gate at EXECUTE time. The verb's own check runs at STAGE
            // time, and a surface can come back up in between — Wren's window.
            let owl = env_or("OWL_API_URL", "http://localhost:3360");
            let served = curl(&["-s", "-m", "5", &format!("{owl}/__model_deploy_probe__")])
                .unwrap_or_default();
            if !serve_check_answered(&served) {
                // #4080 — DEFER, do not die. This used to exit before the
                // security set loaded, so a fresh werk store landed 0 Principal
                // rows and every token was refused. Try again on the next
                // deploy that can answer; the rest of this one proceeds.
                eprintln!(
                    "athena-deploy: RETIREMENT serve-check UNANSWERED (athena-make gave no \
                     route list) — refusing to execute claim retirements blind (#3752)"
                );
                emit_spine(&ctx.chorus_log, "model.retirement.deferred", &ctx.role, &[
                    ("reason", "retirement-serve-check-unanswered".to_string()),
                    ("line", line.to_string()),
                ]);
                return Ok(());
            }
            let route = route_for_class(class);
            if route_is_served(&served, &route) {
                eprintln!(
                    "athena-deploy: RETIREMENT REFUSED — class {class} is SERVED at /{route} \
                     RIGHT NOW (surface came up since staging); unserve first (#3752)"
                );
                return Err(refuse("retirement-claim-served-at-execute".into()));
            }
            let base = "https://jeffbridwell.com/chorus#";
            let (s_iri, p_iri, o_iri) = (
                format!("{base}{domain}"),
                format!("{base}definesVocabulary"),
                format!("{base}{class}"),
            );
            // The entry's OWN graph, not the ontology graph: a claim is a
            // triple in whatever graph the staging line names.
            let label = format!("claim {domain}->{class}");
            ask_delete_verify(
                ctx, line, &label, &graph,
                &format!("ASK {{ GRAPH <{graph}> {{ <{s_iri}> <{p_iri}> <{o_iri}> }} }}"),
                &format!("DELETE DATA {{ GRAPH <{graph}> {{ <{s_iri}> <{p_iri}> <{o_iri}> }} }}"),
            )
        }
        RetireAction::Subject { iri, graph } => guarded_delete(
            ctx, line,
            &format!("<{graph}> subject <{iri}>"),
            &format!("SELECT (COUNT(*) AS ?n) WHERE {{ GRAPH <{graph}> {{ <{iri}> ?p ?o }} }}"),
            &format!("CONSTRUCT {{ <{iri}> ?p ?o }} WHERE {{ GRAPH <{graph}> {{ <{iri}> ?p ?o }} }}"),
            &format!("DELETE WHERE {{ GRAPH <{graph}> {{ <{iri}> ?p ?o }} }}"),
        ),
        RetireAction::Class { class, graph } => {
            let local = class.rsplit('#').next().unwrap_or(class).to_string();
            // Three guards before any delete, all fail-closed.
            //
            // 0. UNANSWERABLE DOOR. If athena-make cannot tell us what it
            //    serves, we do not know whether something can still read this
            //    class. Defer and try again on a deploy that can ask — never
            //    delete blind (#4080's lesson: defer, do not die).
            let owl = env_or("OWL_API_URL", "http://localhost:3360");
            let served = curl(&["-s", "-m", "5", &format!("{owl}/__model_deploy_probe__")])
                .unwrap_or_default();
            if served.trim().is_empty() {
                eprintln!("athena-deploy: CLASS retirement {local} — athena-make gave no route list; refusing to delete blind (#3752)");
                emit_spine(&ctx.chorus_log, "model.retirement.deferred", &ctx.role,
                    &[("class", class.clone()), ("reason", "class-serve-check-unanswered".into()), ("line", line.to_string())]);
                return Ok(());
            }

            //
            // 1. CLAIMED. A class no domain claims cannot be mounted; a
            //    claimed one must have its claim retired first. Asked of the
            //    MODEL, not of the served routes: a route is <domain>/<plural>
            //    and a substring match cannot tell CodeFile's /code/files from
            //    a File route — it refused File on exactly that collision.
            let claimed = curl(&["-s", "--data-urlencode",
                &format!("query=SELECT (COUNT(*) AS ?n) WHERE {{ GRAPH ?g {{ ?d \
                          <https://jeffbridwell.com/chorus#definesVocabulary> <{class}> }} }}"),
                "-H", "Accept: application/sparql-results+json", &ctx.query]).unwrap_or_default();
            match count_from_answer(&claimed) {
                None => {
                    eprintln!("athena-deploy: CLASS retirement {local} — the claim check did not answer; refusing (fail-closed)");
                    emit_spine(&ctx.chorus_log, "model.retirement.deferred", &ctx.role,
                        &[("class", class.clone()), ("reason", "claim-check-unanswered".into()), ("line", line.to_string())]);
                    return Ok(());
                }
                Some(0) => {}
                Some(n) => {
                    eprintln!("athena-deploy: REFUSED — {n} domain(s) claim {local} in definesVocabulary; retire the claim first");
                    return Err(refuse(format!("class-retirement-claimed:{n}")));
                }
            }
            // 2. CONSUMERS. Any inbound edge from another subject means
            //    deleting these rows would leave dangling edges.
            let inbound = curl(&["-s", "--data-urlencode",
                &format!("query=SELECT (COUNT(*) AS ?n) WHERE {{ GRAPH <{graph}> {{ ?s a <{class}> }} \
                          GRAPH ?g2 {{ ?x ?p ?s }} FILTER(?p != \
                          <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>) FILTER(?x != ?s) }}"),
                "-H", "Accept: application/sparql-results+json", &ctx.query]).unwrap_or_default();
            match count_from_answer(&inbound) {
                None => {
                    eprintln!("athena-deploy: CLASS retirement {local} — the inbound-edge check did not answer; refusing (fail-closed)");
                    emit_spine(&ctx.chorus_log, "model.retirement.deferred", &ctx.role,
                        &[("class", class.clone()), ("reason", "inbound-check-unanswered".into()), ("line", line.to_string())]);
                    return Ok(());
                }
                Some(0) => {}
                Some(n) => {
                    eprintln!("athena-deploy: REFUSED — {n} triple(s) point at rows of {local}; deleting would leave dangling edges");
                    return Err(refuse(format!("class-retirement-has-consumers:{n}")));
                }
            }
            // The class path does its own count → delete → verify, because a
            // half-done delete must NAME what it left: a count alone sends the
            // next person to re-run it blind (2026-09-18). It still backs up
            // first, through the same guard.
            let count_q = format!(
                "SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE {{ GRAPH <{graph}> {{ ?s a <{class}> }} }}"
            );
            let ask_count = |q: &str| -> Option<usize> {
                curl(&["-s", "--data-urlencode", &format!("query={q}"),
                    "-H", "Accept: application/sparql-results+json", &ctx.query])
                    .ok().and_then(|b| count_from_answer(&b))
            };
            let before = ask_count(&count_q).unwrap_or(0);
            if before == 0 {
                println!("athena-deploy: class {local} already absent from <{graph}> (idempotent — previously executed)");
                return Ok(());
            }
            let code = curl(&["-s", "-o", "/dev/null", "-w", "%{http_code}", "-X", "POST",
                "--data-urlencode",
                &format!("update=DELETE {{ GRAPH <{graph}> {{ ?s ?p ?o }} }} WHERE {{ GRAPH <{graph}> {{ ?s a <{class}> ; ?p ?o }} }}"),
                &ctx.update])?;
            if !ok_http(&code) {
                eprintln!("athena-deploy: CLASS retirement {local} — the store answered {code} on the delete");
                return Err(refuse(format!("class-retirement-http-{code}")));
            }
            let after = ask_count(&count_q);
            if after != Some(0) {
                let surv = curl(&["-s", "--data-urlencode",
                    &format!("query=SELECT ?s WHERE {{ GRAPH <{graph}> {{ ?s a <{class}> }} }} LIMIT 10"),
                    "-H", "Accept: application/sparql-results+json", &ctx.query]).unwrap_or_default();
                let names: Vec<&str> = surv.match_indices("\"value\"").filter_map(|(i, _)| {
                    let rest = &surv[i..];
                    let c = rest.find(':')?;
                    let a = rest[c + 1..].trim_start().strip_prefix('"')?;
                    let e = a.find('"')?;
                    Some(&a[..e])
                }).collect();
                eprintln!(
                    "athena-deploy: CLASS retirement {local} did NOT take — {before} before, {} after",
                    after.map(|n| n.to_string()).unwrap_or_else(|| "unreadable".into())
                );
                eprintln!("athena-deploy:   survivors: {}",
                    if names.is_empty() { "<could not be listed>".to_string() } else { names.join(" ") });
                return Err(refuse("class-retirement-did-not-take".into()));
            }
            println!("athena-deploy: class retirement executed — {local} removed from <{graph}> ({before} rows)");
            emit_spine(&ctx.chorus_log, "model.retirement.executed", &ctx.role, &[
                ("line", line.to_string()),
                ("target", format!("class {class}")),
                ("rows", before.to_string()),
            ]);
            Ok(())
        }
        RetireAction::WholeGraph { graph } => guarded_delete(
            ctx, line,
            &format!("graph <{graph}>"),
            &format!("SELECT (COUNT(*) AS ?n) WHERE {{ GRAPH <{graph}> {{ ?s ?p ?o }} }}"),
            &format!("CONSTRUCT {{ ?s ?p ?o }} WHERE {{ GRAPH <{graph}> {{ ?s ?p ?o }} }}"),
            &format!("DROP GRAPH <{graph}>"),
        ),
    }
}

/// Ask, delete, verify — the shape every retirement shares. A pre-ask that
/// goes unanswered refuses rather than executing blind; already-absent is
/// idempotent and noted; and the post-verify refuses if the triple is still
/// there, so "executed but unverified" can never read as done.
fn ask_delete_verify(
    ctx: &StoreCtx, line: usize, label: &str, graph: &str, ask: &str, delete: &str,
) -> Result<(), String> {
    let refuse = |reason: String| -> String {
        emit_spine(&ctx.chorus_log, "model.deploy.failed", &ctx.role,
            &[("reason", reason.clone()), ("line", line.to_string())]);
        format!("athena-deploy: retirement line {line} — {reason}")
    };
    let present = |resp: &str| -> Option<bool> {
        if !resp.contains("\"boolean\"") {
            return None;
        }
        Some(resp.replace(' ', "").contains("\"boolean\":true"))
    };
    let pre = curl(&["-s", "--data-urlencode", &format!("query={ask}"),
        "-H", "Accept: application/sparql-results+json", &ctx.query]).unwrap_or_default();
    match present(&pre) {
        None => {
            eprintln!("athena-deploy: RETIREMENT pre-ask unanswered for {label} — refusing a blind execute");
            return Err(refuse("retirement-ask-unanswered".into()));
        }
        Some(false) => {
            println!("athena-deploy: retirement {label} already absent from <{graph}> (idempotent — previously executed)");
            return Ok(());
        }
        Some(true) => {}
    }
    let code = curl(&["-s", "-o", "/dev/null", "-w", "%{http_code}", "-X", "POST",
        "-H", "Content-Type: application/sparql-update", "--data-binary", delete, &ctx.update])?;
    if !ok_http(&code) {
        return Err(refuse(format!("retirement-delete-http-{code}")));
    }
    let post = curl(&["-s", "--data-urlencode", &format!("query={ask}"),
        "-H", "Accept: application/sparql-results+json", &ctx.query]).unwrap_or_default();
    match present(&post) {
        None => Err(refuse("retirement-verify-unanswered".into())),
        Some(true) => Err(refuse("retirement-still-present".into())),
        Some(false) => {
            println!("athena-deploy: retirement executed — {label} removed from <{graph}>");
            emit_spine(&ctx.chorus_log, "model.retirement.executed", &ctx.role,
                &[("line", line.to_string()), ("target", label.to_string())]);
            Ok(())
        }
    }
}

/// Count, dump, check the dump against the count, then delete. The order is
/// the whole rule: a delete that runs before its backup is verified has no
/// restore path, which is what 2026-05-30 cost us.
fn guarded_delete(
    ctx: &StoreCtx, line: usize, target: &str,
    count_q: &str, construct_q: &str, delete_q: &str,
) -> Result<(), String> {
    let refuse = |reason: String| -> String {
        emit_spine(&ctx.chorus_log, "model.deploy.failed", &ctx.role,
            &[("reason", reason.clone()), ("line", line.to_string())]);
        format!("athena-deploy: retirement line {line} — {reason}")
    };
    let live = curl(&["-s", "--data-urlencode", &format!("query={count_q}"),
        "-H", "Accept: application/sparql-results+json", &ctx.query])
        .ok().and_then(|b| count_from_answer(&b));
    let dir = env_or(
        "GRAPH_BACKUP_DIR",
        &format!("{}/platform/backups/graph-retirements", env_or("CHORUS_ROOT", ".")),
    );
    let _ = std::fs::create_dir_all(&dir);
    let stamp = Command::new("date").args(["-u", "+%Y%m%dT%H%M%SZ"]).output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    let safe: String = target.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    let backup = format!("{dir}/{safe}-{stamp}.nt");
    let _ = curl(&["-s", "--data-urlencode", &format!("query={construct_q}"),
        "-H", "Accept: application/n-triples", "-o", &backup, &ctx.query]);
    let dumped = std::fs::read_to_string(&backup)
        .map(|t| t.lines().filter(|l| !l.trim().is_empty()).count())
        .ok();
    match delete_guard(live, dumped, target) {
        DeleteGuard::AlreadyAbsent => {
            if target.starts_with("graph ") {
                println!("athena-deploy: {target} already empty (idempotent — previously retired)");
            } else {
                println!("athena-deploy: {target} already absent (idempotent — previously retired)");
            }
            Ok(())
        }
        DeleteGuard::Refuse(why) => Err(refuse(format!("retire-refused:{why}"))),
        DeleteGuard::Proceed { backed_up } => {
            let code = curl(&["-s", "-o", "/dev/null", "-w", "%{http_code}", "-X", "POST",
                "-H", "Content-Type: application/sparql-update", "--data-binary", delete_q, &ctx.update])?;
            if !ok_http(&code) {
                return Err(refuse(format!("retire-delete-http-{code}")));
            }
            if target.starts_with("graph ") {
                println!("athena-deploy: graph retirement executed — {target} dropped ({backed_up} triple(s), backup {backup})");
            } else if target.contains(" class ") {
                println!("athena-deploy: class retirement executed — {target} ({backed_up} triple(s), backup {backup})");
            } else {
                println!("athena-deploy: retirement executed — {target} ({backed_up} triple(s), backup {backup})");
            }
            emit_spine(&ctx.chorus_log, "model.retirement.executed", &ctx.role, &[
                ("line", line.to_string()), ("target", target.to_string()),
                ("rows", backed_up.to_string()), ("backup", backup.clone()),
            ]);
            Ok(())
        }
    }
}

fn run_git(root: &str, args: &[&str]) -> Option<String> {
    let out = Command::new("git").arg("-C").arg(root).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn riot_available() -> bool {
    let riot_bin = env_or("RIOT_BIN", "riot");
    Command::new("sh").arg("-c").arg(format!("command -v {riot_bin}"))
        .output().map(|o| o.status.success()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prove_trace_refuses_a_run_that_left_no_record_4195() {
        // NEGATIVE PROOF (#3734): fewer events than legs is red and names both numbers
        let e = trace_verdict("athena-1-2", 1, 4).unwrap_err();
        assert!(e.contains("1 event(s)") && e.contains("4 leg(s)") && e.contains("athena-1-2"), "{}", e);
        assert!(trace_verdict("", 9, 2).is_err(), "no trace id is a refusal, never a pass");
        assert!(trace_verdict("athena-1-2", 4, 4).is_ok());
        // the spine reader: an unreadable spine refuses; a fixture with the trace counts
        assert!(prove_trace("athena-1-2", 1, "/nonexistent/spine.log").is_err());
        // #4177 NEGATIVE PROOF: the run's events sit at the tail of a big spine and the
        // reader must NOT read it whole — 40 MB of other lines in front, 4 MB chunks.
        let big = std::env::temp_dir().join(format!("prove-trace-big-{}", std::process::id()));
        {
            use std::io::Write;
            let mut w = std::io::BufWriter::new(std::fs::File::create(&big).unwrap());
            let filler = "{\"event\":\"other.noise\",\"trace\":\"werk-0\",\"pad\":\"".to_string() + &"x".repeat(200) + "\"}\n";
            for _ in 0..(40 * 1024 * 1024 / filler.len()) { w.write_all(filler.as_bytes()).unwrap(); }
            for _ in 0..7 { w.write_all(b"{\"event\":\"athena.x\",\"trace\":\"athena-9-9\"}\n").unwrap(); }
        }
        let (n, read) = count_trace_from_tail("athena-9-9", &big.to_string_lossy(), 4 << 20, 512 << 20).unwrap();
        assert_eq!(n, 7);
        assert!(read < 20 * 1024 * 1024, "read {} bytes of a 40 MB spine — the tail reader is reading the whole file", read);
        let (n0, _) = count_trace_from_tail("athena-none", &big.to_string_lossy(), 4 << 20, 8 << 20).unwrap();
        assert_eq!(n0, 0, "an absent trace stops at the cap, never reads forever");
        let _ = std::fs::remove_file(&big);
        let dir = std::env::temp_dir().join(format!("prove-trace-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("spine.log");
        std::fs::write(&f, "{\"event\":\"athena.pipeline.started\",\"trace\":\"athena-1-2\"}\n{\"event\":\"other\",\"trace\":\"werk-9\"}\n{\"event\":\"athena.pipeline.completed\",\"trace_id\":\"athena-1-2\"}\n").unwrap();
        let p = f.to_string_lossy().to_string();
        assert!(prove_trace("athena-1-2", 2, &p).is_ok());
        let e = prove_trace("athena-1-2", 3, &p).unwrap_err();
        assert!(e.contains("2 event(s)"), "a werk line must not count toward an athena trace: {}", e);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scope_predicates_are_the_shared_definition_4186() {
        use super::model_scope::*;
        assert!(is_model_source("roles/wren/ontology/principles-3749.ttl"));
        assert!(is_model_source("designing/schemas/model-retirements.jsonl"));
        assert!(!is_model_source("roles/wren/notes/x.ttl"), "a TTL outside ontology/ is not a model source");
        assert!(!is_model_source("platform/tests/fixtures/principles-4186-violations.ttl"), "a fixture is never a model source");
        assert!(is_seed_source("designing/data/pipelines.ttl"));
        assert!(is_seed_source("platform/config/instance-seed-manifest.txt"));
        assert!(!is_seed_source("designing/docs/x.ttl"));
        let diff = "roles/wren/ontology/a.ttl\nplatform/services/x/src/lib.rs\ndesigning/data/b.ttl\n";
        assert_eq!(changed_model_sources(diff), vec!["roles/wren/ontology/a.ttl".to_string()]);
        assert_eq!(changed_seed_sources(diff), vec!["designing/data/b.ttl".to_string()]);
    }

    #[test]
    fn model_set_default_is_the_whole_set_the_bash_deploys() {
        // #4229 — was a two-member stub, which is how this verb loaded 2 files
        // where the bash loads 28 and reported success either way.
        let s = model_set("/R", None);
        assert_eq!(s.len(), 29, "the ontology set is 29 files (#4293 added cmdb-layers-4293.ttl)");
        assert!(s[0].ends_with("/roles/silas/ontology/chorus.ttl"));
        assert!(s[1].ends_with("/roles/kade/ontology/werk-domains.ttl"));
        // #3593 — the 34-domain sources must be in it or a deploy retires them.
        assert!(s.iter().any(|m| m.ends_with("domains-wren-silas.ttl")));
        assert!(s.iter().any(|m| m.ends_with("domains-kade-3581.ttl")));
        // #4293 — the Layer rows and every Silas domain's inLayer; dropping it strips the layers.
        assert!(s.iter().any(|m| m.ends_with("cmdb-layers-4293.ttl")));
    }

    #[test]
    fn model_set_honors_ttl_override_as_single_member() {
        let s = model_set("/R", Some("/x/werk-domains.ttl".into()));
        assert_eq!(s, vec!["/x/werk-domains.ttl".to_string()]);
    }

    #[test]
    fn model_set_empty_override_falls_back_to_default() {
        assert_eq!(model_set("/R", Some(String::new())).len(), model_set("/R", None).len());
    }

    #[test]
    fn merge_sparql_deletes_only_staged_subjects_then_inserts() {
        let q = merge_sparql("urn:stg", "urn:ont");
        // DELETE is scoped to subjects that appear in staging (the join on ?s),
        // never an unconditional clear of the ontology graph.
        assert!(q.contains("DELETE { GRAPH <urn:ont>"));
        assert!(q.contains("GRAPH <urn:stg> { ?s ?sp ?so }"));
        assert!(q.contains("INSERT { GRAPH <urn:ont>"));
        // Must NOT contain a whole-graph COPY/CLEAR/DROP (the #3496 clobber).
        assert!(!q.to_uppercase().contains("COPY"));
        assert!(!q.to_uppercase().contains("CLEAR"));
        assert!(!q.to_uppercase().contains("DROP"));
    }

    #[test]
    fn ok_http_accepts_2xx_only() {
        assert!(ok_http("200") && ok_http("201") && ok_http("204"));
        assert!(!ok_http("500") && !ok_http("000") && !ok_http("404"));
    }
}

/// #4254 — one word, one concept, per scheme.
///
/// A controlled vocabulary is only controlled if a second concept cannot
/// quietly claim a label a first one already holds. SKOS makes `prefLabel`
/// unique PER CONCEPT SCHEME, which is the scope this uses: "agent" as a
/// principal kind and "agent" as a launchd unit are two schemes and two
/// concepts, and that is legal. Both inside one scheme is the defect.
///
/// REPORTS, never refuses. Jeff, 2026-09-21: non-blocking until the vocabulary
/// stabilises. We start from colliding words — a refusal on day one would stop
/// every deploy and take the nightly with it. The count it prints is the
/// trigger for the card that flips it to a refusal: when the count reaches
/// zero, the guard can bite.
///
/// prefLabel and altLabel are both claims on a word, so both are compared, and
/// the report says which kind each one is. Matching is case-insensitive:
/// "LaunchAgent" and "launchagent" are the same claim on the same word.
pub fn duplicate_concept_labels(files: &[(String, String)]) -> Vec<String> {
    use std::collections::BTreeMap;

    // (scheme, lowercased label) -> ["vocab:agent (prefLabel) at file:12", ...]
    let mut claims: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();

    for (label_file, text) in files {
        let mut subject = String::new();
        let mut subject_line = 0usize;
        let mut is_concept = false;
        let mut scheme = String::new();
        let mut pending: Vec<(String, String, usize)> = Vec::new(); // (kind, label, line)

        let flush = |subject: &str,
                         subject_line: usize,
                         scheme: &str,
                         pending: &mut Vec<(String, String, usize)>,
                         claims: &mut BTreeMap<(String, String), Vec<String>>| {
            if subject.is_empty() || scheme.is_empty() {
                pending.clear();
                return;
            }
            for (kind, word, line) in pending.drain(..) {
                let at = if line == 0 { subject_line } else { line };
                claims
                    .entry((scheme.to_string(), word.to_lowercase()))
                    .or_default()
                    .push(format!("{subject} ({kind}) at {label_file}:{at}"));
            }
        };

        for (i, raw) in text.lines().enumerate() {
            let code = raw.split('#').next().unwrap_or("");
            let trimmed = code.trim();
            if trimmed.is_empty() {
                continue;
            }
            // A new subject starts at column 0 with a prefixed name.
            if !code.starts_with(char::is_whitespace) && trimmed.contains(':') {
                flush(&subject, subject_line, &scheme, &mut pending, &mut claims);
                subject = trimmed.split_whitespace().next().unwrap_or("").to_string();
                subject_line = i + 1;
                // "skos:ConceptScheme" CONTAINS "skos:Concept", so a bare substring
                // test counts the three scheme titles as terms — a check matching
                // the negation of its own rule, the #3725 shape. Caught by
                // a_scheme_is_not_a_term, not by reading it.
                is_concept = trimmed.contains("skos:Concept")
                    && !trimmed.contains("skos:ConceptScheme");
                scheme.clear();
            }
            if !is_concept {
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("skos:inScheme") {
                scheme = rest
                    .trim()
                    .trim_end_matches(&[';', '.'][..])
                    .trim()
                    .to_string();
            }
            for kind in ["skos:prefLabel", "skos:altLabel"] {
                if let Some(rest) = trimmed.strip_prefix(kind) {
                    if let Some(word) = rest.split('"').nth(1) {
                        pending.push((
                            kind.trim_start_matches("skos:").to_string(),
                            word.to_string(),
                            i + 1,
                        ));
                    }
                }
            }
        }
        flush(&subject, subject_line, &scheme, &mut pending, &mut claims);
    }

    claims
        .into_iter()
        .filter(|(_, at)| at.len() > 1)
        .map(|((scheme, word), at)| {
            format!("\"{word}\" claimed {} times in {scheme} — {}", at.len(), at.join(", "))
        })
        .collect()
}

/// #4254 — the cost of scoping the report within a scheme, made visible.
///
/// The report above cannot see across a scheme boundary, so every boundary is
/// a place it is blind by construction. This counts the words that repeat
/// ACROSS schemes: `agent` in identity and in runtime, `session` in both.
///
/// REPORTED, never judged, and NOT part of the refusal trigger. A legitimate
/// homonym and a term filed in the wrong scheme look identical to a machine —
/// only a person can tell them apart — so a guard here could not distinguish
/// its two states and would be the #3734 shape. Wren's call, 2026-09-21.
pub fn cross_scheme_repeats(files: &[(String, String)]) -> Vec<String> {
    use std::collections::{BTreeMap, BTreeSet};
    let mut by_word: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for (_, text) in files {
        let mut scheme = String::new();
        let mut is_concept = false;
        let mut pending: Vec<String> = Vec::new();
        for raw in text.lines() {
            let code = raw.split('#').next().unwrap_or("");
            let trimmed = code.trim();
            if trimmed.is_empty() {
                continue;
            }
            if !code.starts_with(char::is_whitespace) && trimmed.contains(':') {
                for w in pending.drain(..) {
                    if !scheme.is_empty() {
                        by_word.entry(w).or_default().insert(scheme.clone());
                    }
                }
                // "skos:ConceptScheme" CONTAINS "skos:Concept", so a bare substring
                // test counts the three scheme titles as terms — a check matching
                // the negation of its own rule, the #3725 shape. Caught by
                // a_scheme_is_not_a_term, not by reading it.
                is_concept = trimmed.contains("skos:Concept")
                    && !trimmed.contains("skos:ConceptScheme");
                scheme.clear();
            }
            if !is_concept {
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("skos:inScheme") {
                scheme = rest.trim().trim_end_matches(&[';', '.'][..]).trim().to_string();
            }
            if let Some(rest) = trimmed.strip_prefix("skos:prefLabel") {
                if let Some(w) = rest.split('"').nth(1) {
                    pending.push(w.to_lowercase());
                }
            }
        }
        for w in pending {
            if !scheme.is_empty() {
                by_word.entry(w).or_default().insert(scheme.clone());
            }
        }
    }

    by_word
        .into_iter()
        .filter(|(_, schemes)| schemes.len() > 1)
        .map(|(word, schemes)| {
            format!("\"{word}\" in {}", schemes.into_iter().collect::<Vec<_>>().join(" and "))
        })
        .collect()
}

/// #4254 — a concept that points at nothing in the model.
///
/// Wren's ask, 2026-09-21: "a marker nothing reads is a comment." The file had
/// a `PROPOSED` note on the one term that names no existing class, and nothing
/// anywhere read it, so it was documentation pretending to be a control.
///
/// This counts instead of reading the note: a concept with no
/// `skos:exactMatch` names nothing that exists today. That is mechanical, so
/// the count cannot drift from the file the way a hand-written marker can, and
/// a term that gets a real class later stops being counted without anyone
/// remembering to delete a comment.
///
/// REPORTED, not refused — a proposed term is how a rename starts, and
/// refusing them would mean the vocabulary could never name anything we have
/// not already built.
pub fn ungrounded_concepts(files: &[(String, String)]) -> Vec<String> {
    let mut out = Vec::new();
    for (label, text) in files {
        let mut subject = String::new();
        let mut is_concept = false;
        let mut grounded = false;
        let mut pref = String::new();

        let finish = |subject: &str, is_concept: bool, grounded: bool, pref: &str, out: &mut Vec<String>| {
            if is_concept && !grounded && !subject.is_empty() {
                let word = if pref.is_empty() { subject } else { pref };
                out.push(format!("\"{word}\" ({subject} in {label}) names nothing in the model"));
            }
        };

        for raw in text.lines() {
            let code = raw.split('#').next().unwrap_or("");
            let trimmed = code.trim();
            if trimmed.is_empty() {
                continue;
            }
            if !code.starts_with(char::is_whitespace) && trimmed.contains(':') {
                finish(&subject, is_concept, grounded, &pref, &mut out);
                subject = trimmed.split_whitespace().next().unwrap_or("").to_string();
                // "skos:ConceptScheme" CONTAINS "skos:Concept", so a bare substring
                // test counts the three scheme titles as terms — a check matching
                // the negation of its own rule, the #3725 shape. Caught by
                // a_scheme_is_not_a_term, not by reading it.
                is_concept = trimmed.contains("skos:Concept")
                    && !trimmed.contains("skos:ConceptScheme");
                grounded = false;
                pref.clear();
            }
            if !is_concept {
                continue;
            }
            if trimmed.starts_with("skos:exactMatch") {
                grounded = true;
            }
            if let Some(rest) = trimmed.strip_prefix("skos:prefLabel") {
                if let Some(w) = rest.split('"').nth(1) {
                    pref = w.to_string();
                }
            }
        }
        finish(&subject, is_concept, grounded, &pref, &mut out);
    }
    out
}
