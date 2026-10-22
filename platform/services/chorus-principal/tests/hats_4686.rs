// @domain: security
// #4686 — step 3c read Fuseki's COUNT answer with a `"value":"` needle, but
// Fuseki writes `"value": "4"`, so every create read 0 of 4 hats and rolled back.
use chorus_principal::{links::half_made_webid, sparql_count, wears_all_hats, HATS};

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
    assert_eq!(wears_all_hats(&fuseki("4")), Ok(HATS));
}

#[test]
fn compact_answer_reads_the_same() {
    let c = r#"{"head":{"vars":["n"]},"results":{"bindings":[{"n":{"type":"literal","value":"4"}}]}}"#;
    assert_eq!(wears_all_hats(c), Ok(4));
}

#[test]
fn negative_three_hats_refuses_naming_three() {
    assert_eq!(wears_all_hats(&fuseki("3")), Err(Some(3)));
    assert_eq!(wears_all_hats(&fuseki("0")), Err(Some(0)), "zero hats is a count, not unreadable");
}

#[test]
fn negative_unreadable_answer_refuses_as_unreadable() {
    assert_eq!(wears_all_hats(""), Err(None));
    assert_eq!(wears_all_hats("<html>502</html>"), Err(None));
    assert_eq!(wears_all_hats(r#"{"results":{"bindings":[]}}"#), Err(None));
    assert_eq!(sparql_count(&fuseki("4"), "m"), None, "another variable is not this count");
}

// The service account's webIdLinks answer, the shape CSS serves it in.
const LINKS: &str = r#"{"webIdLinks":{"https://id.lightlifeurbangardens.com/borg/profile/card#me":"http://x/l/1","https://id.lightlifeurbangardens.com/borg-queen/profile/card#me":"http://x/l/2"}}"#;

#[test]
fn an_agent_pod_the_service_account_links_is_resumed() {
    assert_eq!(half_made_webid(LINKS, "borg-queen").as_deref(), Some("https://id.lightlifeurbangardens.com/borg-queen/profile/card#me"));
}

#[test]
fn negative_no_link_by_that_exact_name_is_not_resumed() {
    assert_eq!(half_made_webid(LINKS, "queen"), None, "a suffix is not the name");
    assert_eq!(half_made_webid(LINKS, "borg-q"), None, "a prefix is not the name");
    assert_eq!(half_made_webid(r#"{"webIdLinks":{}}"#, "borg-queen"), None);
    assert_eq!(half_made_webid("", "borg-queen"), None, "an unreadable answer resumes nothing");
}
