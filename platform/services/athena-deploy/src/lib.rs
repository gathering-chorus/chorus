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
        _ => vec![
            format!("{root}/roles/silas/ontology/chorus.ttl"),
            format!("{root}/roles/kade/ontology/werk-domains.ttl"),
        ],
    }
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
        if parts.len() != 3 || parts.iter().any(|p| p.is_empty()) {
            return Err(format!(
                "domain-set-manifest line {}: expected <set>|<graph>|<path>, got {:?}",
                i + 1,
                line
            ));
        }
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
            }
            None => out.push(DomainSet {
                name: name.to_string(),
                graph: graph.to_string(),
                files: vec![path.to_string()],
            }),
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
    let out = Command::new("curl")
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
    let ttl_override = std::env::var("TTL").ok();
    let set = model_set(&root, ttl_override);
    let staging = format!("{ontology}-staging-deploy");

    let fail = |reason: &str| -> String {
        emit_spine(&chorus_log, "athena.deploy.failed", &role,
            &[("graph", ontology.clone()), ("reason", reason.to_string())]);
        format!("athena-deploy: {reason}")
    };

    // Validate every member exists + is riot-valid (don't deploy a broken model).
    for ttl in &set {
        if !Path::new(ttl).exists() {
            return Err(fail(&format!("ttl-not-found:{ttl}")));
        }
        if riot_available() {
            let status = Command::new("riot").arg("--validate").arg(ttl)
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

    // Step 2: additive merge (delete-staged-subjects-then-insert), one transaction.
    let sparql = merge_sparql(&staging, &ontology);
    let mcode = curl(&["-s", "-o", "/dev/null", "-w", "%{http_code}", "-X", "POST",
        "-H", "Content-Type: application/sparql-update", "--data-binary", &sparql, &update])?;
    let _ = curl(&["-s", "-X", "DELETE", "-o", "/dev/null", &format!("{gsp}?graph={staging}")]);
    if !ok_http(&mcode) {
        return Err(fail(&format!("merge-http-{mcode}")));
    }

    // Step 3: verify the graph is non-empty (proof, not assumption).
    let body = curl(&["-s", "--data-urlencode",
        &format!("query=ASK {{ GRAPH <{ontology}> {{ ?s ?p ?o }} }}"),
        "-H", "Accept: application/sparql-results+json", &query])?;
    if !body.replace(' ', "").contains("\"boolean\":true") {
        return Err(fail("verify-empty"));
    }

    emit_spine(&chorus_log, "athena.deployed", &role,
        &[("graph", ontology.clone()), ("members", set.len().to_string())]);
    // #3561 compatibility — everything watching the deploy reads the bash's
    // names. Both pairs are emitted until every reader is repointed; dropping
    // one at the swap is how a dashboard goes blind without anyone noticing.
    emit_spine(&chorus_log, "model.deployed", &role,
        &[("graph", ontology.clone()), ("members", set.len().to_string())]);

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

    emit_spine(&ctx.chorus_log, "model.deployed", &ctx.role,
        &[("graph", set.graph.clone()), ("files", set.files.len().to_string())]);
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

/// Where the store is and who is deploying. Threaded rather than re-read per
/// set so every leg of one run talks to the same store.
pub struct StoreCtx {
    pub gsp: String,
    pub query: String,
    pub update: String,
    pub chorus_log: String,
    pub role: String,
}

fn riot_available() -> bool {
    Command::new("sh").arg("-c").arg("command -v riot")
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
    fn model_set_default_is_the_two_member_set() {
        let s = model_set("/R", None);
        assert_eq!(s.len(), 2);
        assert!(s[0].ends_with("/roles/silas/ontology/chorus.ttl"));
        assert!(s[1].ends_with("/roles/kade/ontology/werk-domains.ttl"));
    }

    #[test]
    fn model_set_honors_ttl_override_as_single_member() {
        let s = model_set("/R", Some("/x/werk-domains.ttl".into()));
        assert_eq!(s, vec!["/x/werk-domains.ttl".to_string()]);
    }

    #[test]
    fn model_set_empty_override_falls_back_to_default() {
        assert_eq!(model_set("/R", Some(String::new())).len(), 2);
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
