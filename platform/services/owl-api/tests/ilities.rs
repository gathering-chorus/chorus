//! #3364 remaining ilities — red-first tests (DEC-1674).
//!
//! AC1: generate() refuses ADR-040-violating input with a typed refusal.
//! AC2: the generation pass emits an OpenAPI contract from the shapes.
//! AC6: a missing/blank trace header mints a joinable trace id (Kade's
//!      #3354 verification finding — silent trace_id:"" is the
//!      silent-degradation class).

use owl_api::{adr040_check, effective_trace, openapi_json, RouteTable};

fn table() -> RouteTable {
    RouteTable {
        class: "https://jeffbridwell.com/chorus#Domain".into(),
        fields: vec![
            "atStep|edge:ValueStreamStep".into(),
            "comment|datatype:string".into(),
            "contains|plain".into(),
        ],
        routes: vec![
            "GET /domains".into(),
            "GET /domains/:name".into(),
            "GET /domains/:name/contains".into(),
            "GET /domains/:name/partof".into(),
            "GET /domains/:name/has-child".into(),
            "GET /schema/domain".into(),
        ],
        secured: vec!["/schema/domain".into()],  // #3414 — model-projected secured-set
        mandatory: vec!["comment".into()],       // #3468 — model-projected completeness floor
        repo_target: "generated/domain".into(),  // #3488 — repo land location
        exposure: vec![],
        instances_graph: "urn:chorus:instances".into(),  // #3570 — default home (back-compat)
        tree_edges: vec![],
        tree_order: None,
    }
}

// ── AC1: ADR-040 conformance at the source ─────────────────────────────

#[test]
fn adr040_refuses_non_camelcase_class() {
    // L4: classes are CamelCase. snake/kebab/lowercase class names must refuse.
    for bad in ["domain_thing", "domain-thing", "domain", "2Domain", ""] {
        let e = adr040_check(bad, &[]).unwrap_err();
        assert!(e.contains("adr040"), "refusal must be typed adr040, got: {}", e);
    }
}

#[test]
fn adr040_refuses_non_camelcase_field() {
    // L4: properties are camelCase. snake_case or CamelCase fields refuse.
    for bad in ["has_design_doc|plain", "OwnedBy|plain", "owned-by|plain"] {
        let e = adr040_check("Domain", &[bad.to_string()]).unwrap_err();
        assert!(e.contains("adr040"), "refusal must be typed adr040, got: {}", e);
    }
}

#[test]
fn adr040_passes_conformant_input() {
    assert!(adr040_check("Domain", &table().fields).is_ok());
    assert!(adr040_check("ValueStreamStep", &["atStep|edge:ValueStreamStep".to_string()]).is_ok());
}

// ── AC2: generated OpenAPI contract ────────────────────────────────────

#[test]
fn openapi_is_valid_json_with_paths_and_typed_properties() {
    let spec = openapi_json(&table());
    assert!(spec.contains("\"openapi\": \"3.1.0\""));
    // every generated route appears as a path
    assert!(spec.contains("\"/domains\""));
    assert!(spec.contains("\"/domains/{name}\""));
    assert!(spec.contains("\"/domains/{name}/contains\""));
    assert!(spec.contains("\"/domains/{name}/partof\""), "#3420 — the upward inverse-edge route is in the contract");
    assert!(spec.contains("\"/domains/{name}/has-child\""), "#3351 — the structural-recursion (hasChild) fold is in the contract");
    assert!(spec.contains("\"/schema/domain\""));
    // datatype fields map to JSON types; edges map to the name+label object
    assert!(spec.contains("\"comment\": { \"type\": \"string\" }"));
    assert!(spec.contains("\"atStep\""));
    assert!(spec.contains("\"$ref\": \"#/components/schemas/EdgeRef\""));
    // audit fields from the DAL are part of the contract
    assert!(spec.contains("\"created\""));
    assert!(spec.contains("\"creator\""));
}

#[test]
fn openapi_is_deterministic() {
    assert_eq!(openapi_json(&table()), openapi_json(&table()));
}

// ── AC6: trace mint-when-absent ────────────────────────────────────────

#[test]
fn blank_trace_header_mints_a_joinable_id() {
    let minted = effective_trace("", 1781215124106, 7);
    assert!(minted.starts_with("owl-"), "minted trace must be recognizable: {}", minted);
    assert!(minted.contains("1781215124106"));
    // never empty — unjoinable-with-no-complaint is the failure class
    assert!(!minted.is_empty());
}

#[test]
fn provided_trace_header_passes_through_untouched() {
    assert_eq!(effective_trace("jeff-demo-1758", 1, 1), "jeff-demo-1758");
}
