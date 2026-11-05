// @test-type: unit — pure: AC check parsing over card text; no cards CLI.
// @domain: proving
// @card: #4889
//! AC9: no two AC point to the same check file, and every AC names one (AC6: a card with
//! no checks fails). validate refuses before any review runs.
use werk_demo::ac_check_problems;

#[test]
fn every_ac_with_its_own_check_has_no_problem() {
    let v = "  - [x] a. check: test t/a.bats\n  - [ ] b. check: http /health 200\n";
    assert!(ac_check_problems(v).is_empty(), "{:?}", ac_check_problems(v));
}

#[test]
fn negative_proof_two_ac_on_one_check_file_are_refused() {
    let v = "  - [x] a. check: test t/x.bats\n  - [x] b. check: test t/x.bats\n";
    assert_eq!(ac_check_problems(v), vec!["AC 2 shares its check `test t/x.bats` with AC 1"]);
}

#[test]
fn negative_proof_an_ac_with_no_check_is_refused() {
    let v = "  - [x] a. check: test t/a.bats\n  - [ ] b, after land, by hand\n";
    assert_eq!(ac_check_problems(v), vec!["AC 2 names no check"]);
}

#[test]
fn negative_proof_a_check_word_with_no_target_is_no_check() {
    // "check:" alone (or with a kind but no target) names nothing to run
    let v = "  - [x] a. check: test\n";
    assert_eq!(ac_check_problems(v), vec!["AC 1 names no check"]);
}

#[test]
fn this_cards_own_ac_pass() {
    let v = include_str!("fixtures/ac_4889.txt");
    assert!(ac_check_problems(v).is_empty(), "{:?}", ac_check_problems(v));
}
