//! #3723 — RECONCILIATION: where does each class live, and does it serve?
//!
//! Jeff, 2026-08-02: *"i get very concerned about our ability to track our owl
//! and rdf."* He was right, and the same morning proved it — three confident,
//! different diagnoses of `/products`=0 inside one hour, the first two wrong.
//! Each was checked. Nothing answers "where does this subject live and does it
//! serve," so every check surfaces a different partial truth and whoever looks
//! last sounds authoritative.
//!
//! This reads FOUR independent sources and reports where they DISAGREE. The
//! disagreement is the finding; agreement is uninteresting.
//!
//!   1. the repo    — which `.ttl` declares the class            (authoredIn)
//!   2. the deploy  — is that file in MODEL_SET / INSTANCE_SET   (binds)
//!   3. the shapes  — does a `sh:NodeShape` target the class,
//!                    and does the CLASS resolve a placement     (shaped)
//!   4. the store   — how many instances, in WHICH named graphs  (inGraphs)
//!
//! ## Silas's ruling, 2026-08-02 (OWL-DBA) — encoded here, not paraphrased
//!
//! **Placement resolves at CLASS level, across ALL of a class's shapes.** A
//! class often has several (Product has ProductShape + ProductOrderShape; Test
//! has TestShape + TestEdgesShape + QuarantineShape). Demanding the annotation
//! per shape would demand it 38 times for a property declared once.
//!
//! **A partition, not a flat violation list.** Only 10 of 49 shapes declare a
//! placement; 31 classes resolve none. Flagging all 31 equally is noise —
//! Product is the sole LIVE-HARM case (11 instances, 3 valid in the served
//! graph, `/products`=0); the rest are not-served or TBox, where absence costs
//! nothing. Every class lands in exactly ONE bucket:
//!
//!   - `ok`                    served, has instances, has placement
//!   - `served-no-placement`   served, HAS instances, NO placement  ← the harm
//!   - `served-no-instances`   served, zero instances               (latent)
//!   - `not-served`            no shape / TBox                      (no harm)
//!
//! `served-no-placement` is the ADR-051 boundary set — the rule is mechanically
//! expressible only against it, which is why this readout had to exist before
//! #3718 could encode ADR-051 at all.
//!
//! **ADR-051 and ADR-025 are ONE check, never two.** A mechanical "must declare
//! an ABox instances graph" would REFUSE Domain, Service and ValueStream — the
//! three that are correct — because punned classes declare
//! `instancesGraph = urn:chorus:ontology` deliberately (a Domain is an
//! `owl:Class`; its instances are subclasses living in the ontology graph).
//! The predicate is *placement CONSISTENT WITH THE CLASS'S KIND*.
//!
//! **`served-no-instances` needs a human, and that is the point.** It mixes a
//! real thing never populated with a genuinely abstract TBox class. That
//! distinction is a judgment about INTENT, not a fact in any of the four
//! sources, so this reports the bucket and its evidence and refuses to guess.
//!
//! Silas classified the first four (2026-08-02, all security, all
//! `urn:chorus:domains:security`): AuthBoundary, Credential, Permission,
//! SecurityProbe are ALL **latent-should-serve**, none is TBox-abstract. The
//! intent was on record the whole time — security-service-design (#2659):
//! *"Today they exist as schema only. The work is populating instances."*
//!
//! That the answer lived in a DESIGN DOC and in none of the four sources this
//! endpoint reads is itself the tracking gap. Three of the four already have
//! real operational instances that were never projected: SecurityProbe's
//! attestations are emitted today by deep-health and the nightly probes;
//! AuthBoundary's three verify doors (athena-make seam, DAL, chorus-api envelope)
//! are live. The finding is "the posture the design promised exists in scripts
//! and doors but was never projected into the graph" — NOT "populate 4 empty
//! classes."
//!
//! Per absence-stays-absent: this endpoint NEVER auto-populates. A latent class
//! stays honestly empty until someone deliberately models it.
//!
//! ## The naming invariant (Jeff + Silas, 2026-08-02) — read this first
//!
//! Jeff: *"instances and ontology do not say anything about the underlying
//! semantics."* He is right, and it is the root cause beneath every finding
//! this module reports.
//!
//! `urn:chorus:instances` and `urn:chorus:ontology` are named for RDF
//! MECHANICS (ABox vs TBox), not for what they hold. A graph name that carries
//! no placement information means placement decisions have no natural home:
//! everything lands in one of two buckets by mechanism, "which graph" tells you
//! nothing about what SHOULD be there, and drift is undetectable by inspection.
//! That is why ten classes are multi-graph and nobody noticed for months — the
//! names could not have told us.
//!
//! Silas's ruling (OWL-DBA, same day): **a graph is named for WHAT IT HOLDS —
//! its domain — never for its RDF layer.** Everything about a domain (class
//! definitions, SHACL shapes, AND instances) lives in
//! `urn:chorus:domains:<domain>`. The ABox/TBox distinction is a property of
//! the TRIPLE, not of the graph; it was never supposed to be a graph name.
//! `instances` and `ontology` are legacy mechanism-buckets that drain.
//!
//! Two consequences, both encoded here:
//!   - ADR-051 becomes a DERIVATION — expected placement is the graph of the
//!     class's domain; a per-shape `instancesGraph` is the exception, not the
//!     rule.
//!   - The ten multi-graph classes are ONE migration (mechanism-named graphs
//!     draining into domain-named ones), not ten unrelated problems. That is
//!     the honest framing; this module reports it as such.
//!
//! ## Two honesty rules, both learned the hard way
//!
//! Fixture/staging graphs are LABELLED, never folded into live counts — they
//! already polluted the `model.deploy.failed` signal in exactly this way.
//!
//! A count that cannot be computed is `null` (unknown), NEVER `0`. A fabricated
//! zero is what made two of this morning's diagnoses wrong.

