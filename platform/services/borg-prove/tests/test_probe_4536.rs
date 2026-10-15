// @test-type: unit
// @domain: tests
//! #4536 — a `test` check reads the test's latest result from Loki (the
//! runner's test.case.* events), not from TestResult rows, which the runner
//! no longer writes. Card runs and the nightly both log there, so the latest
//! answer is whichever ran last.

use borg_prove::{latest_case_result, test_logql};

#[test]
fn the_query_names_the_test_by_file_or_case_and_matches_the_event_exactly() {
    let q = test_logql("platform/tests/x.bats", 4530);
    assert!(q.starts_with(r#"{job="platform-chorus", appName="chorus-events"}"#), "{q}");
    assert!(q.contains(r#"| json | event=~"test.case.(passed|failed)""#), "{q}");
    assert!(q.contains(r#"file="platform/tests/x.bats" or case="platform/tests/x.bats""#), "{q}");
}

#[test]
fn a_quote_in_the_target_cannot_break_out_of_the_query() {
    let q = test_logql(r#"a" or file=~".*"#, 1);
    assert!(!q.contains(r#"a" or file=~".*""#), "{q}");
}

#[test]
fn the_newest_event_wins() {
    let body = r#"{"data":{"result":[{"stream":{},"values":[
        ["200","{\"event\":\"test.case.failed\",\"file\":\"x.bats\",\"case\":\"c\"}"],
        ["100","{\"event\":\"test.case.passed\",\"file\":\"x.bats\",\"case\":\"c\"}"]]}]}}"#;
    assert_eq!(latest_case_result(body).as_deref(), Some("fail"));
}

#[test]
fn no_events_is_no_answer_never_a_pass() {
    assert_eq!(latest_case_result(r#"{"data":{"result":[]}}"#), None);
    assert_eq!(latest_case_result("not json"), None);
}

// NEGATIVE PROOF (Silas 08:11): another card's branch run never answers for
// this card — the query is bound to the card being proved.
#[test]
fn only_the_card_being_proved_can_answer() {
    let q = test_logql("platform/tests/3706-model-census.bats", 4536);
    assert!(q.contains(r#"| card_id="4536" |"#), "{q}");
    assert!(!q.contains("4530"), "{q}");
}
