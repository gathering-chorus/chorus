// @domain: domains — placed by chorus-crawl's domain rules (#4538)
// #3752 reopen 1 — subject retirements are checked with one presence query per
// graph instead of a count + dump per subject (26.4s of a 27.4s deploy).
use athena_deploy::{subjects_from_answer, subjects_present_query};

#[test]
fn one_query_names_every_subject_in_its_graph() {
    let q = subjects_present_query("urn:chorus:domains:security", &["https://x#a", "https://x#b"]);
    assert!(q.contains("VALUES ?s { <https://x#a> <https://x#b> }"), "{q}");
    assert!(q.contains("GRAPH <urn:chorus:domains:security> { ?s ?p ?o }"), "{q}");
}

#[test]
fn the_answer_names_only_the_subjects_still_there() {
    let body = r#"{"head":{"vars":["s"]},"results":{"bindings":[{"s":{"type":"uri","value":"https://x#b"}}]}}"#;
    let present = subjects_from_answer(body).unwrap();
    assert!(present.contains("https://x#b"));
    assert!(!present.contains("https://x#a"), "a is gone, so it is not named");
    let none = subjects_from_answer(r#"{"head":{"vars":["s"]},"results":{"bindings":[]}}"#).unwrap();
    assert!(none.is_empty(), "every subject gone: an empty, answered set");
}

#[test]
fn negative_proof_an_unanswered_query_is_not_read_as_all_absent() {
    // a store that errors or times out: no bindings, so the caller checks each subject itself
    assert_eq!(subjects_from_answer(""), None);
    assert_eq!(subjects_from_answer("Error 503: Service Unavailable"), None);
}