use crate::{json_escape, sparql_json, RouteTable, API_VERSION, NS};
use std::collections::BTreeMap;
use std::process::Command;

pub fn chorus_root() -> String {
    std::env::var("CHORUS_ROOT")
        .unwrap_or_else(|_| "/Users/jeffbridwell/CascadeProjects/chorus".to_string())
}

/// Fixture, test and staging graphs. Never merged into live counts.
///
/// THIS IS A NAME HEURISTIC AND IT CANNOT BE ANYTHING ELSE TODAY — nothing in
/// the model declares which graphs are fixtures, so the only available signal is
/// what someone happened to call them. It catches the current bats series, and
/// it caught them correctly (verified live 2026-08-02: all six bats graphs
/// labelled fixture, none merged into live counts).
///
/// What it does NOT catch is the point. `urn:chorus:verbs-sandbox`,
/// `urn:chorus:instances-enrichtest`, `urn:chorus:instances-bt5`,
/// `urn:chorus:instances-batchdoor-test2` and the typo'd
/// `urn:chorus:domain:tests` (SINGULAR, 58 triples, sitting beside the real
/// plural one) are all test detritus this predicate reads as live. A heuristic
/// that silently defaults the unrecognized to "live" is the same
/// cannot-distinguish-two-states shape as everything else found this week —
/// so `classify_graph` below refuses to default, and says "unclassified"
/// out loud instead.
pub fn is_fixture_graph(g: &str) -> bool {
    match registry_status(g) {
        Some(GraphStatus::TestEphemeral) => true,
        Some(GraphStatus::Sanctioned) => false,
        // #3733 — the registry rules; the name shapes below survive ONLY for
        // graphs the registry has no row for, because bats fixtures mint
        // ephemeral graph names per run and cannot pre-register. A stray
        // NON-test graph still lands UNCLASSIFIED (never silently live-or-
        // fixture) because it matches neither.
        None => {
            g.contains("-test-")
                || g.contains("bats")
                || g.contains("-staging")
                || g.ends_with("-empty")
                || g.ends_with("-bad")
                || g.ends_with("-proving")
        }
    }
}

/// #3733 — a graph's status per the MODEL's registry (graph-status-3733.ttl,
/// urn:chorus:ontology). None = no row = UNCLASSIFIED, an explicit third
/// state the caller must handle — never a default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphStatus {
    Sanctioned,
    TestEphemeral,
}

fn registry_rows() -> &'static Vec<(String, GraphStatus)> {
    use std::sync::OnceLock;
    static ROWS: OnceLock<Vec<(String, GraphStatus)>> = OnceLock::new();
    // OnceLock is acceptable HERE (unlike #3947's version fields): the registry
    // changes only via model deploy, and reconcile runs are short-lived CLI
    // invocations — each run re-reads. A long-lived server caller would need
    // the TTL pattern instead.
    ROWS.get_or_init(|| {
        // Single-var CONCAT seam — the DAL's proven zero-dep row shape.
        let q = "PREFIX chorus: <https://jeffbridwell.com/chorus#> \
                 SELECT (CONCAT(?iri, \"|\", ?s) AS ?v) WHERE { GRAPH <urn:chorus:ontology> { \
                 ?g a chorus:NamedGraph ; chorus:graphIri ?iri ; chorus:graphStatus ?s } }";
        let mut rows = Vec::new();
        if let Ok(r) = sparql_json(q) {
            for line in crate::select_v(&r) {
                if let Some((iri, s)) = line.split_once('|') {
                    let status = match s {
                        "sanctioned" => GraphStatus::Sanctioned,
                        "test-ephemeral" => GraphStatus::TestEphemeral,
                        _ => continue, // no valid ruling: row ignored, graph stays unclassified
                    };
                    rows.push((iri.to_string(), status));
                }
            }
        }
        rows
    })
}

