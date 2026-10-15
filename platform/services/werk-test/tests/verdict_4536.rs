// @test-type: unit
// @domain: tests
//! #4536 — every run ends in exactly one result, and a red means the product broke.
//!
//! CLEAN 0 · RED 1 · UNMEASURED 2 (each item names its reason) · REFUSED 3.
//! Results that could not be stored never change the verdict.

use werk_test::{ran_nothing, storage_line, unmeasured_is, verdict, CheckKind, Verdict};

fn reasons(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn every_case_ran_and_passed_is_clean_exit_0() {
    let v = verdict(12, false, &[]);
    assert_eq!(v, Verdict::Clean);
    assert_eq!(v.exit_code(), 0);
}

#[test]
fn a_failed_case_is_red_exit_1() {
    let v = verdict(12, true, &[]);
    assert_eq!(v, Verdict::Red);
    assert_eq!(v.exit_code(), 1);
}

#[test]
fn something_that_could_not_run_is_unmeasured_exit_2_with_its_reason() {
    let v = verdict(12, false, &reasons(&["cargo: missing nextest"]));
    assert_eq!(v, Verdict::Unmeasured(reasons(&["cargo: missing nextest"])));
    assert_eq!(v.exit_code(), 2);
    assert!(v.label().contains("missing nextest"), "the reason is in the label: {}", v.label());
}

#[test]
fn red_wins_over_unmeasured() {
    let v = verdict(12, true, &reasons(&["bats: did not start"]));
    assert_eq!(v, Verdict::Red);
    assert_eq!(v.exit_code(), 1);
}

#[test]
fn a_run_with_zero_cases_is_unmeasured_ran_nothing_never_clean() {
    let v = verdict(0, false, &[]);
    assert_eq!(v, Verdict::Unmeasured(reasons(&["ran nothing"])));
    assert_eq!(v.exit_code(), 2);
}

#[test]
fn refused_is_exit_3_and_names_why() {
    let v = Verdict::Refused("no werk for #9999".into());
    assert_eq!(v.exit_code(), 3);
    assert!(v.label().contains("no werk for #9999"));
}

#[test]
fn results_not_stored_never_change_the_verdict() {
    // #4532 run 1: 9 of 9 suites passed, one write batch got HTTP 000, the run went red.
    let v = verdict(9, false, &[]);
    let line = storage_line(9, 0);
    assert_eq!(v.exit_code(), 0, "storage is not the product");
    assert_eq!(line.as_deref(), Some("results not stored: 9 of 9"));
}

#[test]
fn all_results_stored_prints_no_storage_line() {
    assert_eq!(storage_line(9, 9), None);
    assert_eq!(storage_line(0, 0), None);
}

#[test]
fn every_result_has_its_own_exit_code() {
    let codes = [
        Verdict::Clean.exit_code(),
        Verdict::Red.exit_code(),
        Verdict::Unmeasured(reasons(&["x"])).exit_code(),
        Verdict::Refused("x".into()).exit_code(),
    ];
    assert_eq!(codes, [0, 1, 2, 3]);
}

#[test]
fn a_case_lane_that_passed_with_zero_cases_ran_nothing() {
    for k in [CheckKind::CargoTest, CheckKind::Jest, CheckKind::Bats] {
        assert!(ran_nothing(k, true, 0), "{:?} with 0 cases is not a pass", k);
        assert!(!ran_nothing(k, true, 3), "{:?} with cases ran something", k);
        assert!(!ran_nothing(k, false, 0), "{:?} failed is RED, not unmeasured", k);
    }
}

#[test]
fn lanes_without_cases_by_design_never_ran_nothing() {
    for k in [CheckKind::Tsc, CheckKind::ClippyRatchet, CheckKind::LintRatchet, CheckKind::DocCoherence] {
        assert!(!ran_nothing(k, true, 0), "{:?} has no cases by design", k);
    }
}

#[test]
fn an_unmeasured_entry_names_its_unit_and_reason() {
    assert!(unmeasured_is("platform/tests/x.bats — not built", "platform/tests/x.bats"));
    assert!(!unmeasured_is("platform/tests/x.bats — not built", "platform/tests/x"));
}
