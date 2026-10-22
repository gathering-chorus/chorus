// @domain: security
// #4686 — step 3c read Fuseki's COUNT answer with a `"value":"` needle, but
// Fuseki writes `"value": "4"`, so every create read 0 of 4 hats and rolled back.
use chorus_principal::{sparql_count, wears_all_hats};

// Fuseki's own answer, as measured 2026-10-21 for principal-borg-queen.
fn fuseki(n: &str) -> String {
    format!(r#"{{ "head": {{
    "vars": [ "n" ]
  }} ,
  "results": {{
    "bindings": [
      {{ 
        "n": {{ "type": "literal" , "datatype": "http://www.w3.org/2001/XMLSchema#integer" , "value": "{n}" }}
      }}
    ]
  }}
}}"#)
}

#[test]
fn fuseki_spaced_answer_reads_four_and_passes() {
    assert_eq!(sparql_count(&fuseki("4"), "n"), Some(4));
    assert_eq!(wears_all_hats(&fuseki("4")), Ok(4));
}

#[test]
fn compact_answer_reads_the_same() {
    let c = r#"{"head":{"vars":["n"]},"results":{"bindings":[{"n":{"type":"literal","value":"4"}}]}}"#;
    assert_eq!(wears_all_hats(c), Ok(4));
}

#[test]
fn negative_three_hats_refuses_naming_three() {
    assert_eq!(wears_all_hats(&fuseki("3")), Err(3));
}

#[test]
fn negative_unreadable_answer_refuses_as_zero() {
    assert_eq!(wears_all_hats(""), Err(0));
    assert_eq!(wears_all_hats("<html>502</html>"), Err(0));
    assert_eq!(wears_all_hats(r#"{"results":{"bindings":[]}}"#), Err(0));
    assert_eq!(sparql_count(&fuseki("4"), "m"), None, "another variable is not this count");
}