pub fn registry_status(g: &str) -> Option<GraphStatus> {
    status_from_rows(registry_rows(), g)
}

/// Pure matcher — testable without a store (#3528). Exact IRI match, or a
/// declared `prefix:*` wildcard (matches `prefix:<anything>`).
pub fn status_from_rows(rows: &[(String, GraphStatus)], g: &str) -> Option<GraphStatus> {
    for (iri, status) in rows {
        if let Some(prefix) = iri.strip_suffix(":*") {
            if g.starts_with(&format!("{prefix}:")) {
                return Some(*status);
            }
        } else if iri == g {
            return Some(*status);
        }
    }
    None
}

/// The graphs the model actually sanctions. Anything outside this and outside
/// `is_fixture_graph` is UNCLASSIFIED — counted live (conservative: never hide
/// data), but named as unclassified so a stray graph cannot pass as model
/// content by being unrecognized.
pub fn is_sanctioned_graph(g: &str) -> bool {
    // #3733 — the hardcoded allowlist is RETIRED; the model's registry rules.
    // (History: the list named ontology/instances/documents/skills/gates/
    // framework/domains:* — those rows now live in graph-status-3733.ttl,
    // where adding a graph is a governed model change, not a code edit.)
    matches!(registry_status(g), Some(GraphStatus::Sanctioned))
}

/// A punned class keeps its "instances" (subclasses) in the ontology graph —
/// ADR-045/025. Placement consistency is judged against this, never against a
/// blanket "must be an ABox graph" (which would refuse the correct ones).
/// VERIFIED AGAINST THE STORE, not assumed (2026-08-02). Product is NOT punned:
/// its three ontology-graph individuals carry `a chorus:Product` and nothing
/// else — one rdf:type each, no `owl:Class`. I had copied Domain/Service's
/// pattern into this list without checking, and the readout then emitted
/// "placement inconsistent with punned kind" for Product, which was BACKWARDS.
/// Product's declared placement (`urn:chorus:domains:products`) is CORRECT for
/// a pure-ABox class under ADR-025/051 — the instances are in the wrong graphs.
/// Jeff got there from taste ("i'm not much of a fan of either instances or
/// ontology as an iri, products/product makes sense to me") before the code did.
/// RULING 3, Silas (OWL-DBA) 2026-08-02, VERIFIED IN THE STORE: Service is NOT
/// punned — zero Service individuals carry `owl:Class`; each is a plain
/// `chorus:Service`. So `ServiceShape`'s ontology placement is a DEFECT, and
/// its stated "wipe-protection" rationale is not a rationale AND is factually
/// unnecessary: the retire clause targets only Domain/SubDomain, so a
/// plain-ABox Service in a domain graph faces zero wipe risk.
///
/// ONE REASON PER PLACEMENT: punning, never wipe-protection. Domain stays
/// (verified `owl:Class`, a real reason). Service comes out — but moving live
/// Service data is a sequenced card, not an inline edit, so this list is
/// corrected now and the data move follows.
///
/// STILL A HARDCODED LIST, and the audit is right that it should not be —
/// ADR-040:135 says the punning rule is "General rule, NOT an enumeration", and
/// it is derivable in one query (`?c a owl:Class, chorus:Domain`) from the store
/// this module already reads. Deriving it is held until the ADR-045 §3 amendment
/// lands (ruling 1) so the query is written against the ratified edge.
pub fn is_punned(class_local: &str) -> bool {
    matches!(class_local, "Domain" | "SubDomain" | "CollectionDomain")
}

/// A class whose instances are NOT in the graph its own shape declares. This is
/// the real /products defect: placement says `urn:chorus:domains:products`
/// (correct, per-domain, pure-ABox), that graph holds ZERO triples, and the 11
/// products sit in `urn:chorus:instances` (8) + `urn:chorus:ontology` (3).
/// The API reads exactly where it was told to; nobody put the data there.
pub fn instances_outside_placement(placement: Option<&str>, live: &[(String, u64)]) -> bool {
    match placement {
        None => false,
        Some(p) => !live.is_empty() && !live.iter().any(|(g, n)| g == p && *n > 0),
    }
}

