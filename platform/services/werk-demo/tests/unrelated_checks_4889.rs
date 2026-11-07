// @test-type: unit — pure: AC check headers over card text; files read through a stand-in.
// @domain: cicd
// @card: #4889
//! AC8: the code review flags an AC whose check is an unrelated test (its header never
//! names the card) or a missing file. Exercises werk-demo src/lib.rs unrelated_checks.
use werk_demo::unrelated_checks;

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
