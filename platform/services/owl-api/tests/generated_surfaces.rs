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
