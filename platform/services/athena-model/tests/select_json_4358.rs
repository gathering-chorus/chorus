//! #4358 (reopened 2026-10-07) — the DAL reads every shape SELECT through
//! `parse_select_v`. It used to cut values out of Fuseki's JSON by hand and kept
//! the JSON escapes, so the stored pattern `^https://jeffbridwell\.com/...` came
//! back as `\\.` and the store's REGEX refused every principle row in prod
//! (18:09, the 14 cleanup PUTs). These tests feed the exact JSON Fuseki sends.

use athena_model::parse_select_v;

/// Fuseki's SPARQL-JSON for one ?v binding whose value is `^https://x\.com/a$`.
const PATTERN_JSON: &str = r#"{ "head": { "vars": [ "v" ] },
  "results": { "bindings": [
      { "v": { "type": "literal" , "value": "|^https://x\\.com/a$" } }
  ] } }"#;

#[test]
fn a_backslash_comes_back_as_one_backslash() {
    let vals = parse_select_v(PATTERN_JSON).unwrap();
    assert_eq!(vals, vec![r"|^https://x\.com/a$".to_string()]);
}

#[test]
fn negative_proof_the_json_text_itself_holds_a_doubled_backslash() {
    // The old reader returned the bytes between the quotes; those bytes hold `\\.`.
    // If the parser ever returns this again, the first test goes red.
    assert!(PATTERN_JSON.contains(r"x\\.com"));
    assert_ne!(parse_select_v(PATTERN_JSON).unwrap()[0], r"|^https://x\\.com/a$");
}

#[test]
fn an_escaped_quote_does_not_cut_the_value_short() {
    let body = r#"{ "head": { "vars": [ "v" ] }, "results": { "bindings": [
        { "v": { "type": "literal", "value": "cite as \"Hemenway\"" } },
        { "v": { "type": "literal", "value": "second" } } ] } }"#;
    assert_eq!(parse_select_v(body).unwrap(), vec!["cite as \"Hemenway\"".to_string(), "second".to_string()]);
}

#[test]
fn rows_without_v_are_skipped_and_garbage_is_an_error() {
    let body = r#"{ "head": { "vars": [ "v" ] }, "results": { "bindings": [ {}, { "v": { "type": "uri", "value": "https://x/a" } } ] } }"#;
    assert_eq!(parse_select_v(body).unwrap(), vec!["https://x/a".to_string()]);
    assert!(parse_select_v("<html>502 Bad Gateway</html>").is_err());
}
