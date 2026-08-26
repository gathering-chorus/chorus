//! #3739 — /schema carries RELATIONSHIPS, not just fields. The model's edges
//! (`field|edge:Target`) were already in the RouteTable; the schema projection
//! dropped them to bare annotated strings nothing parsed, so every consumer saw
//! entities without lines. Red-first: schema_set_json is the pure function the
//! /schema route serves, and it must emit a typed `relationships` array per class.

use athena_make::{schema_set_json, RouteTable};

fn table(class: &str, fields: Vec<&str>) -> RouteTable {
    RouteTable {
        class: format!("https://jeffbridwell.com/chorus#{}", class),
        fields: fields.into_iter().map(|f| f.into()).collect(),
        routes: vec![],
        secured: vec![],
        mandatory: vec!["label".into()],
        write_required: vec!["label".into()],
        repo_target: format!("generated/{}", class.to_lowercase()),
        exposure: vec![],
        instances_graph: "urn:chorus:instances".into(),
        tree_edges: vec![],
        tree_order: None,
        model_version: "v2".to_string(),
    }
}

#[test]
fn schema_carries_typed_edges_for_a_class_that_has_them() {
    let tables = vec![table(
        "Card",
        vec!["label|datatype:string", "hasVerbRun|edge:VerbRun", "ownedBy|edge:Role"],
    )];
    let doc = schema_set_json(&tables);
    assert!(doc.contains("\"relationships\""), "schema must carry a relationships array");
    assert!(
        doc.contains("\"property\": \"hasVerbRun\"") && doc.contains("\"range\": \"VerbRun\""),
        "each edge is a typed (property, range) pair: {}",
        doc
    );
    assert!(doc.contains("\"property\": \"ownedBy\"") && doc.contains("\"range\": \"Role\""));
}

#[test]
fn edge_fields_do_not_leak_into_plain_fields_and_plain_fields_make_no_edges() {
    // The NEGATIVE proof both ways: a class with zero edges must serve an
    // EMPTY relationships array (not omit it, not invent edges from datatype
    // fields) — otherwise the ERD cannot distinguish "no relationships"
    // (a real finding, AC3) from "relationships not served".
    let tables = vec![table("Principle", vec!["label|datatype:string", "comment|plain"])];
    let doc = schema_set_json(&tables);
    assert!(
        doc.contains("\"relationships\": []"),
        "a class with no edges serves an explicit empty relationships array: {}",
        doc
    );
    assert!(!doc.contains("\"range\": \"string\""), "datatype fields are not edges");
}

#[test]
fn schema_set_names_every_class_once_with_fields_and_relationships() {
    let tables = vec![
        table("Card", vec!["label|datatype:string", "hasVerbRun|edge:VerbRun"]),
        table("Gate", vec!["label|datatype:string", "gatekeeper|edge:Role"]),
    ];
    let doc = schema_set_json(&tables);
    assert!(doc.contains("\"count\": 2"));
    assert!(doc.contains("\"kind\": \"Card\"") && doc.contains("\"kind\": \"Gate\""));
    assert!(doc.contains("\"gatekeeper\""));
    // fields stay intact for existing consumers (#3706 page reads them)
    assert!(doc.contains("\"fields\""), "fields block survives the extension");
}
