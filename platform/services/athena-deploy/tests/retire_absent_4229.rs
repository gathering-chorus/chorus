//! #3536 — the truncate-by-default root. A deploy whose staging lacked the 34
//! live domains retired every one of them on 2026-06-26. Two rules came out of
//! it: retire is opt-in, and even opted in, zero domains in staging refuses.
use athena_deploy::{retire_absent_on, retire_guard_allows};

#[test]
fn retire_is_off_unless_someone_asks_for_it() {
    assert!(!retire_absent_on(None));
    assert!(!retire_absent_on(Some("0")));
    assert!(!retire_absent_on(Some("")));
    assert!(retire_absent_on(Some("1")));
}

#[test]
fn negative_proof_anything_that_is_not_one_leaves_it_off() {
    // "true", "yes", "on" must NOT turn truncation on. A loose reading here is
    // the 06-26 wipe waiting for a typo.
    for loose in ["true", "yes", "on", "TRUE", "2", "-1"] {
        assert!(!retire_absent_on(Some(loose)), "{loose:?} must not enable retire");
    }
}

#[test]
fn a_staging_graph_with_domains_may_retire() {
    assert!(retire_guard_allows(Some(34)).is_ok());
    assert!(retire_guard_allows(Some(1)).is_ok());
}

#[test]
fn negative_proof_zero_domains_in_staging_refuses_rather_than_wiping() {
    let why = retire_guard_allows(Some(0)).unwrap_err();
    assert!(why.contains("0 domain subjects"), "{why}");
}

#[test]
fn negative_proof_an_unanswered_count_is_not_zero_and_not_a_licence() {
    // "the store did not answer" and "staging is empty" are different states.
    // Reading the first as the second deletes everything on a timeout.
    assert!(retire_guard_allows(None).is_err());
    assert_ne!(
        retire_guard_allows(None).unwrap_err(),
        retire_guard_allows(Some(0)).unwrap_err(),
        "the two refusals must say different things"
    );
}
