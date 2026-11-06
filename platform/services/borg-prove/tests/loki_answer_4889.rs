// @test-type: unit — pure: Loki answers and the card's window; no Loki, no board
// @domain: proving
// @card: #4889
//! Land 11-05: borg-verify read Loki's 429 ("too many outstanding requests") as "no result
//! yet" for all 19 now-checks, twice, and asked 30 days per check. Now the window starts at
//! the card's creation and an answer that is not 200 is an error.
//! Exercises borg-prove src/lib.rs loki_seen, test_seen, card_created, judge.
use borg_prove::{card_created, checks_from_card, judge, loki_seen, test_seen, Outcome};

const ANSWER: &str = r#"{"data":{"result":[{"values":[["1793922046412681000","{\"event\":\"test.case.passed\",\"trace\":\"t1\"}"]]}]}}"#;

#[test]
fn a_200_answer_is_read_for_its_newest_case() {
    assert_eq!(loki_seen("200", ANSWER).unwrap().map(|(_, r)| r).as_deref(), Some("pass"));
    assert_eq!(loki_seen("200", r#"{"data":{"result":[]}}"#), Ok(None));
}

#[test]
fn negative_proof_a_429_is_an_error_never_no_result() {
    let e = loki_seen("429", "too many outstanding requests").unwrap_err();
    assert!(e.starts_with("error: Loki answered 429"), "{e}");
    let c = checks_from_card(4889, "## AC\n- [x] a check: test platform/tests/x.bats\n").remove(0);
    let seen = test_seen(Err(e), Ok(None));
    match judge(&c, &seen) {
        Outcome::Unmeasured(why) => assert!(why.contains("Loki answered 429") && !why.contains("no result yet"), "{why}"),
        o => panic!("a refused lookup must be an unmeasured error, got {o:?}"),
    }
    // and with no error and no events it is still "no result yet"
    match judge(&c, &test_seen(Ok(None), Ok(None))) {
        Outcome::Unmeasured(why) => assert!(why.contains("no result yet"), "{why}"),
        o => panic!("{o:?}"),
    }
}

#[test]
fn the_window_starts_at_the_cards_creation() {
    let view = "#4889 one proof path\n  Status:   WIP\n  Created:  2026-11-04T22:51:49Z\n  Updated:  2026-11-05T19:21:25Z\n";
    assert_eq!(card_created(view).as_deref(), Some("2026-11-04T22:51:49Z"));
    assert_eq!(card_created("no header"), None, "NEGATIVE PROOF: no Created line, no window");
}
