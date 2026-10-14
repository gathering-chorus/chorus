// @test-type: unit
// @domain: services
//! #4514 AC4 — `chorus-runner call` turns a dagu step's key=value pairs into
//! tool arguments, and reads Skill rows from the store the same way the server does.
use chorus_runner::client::args_from_pairs;
use chorus_runner::server::rows_from_sparql;
use serde_json::json;

#[test]
fn pairs_become_text_arguments_and_a_repeated_key_a_list() {
    let a = args_from_pairs(&["card=4514".into(), "atomic=true".into(), "args=scope".into(), "args=.".into()]).unwrap();
    assert_eq!(a, json!({ "card": "4514", "atomic": "true", "args": ["scope", "."] }));
}

#[test]
fn negative_proof_a_pair_without_equals_is_refused() {
    assert!(args_from_pairs(&["card".into()]).is_err());
}

#[test]
fn store_rows_read_like_the_doors_rows() {
    let c = "https://jeffbridwell.com/chorus#";
    let b = |p: &str, o: serde_json::Value| json!({ "s": { "type": "uri", "value": format!("{c}skill-werk-commit") }, "p": { "type": "uri", "value": format!("{c}{p}") }, "o": o });
    let body = json!({ "results": { "bindings": [
        b("ownedBy", json!({ "type": "uri", "value": format!("{c}principal-kade") })),
        b("runsAs", json!({ "type": "literal", "value": "caller" })),
        b("arguments", json!({ "type": "literal", "value": "{card} {caller} --atomic" })),
    ] } }).to_string();
    let rows = rows_from_sparql(&body).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["name"], "werk-commit", "the class prefix is dropped, as the door does");
    assert_eq!(rows[0]["ownedBy"], "principal-kade");
    assert_eq!(rows[0]["runsAs"], "caller");
    // NEGATIVE PROOF: an unreadable answer is an error, never zero tools
    assert!(rows_from_sparql("<html>").is_err());
}
