// @test-type: unit
// @domain: services
//! #4762 (Jeff 2026-10-27: "the api must generate via athena-gen no more
//! custom") — an athena tool runs only where the class's generated
//! openapi.json declares its method (src/lib declared): the door's own spec,
//! not a hand list, says what a class allows.
use chorus_runner::declared;

const SPEC: &str = r#"{ "openapi": "3.0.3", "paths": {
  "/cards/cards": { "get": {}, "post": {} },
  "/cards/cards/{name}": { "get": {}, "put": {}, "patch": {}, "delete": {} } } }"#;

#[test]
fn each_tool_runs_where_the_generated_spec_declares_its_method() {
    for t in ["athena-get", "athena-list", "athena-add", "athena-set", "athena-delete"] {
        assert_eq!(declared(SPEC, t, "/cards/cards"), Ok(()), "{t}");
    }
}

// NEGATIVE PROOF: a method the class's spec does not declare is refused before the door
#[test]
fn a_method_the_spec_does_not_declare_is_refused() {
    let no_delete = SPEC.replace(r#", "delete": {}"#, "");
    let e = declared(&no_delete, "athena-delete", "/cards/cards").unwrap_err();
    assert_eq!(e.status, 405);
    assert!(e.reason.contains("does not declare DELETE /cards/cards/{name}"), "{}", e.reason);
    let no_patch = SPEC.replace(r#", "patch": {}"#, "");
    assert_eq!(declared(&no_patch, "athena-set", "/cards/cards").unwrap_err().status, 405);
}

// NEGATIVE PROOF: no readable spec is a refusal, never a pass
#[test]
fn an_unreadable_spec_is_refused_never_passed() {
    assert_eq!(declared("<html>", "athena-get", "/cards/cards").unwrap_err().status, 502);
    assert_eq!(declared(r#"{"paths":{}}"#, "athena-get", "/cards/cards").unwrap_err().status, 405);
}
