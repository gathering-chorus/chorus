// @test-type: unit
// @domain: services
//! #4515 — an argument whose shape says it names a role must be a role the
//! roles door lists. What a role sees: `clearing-tag owner=bob` is refused
//! with the list of real roles, before the cards CLI ever runs.
use chorus_runner::skills::{role_args, role_refusal};
use serde_json::{json, Map, Value};

fn obj(v: Value) -> Map<String, Value> { v.as_object().cloned().unwrap() }

fn schema() -> Map<String, Value> {
    obj(json!({ "type": "object", "properties": {
        "id": { "type": "integer" },
        "owner": { "type": "string", "namesRole": "peer" },
        "target": { "type": "string", "namesRole": "agent" },
        "title": { "type": "string" }
    }}))
}

fn door() -> Value {
    json!({ "data": [
        { "name": "wren", "roleKind": "agent" }, { "name": "silas", "roleKind": "agent" },
        { "name": "jeff", "roleKind": "human" }, { "name": "herald", "roleKind": "service" }
    ]})
}

#[test]
fn only_arguments_the_shape_marks_are_checked() {
    let checks = role_args(&obj(json!({ "id": 1, "owner": "jeff", "title": "bob" })), &schema());
    assert_eq!(checks, vec![("owner".to_string(), "jeff".to_string(), "peer".to_string())]);
    assert!(role_args(&obj(json!({ "id": 1, "title": "x" })), &schema()).is_empty(), "no role argument, no door read");
}

#[test]
fn a_listed_role_passes_and_jeff_may_own_but_not_act() {
    assert_eq!(role_refusal(&role_args(&obj(json!({ "owner": "jeff" })), &schema()), &door()), None);
    let why = role_refusal(&role_args(&obj(json!({ "target": "jeff" })), &schema()), &door()).expect("a human is not an agent");
    assert_eq!(why, "Unknown target 'jeff' — the roles door lists silas | wren");
}

#[test]
fn negative_proof_an_unlisted_role_is_refused_with_the_real_list() {
    let why = role_refusal(&role_args(&obj(json!({ "owner": "bob" })), &schema()), &door()).expect("bob is no role");
    assert_eq!(why, "Unknown owner 'bob' — the roles door lists jeff | silas | wren");
    let svc = role_refusal(&role_args(&obj(json!({ "owner": "herald" })), &schema()), &door());
    assert!(svc.is_some(), "a service row is not a peer");
}

#[test]
fn a_door_answer_with_no_rows_refuses_rather_than_passes() {
    let checks = role_args(&obj(json!({ "owner": "jeff" })), &schema());
    assert!(role_refusal(&checks, &json!({})).is_some());
}
