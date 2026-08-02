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
//! AuthBoundary's three verify doors (owl-api seam, DAL, chorus-api envelope)
//! are live. The finding is "the posture the design promised exists in scripts
//! and doors but was never projected into the graph" — NOT "populate 4 empty
//! classes."
//!
//! Per absence-stays-absent: this endpoint NEVER auto-populates. A latent class
//! stays honestly empty until someone deliberately models it.
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
pub fn is_fixture_graph(g: &str) -> bool {
    g.contains("-test-")
        || g.contains("bats")
        || g.contains("-staging")
        || g.ends_with("-empty")
        || g.ends_with("-bad")
        || g.ends_with("-proving")
}

/// A punned class keeps its "instances" (subclasses) in the ontology graph —
/// ADR-045/025. Placement consistency is judged against this, never against a
/// blanket "must be an ABox graph" (which would refuse the correct ones).
pub fn is_punned(class_local: &str) -> bool {
    matches!(
        class_local,
        "Domain" | "SubDomain" | "CollectionDomain" | "Service" | "Product"
    )
}

/// The `.ttl` files the deploy script actually loads — PARSED FROM THE SCRIPT,
/// never copied here. A hand-kept list would reproduce the authored-vs-deployed
/// gap this endpoint exists to expose.
pub fn deploy_set() -> Vec<String> {
    let path = format!("{}/platform/scripts/chorus-model-deploy.sh", chorus_root());
    let body = match std::fs::read_to_string(&path) {
        Ok(b) => b,
        Err(_) => return vec![],
    };
    let mut out = vec![];
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
    Some(map)
}

/// The four-way partition. Exactly one bucket per class — Silas's ruling.
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
        let live_total: u64 = live.iter().map(|(_, n)| n).sum();

        let b = bucket(shaped, live_total, has_placement);
        *tally.entry(b).or_insert(0) += 1;

        let mut findings: Vec<String> = vec![];
        if !files.is_empty() && !binds {
            findings.push("authored-but-not-in-deploy-set".into());
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
        "{{ \"apiVersion\": \"{}\", \"service\": \"owl-api\", \"kind\": \"Reconciliation\", \
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
        for c in ["Domain", "Service", "Product", "SubDomain"] {
            assert!(is_punned(c), "{} is punned", c);
        }
        for c in ["ValueStreamStep", "Test", "Principal"] {
            assert!(!is_punned(c), "{} is pure ABox", c);
        }
    }

    // Silas's addendum invariant #2: one served class, one live graph. The
    // multi-graph case is the tracking defect itself, not a warning.
    #[test]
    fn multi_graph_residency_is_a_finding_not_a_warning() {
        // two live graphs for one class is always reportable, whatever the counts
        let live = vec![("urn:chorus:instances".to_string(), 8u64), ("urn:chorus:ontology".to_string(), 3)];
        assert!(live.len() > 1, "the Product case must trip the invariant");
        // fixtures never contribute to residency
        let mixed = ["urn:chorus:ontology", "urn:chorus:ontology-test-bats-3509"];
        let live_only: Vec<_> = mixed.iter().filter(|g| !is_fixture_graph(g)).collect();
        assert_eq!(live_only.len(), 1, "a fixture copy must not fake a multi-graph finding");
    }

    // Fixture graphs polluted model.deploy.failed the same way. Never live.
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
