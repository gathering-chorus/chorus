// @test-type: unit — signal:api is fixture-data
// @domain: tests
//! #4531 — timing checks run in the nightly only (Jeff 2026-10-14 18:25: "we
//! must not run perf tests in demo"). A card run set WERK_TEST_NIGHTLY=1 too,
//! so suites could not tell the lanes apart; WERK_TEST_LANE can.
use werk_test::{lane_name, timing_checks_on, LANE_ENV};

#[test]
fn the_lanes_have_different_names_and_only_nightly_turns_timing_on() {
    assert_eq!(lane_name(true), "nightly");
    assert_eq!(lane_name(false), "card");
    std::env::set_var(LANE_ENV, lane_name(true));
    assert!(timing_checks_on());
    // NEGATIVE PROOF: a card run, and an unset shell, keep timing checks off.
    std::env::set_var(LANE_ENV, lane_name(false));
    assert!(!timing_checks_on());
    std::env::remove_var(LANE_ENV);
    assert!(!timing_checks_on());
}
