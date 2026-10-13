// @test-type: unit
//! #4517 — a create needs a Write row on the graph it lands in, checked before
//! the body (Jeff 2026-10-13 07:55: "isnt that why we have permissions?").
//! Measured on the live door 07:47: an invalid body from a refused caller got
//! 422, so the shape leaked and "may I write" could not be asked cheaply; and on
//! graphs with no Write row for anyone, every role could create.
use athena_make::{create_refusal, handle_write_scoped, RouteTable, WriteOp};

const GRAPH: &str = "urn:chorus:domains:policies";

fn table() -> RouteTable {
    RouteTable { unbounded: vec![], write_authority: String::new(), domain: String::new(), base_path: String::new(),
        class: "https://jeffbridwell.com/chorus#Policy".into(),
        fields: vec!["comment|datatype:string".into(), "ownedBy|edge:Principal".into()],
        routes: vec!["GET /policies".into()],
        secured: vec![],
        mandatory: vec!["comment".into()],
        write_required: vec!["comment".into()], allowed_values: vec![],
        repo_target: "generated/policy".into(),
        exposure: vec![],
        instances_graph: GRAPH.into(),
        tree_edges: vec![],
        tree_order: None, model_version: "v2".to_string(),
    }
}

fn granted() -> Vec<String> { vec![GRAPH.to_string()] }

#[test]
fn a_create_without_a_write_row_on_the_graph_is_refused() {
    let (code, body) = create_refusal(&WriteOp::CreateEntity, GRAPH, &[], "kade").expect("refused");
    assert_eq!(code, 403, "{body}");
    assert!(body.contains("no Write row for 'kade'") && body.contains(GRAPH), "{body}");
    assert!(create_refusal(&WriteOp::CreateBatch, GRAPH, &["urn:chorus:domains:tests".into()], "kade").is_some(),
        "a Write row on another graph opens nothing here");
}

#[test]
fn a_create_with_a_write_row_on_the_graph_passes_this_check() {
    assert!(create_refusal(&WriteOp::CreateEntity, GRAPH, &granted(), "kade").is_none());
    assert!(create_refusal(&WriteOp::CreateBatch, GRAPH, &granted(), "kade").is_none());
}

#[test]
fn edits_to_an_existing_row_stay_with_the_owner_rules() {
    let edit = WriteOp::PatchEntity { name: "p1".into() };
    assert!(create_refusal(&edit, GRAPH, &[], "kade").is_none(), "owner rules govern a row once it exists");
}

/// NEGATIVE PROOF of the order: the same invalid body answers 403 to a caller
/// with no Write row and 422 to one with it. If the body were checked first,
/// both would be 422 (the live door's answer before this card).
#[test]
fn a_refused_caller_gets_403_before_any_body_check() {
    let bad = r#"{"zz-invalid":1}"#;
    let (refused, rbody) = handle_write_scoped("POST", "/policies", bad, &table(), "kade", "", "", &[]);
    assert_eq!(refused, 403, "{rbody}");
    let (allowed, abody) = handle_write_scoped("POST", "/policies", bad, &table(), "kade", "", "", &granted());
    assert_eq!(allowed, 422, "{abody}");
}
