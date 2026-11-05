// @test-type: unit — pure: AC check parsing over card text; no cards CLI.
// @domain: proving
// @card: #4889
//! AC9: no two AC point to the same check file, and every AC names one (AC6: a card with
//! no checks fails). validate refuses before any review runs.
use werk_demo::{ac_check_problems, unrelated_checks};

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
fn negative_proof_two_deferred_checks_are_told_apart_by_their_inner_target() {
    let v = "  - [ ] a. check: after test t/a.bats\n  - [ ] b. check: after test t/b.bats\n";
    assert!(ac_check_problems(v).is_empty(), "{:?}", ac_check_problems(v));
    let same = "  - [ ] a. check: after test t/a.bats\n  - [ ] b. check: test t/a.bats\n";
    assert_eq!(ac_check_problems(same), vec!["AC 2 shares its check `test t/a.bats` with AC 1"]);
}

#[test]
fn this_cards_own_ac_pass() {
    let v = include_str!("fixtures/ac_4889.txt");
    assert!(ac_check_problems(v).is_empty(), "{:?}", ac_check_problems(v));
}

// AC8 (#4889): the code review flags an AC whose check is an unrelated test
fn heads(path: &str) -> Option<String> {
    match path {
        "t/mine.bats" => Some("# @card: #4889 · owner: silas".to_string()),
        "t/other.bats" => Some("# @card: #48890 · owner: kade".to_string()),
        _ => None,
    }
}

#[test]
fn a_check_whose_header_names_the_card_is_related() {
    let v = "  - [x] a. check: test t/mine.bats\n  - [ ] b. check: after test t/mine.bats\n  - [ ] c. check: http /health 200\n";
    assert!(unrelated_checks(v, 4889, heads).is_empty(), "{:?}", unrelated_checks(v, 4889, heads));
}

#[test]
fn negative_proof_an_ac_whose_check_names_an_unrelated_test_is_flagged() {
    let v = "  - [x] a. check: test t/mine.bats\n  - [x] b. check: test t/other.bats\n";
    assert_eq!(unrelated_checks(v, 4889, heads), vec!["t/other.bats:1 — item 1 — AC 2's check never names #4889: an unrelated test"]);
}

#[test]
fn negative_proof_an_ac_whose_check_file_is_missing_is_flagged() {
    let v = "  - [ ] a. check: test t/gone.bats\n";
    assert_eq!(unrelated_checks(v, 4889, heads), vec!["t/gone.bats:1 — item 1 — AC 1's check does not exist"]);
}
