//! #3467 — owl-api projects two MORE surfaces from the model, alongside
//! routes/openapi/page: a TEST manifest (unit + conformance + security) and the
//! ADR-031 MCP tool binding. Both pure functions of the RouteTable. Red-first.

use owl_api::{mcp_binding, tests_manifest, RouteTable};

fn fixture() -> RouteTable {
    RouteTable {
        class: "https://jeffbridwell.com/chorus#Domain".into(),
        fields: vec![
            "label|datatype:string".into(),
            "comment|datatype:string".into(),
            "port|datatype:integer".into(),   // a STRICT datatype → a wrong value is rejectable
            "partOf|edge:Product".into(),     // an edge → target-type is enforced
        ],
        routes: vec![
            "GET /domains".into(),
            "GET /domains/:name".into(),
            "POST /domains".into(),
            "POST /domains/:name/partof".into(),
            "GET /schema/domain".into(),
        ],
        secured: vec!["/schema/domain".into()],
        mandatory: vec!["label".into(), "comment".into()],
        repo_target: "generated/domain".into(),  // #3488 — repo land location
    }
}

#[test]
fn tests_manifest_projects_unit_conformance_security() {
    let m = tests_manifest(&fixture());
    // identity
    assert!(m.contains("\"class\""), "names the class");
    assert!(m.contains("Domain"));
    // unit snapshot carries the projected route/mandatory/secured sets
    assert!(m.contains("\"unit\"") && m.contains("\"mandatory\"") && m.contains("label"));
    // conformance: the list route asserts 200
    assert!(m.contains("\"conformance\"") && m.contains("\"GET /domains\"") || m.contains("\"GET\""));
    assert!(m.contains("200"));
    // security from the model: unauth write → 401
    assert!(m.contains("401"), "unauth write must assert 401");
    // secured surface (sh:requiresAuth-projected) → 401
    assert!(m.contains("/schema/domain"));
    // injection guard → 400
    assert!(m.contains("400"), "injection name must assert 400");
    // completeness floor → incomplete create 422
    assert!(m.contains("422"), "incomplete create must assert 422");
    // #3467 finish — the generated tests ASSERT the new constraint-enforcement:
    // datatype rejection (a strict-typed field gets a bad value → 422) and
    // edge-target-type rejection (an edge points at a wrong-typed target → 422).
    assert!(m.contains("\"constraints\""), "manifest carries a constraints block");
    assert!(m.contains("datatype") && m.contains("port"), "datatype-rejection case for the strict field 'port'");
    assert!(m.contains("edge-target-type") && m.contains("partOf"), "edge-target-type rejection case for the partOf edge");
}

#[test]
fn mcp_binding_is_adr031_conformant() {
    let b = mcp_binding(&fixture());
    // ADR-031 shape: chorus_<plural-resource>_<verb>, closed verb set get/list/add
    assert!(b.contains("chorus_domains_get"), "get tool");
    assert!(b.contains("chorus_domains_list"), "list tool");
    assert!(b.contains("chorus_domains_add"), "add tool");
    // no bare verbs / verb-first / actor suffixes — every tool starts chorus_domains_
    // add delegates to the DAL (the one write authority)
    assert!(b.to_lowercase().contains("dal"), "add delegates to the DAL");
    // pluralized resource, not the bare class
    assert!(!b.contains("chorus_domain_get"), "must pluralize the resource (domains, not domain)");
}

// #3482 (folded into #3488) — the ADR-031 name GATE: not just "the expected
// names exist" but "EVERY generated tool name conforms" — so a future generator
// change that emits a non-conformant name (verb-first, bad verb, un-pluralized,
// uppercase) FAILS here instead of drifting silently. Pure check over the
// generated binding; no regex crate (owl-api is zero-dep).
#[test]
fn every_generated_mcp_name_obeys_adr031_grain() {
    let b = mcp_binding(&fixture());
    // extract every "name": "<tool>" value
    let names: Vec<String> = b
        .match_indices("\"name\":")
        .filter_map(|(i, _)| {
            let after = &b[i + 7..];
            let start = after.find('"')? + 1;
            let end = after[start..].find('"')? + start;
            Some(after[start..end].to_string())
        })
        .collect();
    assert!(!names.is_empty(), "binding must emit at least one tool name");
    let verbs = ["get", "list", "add"];
    for n in &names {
        let parts: Vec<&str> = n.split('_').collect();
        assert_eq!(parts.len(), 3, "ADR-031 grain is chorus_<resource>_<verb>: '{}'", n);
        assert_eq!(parts[0], "chorus", "must be chorus-namespaced: '{}'", n);
        assert!(verbs.contains(&parts[2]), "verb must be one of {:?}: '{}'", verbs, n);
        let resource = parts[1];
        assert!(
            !resource.is_empty()
                && resource.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
            "resource must be lowercase alnum: '{}'", n
        );
        assert!(resource.ends_with('s'), "resource must be pluralized: '{}'", n);
    }
}
