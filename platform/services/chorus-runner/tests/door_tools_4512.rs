// @test-type: unit
// @domain: services
//! #4512 AC3 — the door tools are generated from what the door serves, never
//! hand-listed: the collection enum IS the door's own `served` list, a
//! collection the door does not serve never reaches it, and a row name is
//! checked before it does.

use chorus_runner::{door_path, door_tools, served_collections};
use serde_json::{json, Map, Value};

const DISCOVERY: &str = r#"{"apiVersion":"v1","kind":"Discovery","count":4,"primitives":[
  {"kind":"Card","collection":"/v1/cards/cards","openapi":"/v1/cards/cards/openapi.json"},
  {"kind":"CodeFile","collection":"/v1/code/files","openapi":"/v1/code/files/openapi.json"},
  {"kind":"Card","collection":"/v1/cards/cards","openapi":"/v1/cards/cards/openapi.json"},
  {"kind":"Domain","collection":"/v1/domains/domains","openapi":"/v1/domains/domains/openapi.json"}]}"#;

fn args(v: Value) -> Map<String, Value> {
    v.as_object().unwrap().clone()
}

#[test]
fn the_collections_are_the_doors_own_list() {
    let cols = served_collections(DISCOVERY).unwrap();
    assert_eq!(cols, vec!["/cards/cards", "/code/files", "/domains/domains"]);
    assert!(served_collections(r#"{"primitives":[]}"#).is_err(), "an empty door is an error, not zero tools");
    assert!(served_collections("not json").is_err());
}

#[test]
fn the_tool_enum_is_exactly_what_the_door_serves() {
    let cols = served_collections(DISCOVERY).unwrap();
    let tools = door_tools(&cols);
    let names: Vec<&str> = tools.iter().map(|t| t.name).collect();
    assert_eq!(names, vec!["door_list", "door_get"]);
    for t in &tools {
        let en = &t.input_schema["properties"]["collection"]["enum"];
        assert_eq!(en, &json!(["/cards/cards", "/code/files", "/domains/domains"]), "{}", t.name);
    }
}

/// NEGATIVE PROOF: a collection the door does not serve is refused here and
/// never becomes a door request, whatever the caller sends.
#[test]
fn a_collection_the_door_does_not_serve_never_reaches_it() {
    let cols = served_collections(DISCOVERY).unwrap();
    let bad = door_path("door_list", &args(json!({"collection":"/security/credentials"})), &cols).unwrap_err();
    assert_eq!(bad.status, 400);
    assert!(bad.reason.contains("does not serve /security/credentials"), "{}", bad.reason);
    assert!(bad.reason.contains("it serves: /cards/cards, /code/files, /domains/domains"), "names the served list: {}", bad.reason);
    assert_eq!(door_path("door_list", &args(json!({"collection":"/cards/cards","limit":9999})), &cols).unwrap(),
        "/v1/cards/cards?limit=500", "limit is capped");
}

#[test]
fn a_row_name_is_checked_before_it_reaches_the_door() {
    let cols = served_collections(DISCOVERY).unwrap();
    assert_eq!(door_path("door_get", &args(json!({"collection":"/cards/cards","name":"card-4512"})), &cols).unwrap(),
        "/v1/cards/cards/card-4512");
    for evil in ["../security/credentials", "a b", "x?y=1", ""] {
        assert!(door_path("door_get", &args(json!({"collection":"/cards/cards","name":evil})), &cols).is_err(), "{evil:?}");
    }
    assert_eq!(door_path("door_delete", &args(json!({"collection":"/cards/cards"})), &cols).unwrap_err().status, 404);
}