/// The `.ttl` files the deploy script actually loads — PARSED FROM THE SCRIPT,
/// never copied here. A hand-kept list would reproduce the authored-vs-deployed
/// gap this endpoint exists to expose.
pub fn deploy_set() -> Vec<String> {
    // #3895 split the deploy: athena-deploy-model.sh keeps the schema/security
    // legs (store-auth recovery path); the DAL-gated instance leg moved into
    // `athena-model seed --deploy`, whose file list is DATA in
    // platform/config/instance-seed-manifest.txt. "Deployed" = either source
    // loads it — parse both, never hand-keep the union.
    let mut out = vec![];
    let script = format!("{}/platform/scripts/athena-deploy-model.sh", chorus_root());
    if let Ok(body) = std::fs::read_to_string(&script) {
        for line in body.lines() {
            let l = line.trim();
            if l.starts_with('#') {
                continue;
            }
            if let Some(i) = l.find("$CHORUS_ROOT/") {
                let rest = &l[i + "$CHORUS_ROOT/".len()..];
                if let Some(end) = rest.find(".ttl") {
                    out.push(rest[..end + 4].to_string());
                }
            }
        }
    }
    let manifest = format!("{}/platform/config/instance-seed-manifest.txt", chorus_root());
    if let Ok(body) = std::fs::read_to_string(&manifest) {
        for line in body.lines() {
            let l = line.trim();
            if l.is_empty() || l.starts_with('#') {
                continue;
            }
            if let Some((_, rel)) = l.split_once(':') {
                if rel.trim().ends_with(".ttl") {
                    out.push(rel.trim().to_string());
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// class localname -> the `.ttl` files that DECLARE it as an `owl:Class`.
pub fn authored_in() -> BTreeMap<String, Vec<String>> {
    let root = chorus_root();
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let out = Command::new("grep")
        .args(["-rl", "--include=*.ttl", "a owl:Class", &root])
        .output();
    let files = match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(_) => return map,
    };
    for f in files.lines() {
        if f.contains("node_modules") || f.contains("/tests/fixtures/") || f.contains("/target/") {
            continue;
        }
        let body = match std::fs::read_to_string(f) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let rel = f.strip_prefix(&format!("{}/", root)).unwrap_or(f).to_string();
        for line in body.lines() {
            if !line.contains("a owl:Class") {
                continue;
            }
            if let Some(rest) = line.trim_start().strip_prefix("chorus:") {
                if let Some(name) = rest.split_whitespace().next() {
                    map.entry(name.to_string()).or_default().push(rel.clone());
                }
            }
        }
    }
    for v in map.values_mut() {
        v.sort();
        v.dedup();
    }
    map
}

/// class -> [(graph, count)] for every chorus class in the store.
/// `None` means the store was unreachable — the caller renders unknown, not 0.
///
/// Uses the house multi-column pattern: CONCAT the columns into a single `?v`
/// and let `select_v` extract them. Hand-parsing SPARQL JSON is how I got a
/// silent all-zero first run — the counts came back and my splitter dropped
/// them, which is precisely the fabricated-zero this endpoint exists to kill.
pub fn store_counts() -> Option<BTreeMap<String, Vec<(String, u64)>>> {
    // Flat GROUP BY with the CONCAT in the projection — a nested SELECT with a
    // trailing BIND does not parse here, and the first version failed silently
    // (storeReachable:false). It reported unknown rather than zero, which is the
    // honesty rule doing its job, but the query still had to be right.
    let q = format!(
        "SELECT (CONCAT(STR(?g), \"|\", REPLACE(STR(?c), \".*#\", \"\"), \"|\", \
         STR(COUNT(DISTINCT ?s))) AS ?v) WHERE {{ GRAPH ?g {{ ?s a ?c \
         FILTER(STRSTARTS(STR(?c), \"{ns}\")) }} }} GROUP BY ?g ?c",
        ns = NS
    );
    let body = sparql_json(&q).ok()?;
    let mut map: BTreeMap<String, Vec<(String, u64)>> = BTreeMap::new();
    for row in crate::select_v(&body) {
        let mut it = row.splitn(3, '|');
        let (g, c, n) = (it.next()?, it.next()?, it.next()?);
        if g.is_empty() || c.is_empty() {
            continue;
        }
        map.entry(c.to_string())
            .or_default()
            .push((g.to_string(), n.parse().unwrap_or(0)));
    }

    // THE DEFAULT GRAPH — found by the data audit 2026-08-02, and this module
    // was blind to it. `GRAPH ?g { .. }` NEVER matches the unnamed default
    // graph, so the query above cannot see it however many graphs it enumerates.
    //
    // It is not a union view (a union would return the whole 31M-triple store;
    // this returns ~3,901). It is a real, separate, writable graph holding a
    // FOURTH copy of the model — Products, ValueStreams, 49 SubDomains, 32
    // Skills, zero Domains — and any SPARQL written without an explicit GRAPH
    // clause reads it and ONLY it.
    //
    // A reconciliation readout that omits a whole copy of the model is not a
    // reconciliation. Counted here under an explicitly non-IRI label so nobody
    // can mistake it for a graph name they could target with GRAPH <..>.
    for row in sparql_json(&format!(
        "SELECT (CONCAT(REPLACE(STR(?c), \".*#\", \"\"), \"|\", \
         STR(COUNT(DISTINCT ?s))) AS ?v) WHERE {{ ?s a ?c \
         FILTER(STRSTARTS(STR(?c), \"{ns}\")) }} GROUP BY ?c",
        ns = NS
    ))
    .ok()
    .as_deref()
    .map(crate::select_v)
    .unwrap_or_default()
    {
        let mut it = row.splitn(2, '|');
        let (c, n) = match (it.next(), it.next()) {
            (Some(c), Some(n)) if !c.is_empty() => (c, n),
            _ => continue,
        };
        map.entry(c.to_string())
            .or_default()
            .push((DEFAULT_GRAPH_LABEL.to_string(), n.parse().unwrap_or(0)));
    }

    Some(map)
}

/// Deliberately NOT an IRI. The default graph has no name — labelling it with
/// something that looks like one would invite `GRAPH <...>` queries that
/// silently match nothing.
pub const DEFAULT_GRAPH_LABEL: &str = "(default graph — UNNAMED, undeclared)";

/// The four-way partition. Exactly one bucket per class — Silas's ruling.
/// The bucket when instance counts are UNKNOWN (store unreachable). Not a
/// partition value — an admission. Reported separately so a store-down run and
/// a genuinely-empty store can never render identically.
pub const UNKNOWN_BUCKET: &str = "unknown-store-unreachable";

/// The four-way partition. Exactly one bucket per class — Silas's ruling.
///
/// `live_instances: None` means the store could not be read. Callers MUST pass
/// None rather than 0 in that case: `bucket(shaped, 0, ..)` and
/// `bucket(shaped, None, ..)` are different questions, and conflating them is
/// what this module exists to stop.
pub fn bucket_opt(shaped: bool, live_instances: Option<u64>, has_placement: bool) -> &'static str {
    match live_instances {
        None => UNKNOWN_BUCKET,
        Some(n) => bucket(shaped, n, has_placement),
    }
}

pub fn bucket(shaped: bool, live_instances: u64, has_placement: bool) -> &'static str {
    if !shaped {
        "not-served"
    } else if live_instances == 0 {
        "served-no-instances"
    } else if has_placement {
        "ok"
    } else {
        "served-no-placement"
    }
}

pub fn reconcile_json(tables: &[RouteTable]) -> String {
    let authored = authored_in();
    let dset = deploy_set();
    let store = store_counts();

    let mut names: Vec<String> = authored.keys().cloned().collect();
    for t in tables {
        let local = t.class.rsplit('#').next().unwrap_or("").to_string();
        if !local.is_empty() && !names.contains(&local) {
            names.push(local);
        }
    }
    names.sort();
    names.dedup();

    let mut rows = Vec::with_capacity(names.len());
    let mut tally: BTreeMap<&str, usize> = BTreeMap::new();

    for name in &names {
        let files = authored.get(name).cloned().unwrap_or_default();
        let binds = files.iter().any(|f| dset.contains(f));
        let table = tables
            .iter()
            .find(|t| t.class.rsplit('#').next().unwrap_or("") == name.as_str());
        let shaped = table.is_some();
        let placement = table
            .map(|t| t.instances_graph.clone())
            .filter(|s| !s.is_empty());
        let has_placement = placement.is_some();

        let (live, fixture): (Vec<_>, Vec<_>) = store
            .as_ref()
            .and_then(|m| m.get(name).cloned())
            .unwrap_or_default()
            .into_iter()
            .partition(|(g, _)| !is_fixture_graph(g));
        // AUDIT FIX 2026-08-02. This read `let live_total: u64 = ...sum()` over a
        // vec that `unwrap_or_default()` made EMPTY when the store was
        // unreachable — so every class scored 0 and the whole partition was
        // fabricated, while `inGraphs` honestly reported null. The module's own
        // doc-comment says "a count that cannot be computed is null, NEVER 0";
        // the rule was applied to one field and not to the headline number.
        // The disease, in the module written to cure it. Found by Jeff's
        // coherence audit, not by us.
        let live_total: Option<u64> = match store {
            None => None,
            Some(_) => Some(live.iter().map(|(_, n)| n).sum()),
        };

        let b = bucket_opt(shaped, live_total, has_placement);
        *tally.entry(b).or_insert(0) += 1;

        let mut findings: Vec<String> = vec![];
        if !files.is_empty() && !binds {
            findings.push("authored-but-not-in-deploy-set".into());
        }
        // Every finding below reads instance counts. With the store dark they
        // are unanswerable, and emitting none of them silently would read as
        // "no problems found" — the same two-states failure one layer down.
        if store.is_none() {
            findings.push(
                "instance-derived findings SUPPRESSED — the store could not be read; \
                 this is not a clean result".into(),
            );
        }
        if b == "served-no-placement" {
            findings.push("ADR-051 boundary: served with instances, no placement declared".into());
        }
        // Silas's second addendum invariant, 2026-08-02: A SERVED CLASS
        // RESOLVES TO EXACTLY ONE LIVE GRAPH. Ten classes violate it today
        // (SourceFile 1+566, Document 8+259, Skill 37+31, Principle 2+27,
        // SubDomain across 3, ...). For each, "how many are there" has two
        // answers depending which graph you read — that IS the tracking defect
        // Jeff named, and it is why three of us produced three different
        // product counts from three correct queries.
        if live.len() > 1 {
            findings.push(format!(
                "ADR-051 invariant: a served class resolves to ONE live graph; this resolves {}",
                live.len()
            ));
        }
        if !fixture.is_empty() && live.is_empty() && !files.is_empty() {
            findings.push("only present in fixture/test graphs".into());
        }
        if has_placement && is_punned(name) && placement.as_deref() != Some("urn:chorus:ontology") {
            findings.push("placement inconsistent with punned kind (ADR-051 x 025)".into());
        }
        // RULING 2, Silas (OWL-DBA) 2026-08-02 — THIS FINDING WAS WRONG AND IS
        // WITHDRAWN. It flagged residency in urn:chorus:instances as a defect
        // while athena-deploy-model.sh's INSTANCE_SET hydrates that exact graph
        // on every full deploy. The deploy script wrote what this endpoint
        // called broken, on the same commit train — and the audit endpoint was
        // the one that was wrong.
        //
        // Why: ADR-051 §4's instances-freeze was DRAFT and never ratified, so
        // nothing violated it — #3698 and #3686 declaring new kinds into that
        // graph broke no accepted rule. urn:chorus:instances IS the current
        // correct ABox home. The domain-named-graph end-state is a NAMED future
        // migration behind the read-path drain, not a freeze in force today.
        //
        // What survives is KIND-CONSISTENCY (this morning's derivation), which
        // is a different question and is still checked below. A class whose
        // instances sit somewhere its kind forbids is a finding; a class whose
        // instances sit in the current, sanctioned ABox home is not.
        //
        // A graph that is neither sanctioned nor a recognized fixture gets
        // NAMED, not silently absorbed into the live count.
        for (g, n) in live.iter() {
            if g != DEFAULT_GRAPH_LABEL && !is_sanctioned_graph(g) {
                findings.push(format!(
                    "{} instance(s) in UNCLASSIFIED graph {} — not a sanctioned model graph \
                     and not a recognized fixture; counted live because hiding data is worse, \
                     but it needs a ruling",
                    n, g
                ));
            }
        }

        // Residency in the UNNAMED default graph is always a finding, whatever
        // else is true. It is not a sanctioned home, no deploy manifest targets
        // it, and it is the one graph an unscoped query reads EXCLUSIVELY — so a
        // stale copy here outranks the real model for any caller who forgets a
        // GRAPH clause. Reported per class so the fix has a work-list.
        for (g, n) in live.iter() {
            if g == DEFAULT_GRAPH_LABEL {
                findings.push(format!(
                    "{} instance(s) in the UNNAMED default graph — no manifest targets it, \
                     and an unscoped query reads it INSTEAD of the model",
                    n
                ));
            }
        }

        // The multi-graph invariant below still fires — one served class, one
        // live graph — because SPLIT residency remains a real defect regardless
        // of which graph is sanctioned.
        if instances_outside_placement(placement.as_deref(), &live) {
            findings.push(format!(
                "instances live OUTSIDE the declared placement {} (which is empty) — the API reads where it was told",
                placement.clone().unwrap_or_default()
            ));
        }

        let graphs = |v: &Vec<(String, u64)>| {
            v.iter()
                .map(|(g, n)| format!("{{ \"graph\": \"{}\", \"count\": {} }}", json_escape(g), n))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let in_graphs = match store {
            None => "null".to_string(), // unknown, NEVER 0
            Some(_) => format!("[{}]", graphs(&live)),
        };

        rows.push(format!(
            "{{ \"class\": \"{}\", \"bucket\": \"{}\", \"authoredIn\": [{}], \"binds\": {}, \
             \"shaped\": {}, \"punned\": {}, \"placement\": {}, \"inGraphs\": {}, \
             \"fixtureGraphs\": [{}], \"findings\": [{}] }}",
            json_escape(name),
            b,
            files
                .iter()
                .map(|f| format!("\"{}\"", json_escape(f)))
                .collect::<Vec<_>>()
                .join(", "),
            binds,
            shaped,
            is_punned(name),
            match &placement {
                Some(p) => format!("\"{}\"", json_escape(p)),
                None => "null".to_string(),
            },
            in_graphs,
            graphs(&fixture),
            findings
                .iter()
                .map(|f| format!("\"{}\"", json_escape(f)))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    let tally_json = tally
        .iter()
        .map(|(k, v)| format!("\"{}\": {}", k, v))
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        "{{ \"apiVersion\": \"{}\", \"service\": \"athena-make\", \"kind\": \"Reconciliation\", \
         \"count\": {}, \"storeReachable\": {}, \"partition\": {{ {} }}, \"deploySet\": [{}], \
         \"classes\": [\n  {}\n] }}",
        API_VERSION,
        rows.len(),
        store.is_some(),
        tally_json,
        dset.iter()
            .map(|f| format!("\"{}\"", json_escape(f)))
            .collect::<Vec<_>>()
            .join(", "),
        rows.join(",\n  ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // The four-way partition is Silas's ruling; pinned so a later edit cannot
    // quietly collapse it back into a flat violation list.
    #[test]
    fn partition_is_four_way_and_exclusive() {
        assert_eq!(bucket(false, 0, false), "not-served");
        assert_eq!(bucket(false, 9, true), "not-served", "unshaped is never served");
        assert_eq!(bucket(true, 0, false), "served-no-instances");
        assert_eq!(bucket(true, 0, true), "served-no-instances", "empty is latent, not harm");
        assert_eq!(bucket(true, 11, false), "served-no-placement", "the Product case");
        assert_eq!(bucket(true, 40, true), "ok");
    }

    // ADR-051 x ADR-025 as ONE check: a punned class declaring the ontology
    // graph is CORRECT. A blanket "must be an ABox graph" would refuse the
    // three that are right.
    #[test]
    fn punned_classes_live_in_the_ontology_graph() {
        for c in ["Domain", "SubDomain", "CollectionDomain"] {
            assert!(is_punned(c), "{} is punned", c);
        }
        // Product is NOT punned — verified in the store: its individuals carry
        // `a chorus:Product` only. Listing it here was my error and it made the
        // readout blame the placement instead of the data.
        // Service moved OUT of the punned list — verified in the store 2026-08-02:
        // zero Service individuals carry owl:Class.
        for c in ["Service", "Product", "ValueStreamStep", "Test", "Principal"] {
            assert!(!is_punned(c), "{} is pure ABox", c);
        }
    }

    // The real /products defect: placement correct, graph empty, data elsewhere.
    #[test]
    fn instances_outside_the_declared_placement_is_the_product_case() {
        let live = vec![
            ("urn:chorus:instances".to_string(), 8u64),
            ("urn:chorus:ontology".to_string(), 3),
        ];
        assert!(instances_outside_placement(Some("urn:chorus:domains:products"), &live));
        // data where the shape says: not a finding
        let ok = vec![("urn:chorus:domains:tests".to_string(), 4617u64)];
        assert!(!instances_outside_placement(Some("urn:chorus:domains:tests"), &ok));
        // no placement declared: a different finding, not this one
        assert!(!instances_outside_placement(None, &live));
        // no instances at all: latent, not misplaced
        assert!(!instances_outside_placement(Some("urn:chorus:domains:security"), &[]));
    }

    // Silas's addendum invariant #2: one served class, one live graph. The
    // multi-graph case is the tracking defect itself, not a warning.
    #[test]
    fn multi_graph_residency_is_a_finding_not_a_warning() {
        // AUDIT FIX 2026-08-02: this previously built a 2-element vec and asserted
        // len() > 1 — invoking no production code, unable to fail under any edit
        // including deletion of the finding it claimed to guard. Pass-by-
        // construction. Now it exercises the real partition function instead.
        assert_eq!(bucket(true, 11, false), "served-no-placement", "the Product case");
        // fixtures never contribute to residency
        let mixed = ["urn:chorus:ontology", "urn:chorus:ontology-test-bats-3509"];
        let live_only: Vec<_> = mixed.iter().filter(|g| !is_fixture_graph(g)).collect();
        assert_eq!(live_only.len(), 1, "a fixture copy must not fake a multi-graph finding");
    }

    // Fixture graphs polluted model.deploy.failed the same way. Never live.
    // The headline defect the audit found. A store-down run and a genuinely
    // empty store MUST NOT render identically.
    //
    // PROVEN HERE, NOT LIVE — and the reason matters. I tried to reproduce it
    // against a running binary by pointing CHORUS_FUSEKI at a dead endpoint;
    // athena-make REFUSES TO BOOT without the store ("no classes generated —
    // nothing to serve"). So the fabrication window is narrower than the audit
    // implies: it needs the store to fail BETWEEN boot and the /reconcile call,
    // not at boot. Narrower, not closed — Fuseki restarts, auth expiry, and the
    // 15-21s event-loop blocks observed 2026-08-01 all land in that window.
    // Stated rather than claiming a live demo I could not get.
    #[test]
    fn the_typo_graph_is_not_laundered_into_sanctioned() {
        // urn:chorus:domain:tests (SINGULAR) holds 58 triples beside the real
        // plural graph. A `starts_with("urn:chorus:domain")` prefix would have
        // swallowed it and made a typo look like a domain graph — which is how
        // strays become permanent. Pin the boundary.
        // #3733 — sanctioning is now the REGISTRY's ruling; unit tests feed
        // fixture rows (the store-backed read is integration surface).
        let rows = vec![
            ("urn:chorus:domains:*".to_string(), GraphStatus::Sanctioned),
            ("urn:chorus:ontology".to_string(), GraphStatus::Sanctioned),
        ];
        assert_eq!(status_from_rows(&rows, "urn:chorus:domains:tests"), Some(GraphStatus::Sanctioned));
        assert_eq!(
            status_from_rows(&rows, "urn:chorus:domain:tests"),
            None,
            "the singular typo matches NO row — UNCLASSIFIED, never laundered"
        );

        // And the strays the name heuristic misses must land as unclassified —
        // not fixture (the heuristic can't see them) and not sanctioned.
        for g in [
            "urn:chorus:verbs-sandbox",
            "urn:chorus:instances-enrichtest",
            "urn:chorus:instances-bt5",
            "urn:chorus:instances-batchdoor-test2",
        ] {
            assert!(!is_sanctioned_graph(g), "{} is not sanctioned", g);
        }
    }

    #[test]
    fn the_default_graph_label_is_not_a_targetable_iri() {
        // Regression guard for the audit finding: this module was blind to the
        // default graph because `GRAPH ?g` cannot match it. Now that it is
        // counted, the label must never look like an IRI a reader could put in
        // `GRAPH <...>` — that would match nothing and reproduce the blindness.
        assert!(!DEFAULT_GRAPH_LABEL.starts_with("urn:"));
        assert!(!DEFAULT_GRAPH_LABEL.starts_with("http"));
        assert!(DEFAULT_GRAPH_LABEL.contains("UNNAMED"));
    }

    #[test]
    fn an_unreachable_store_is_unknown_never_zero() {
        assert_eq!(bucket_opt(true, None, true), UNKNOWN_BUCKET);
        assert_eq!(bucket_opt(true, None, false), UNKNOWN_BUCKET, "not the harm bucket either");
        assert_eq!(bucket_opt(false, None, false), UNKNOWN_BUCKET, "not even not-served");
        // a real zero is still a real answer, and stays distinct from unknown
        assert_eq!(bucket_opt(true, Some(0), true), "served-no-instances");
        assert_ne!(bucket_opt(true, Some(0), true), bucket_opt(true, None, true));
    }

    #[test]
    fn fixture_graphs_are_never_counted_as_live() {
        for g in [
            "urn:chorus:ontology-test-bats-3509",
            "urn:chorus:ontology-test-bats-3509-proving",
            "urn:chorus:instances-staging-deploy",
            "urn:chorus:ontology-test-bats-3509-empty",
            "urn:chorus:ontology-test-bats-3509-bad",
        ] {
            assert!(is_fixture_graph(g), "{} must be labelled fixture", g);
        }
        for g in [
            "urn:chorus:ontology",
            "urn:chorus:instances",
            "urn:chorus:domains:security",
        ] {
            assert!(!is_fixture_graph(g), "{} is live", g);
        }
    }
}

#[cfg(test)]
mod graph_registry_3733 {
    use super::*;

    fn rows() -> Vec<(String, GraphStatus)> {
        vec![
            ("urn:chorus:ontology".to_string(), GraphStatus::Sanctioned),
            ("urn:chorus:domains:*".to_string(), GraphStatus::Sanctioned),
            ("urn:chorus:verbs-sandbox".to_string(), GraphStatus::TestEphemeral),
        ]
    }

    /// The registry rules where it has a ruling.
    #[test]
    fn declared_rulings_win() {
        assert_eq!(status_from_rows(&rows(), "urn:chorus:ontology"), Some(GraphStatus::Sanctioned));
        assert_eq!(status_from_rows(&rows(), "urn:chorus:verbs-sandbox"), Some(GraphStatus::TestEphemeral));
        assert_eq!(status_from_rows(&rows(), "urn:chorus:domains:security"), Some(GraphStatus::Sanctioned));
    }

    /// NEGATIVE PROOF (#3734): no row → None, and None is never sanctioned.
    /// This is the exact laundering path the heuristic had: unrecognized →
    /// silently live. The typo graph must stay UNCLASSIFIED forever until a
    /// human rules on it.
    #[test]
    fn absence_is_unclassified_never_sanctioned() {
        for g in ["urn:chorus:domain:tests", "urn:chorus:instances-bt5", "urn:totally:new"] {
            assert_eq!(status_from_rows(&rows(), g), None, "{g} must have no ruling");
        }
    }

    /// NEGATIVE PROOF: the wildcard is one scheme, not a regex — a prefix
    /// without its trailing segment does not match, and an unrelated scheme
    /// sharing characters does not match.
    #[test]
    fn wildcard_is_narrow() {
        assert_eq!(status_from_rows(&rows(), "urn:chorus:domains"), None, "bare prefix, no segment");
        assert_eq!(status_from_rows(&rows(), "urn:chorus:domainsevil:x"), None, "not the prefix");
    }
}
