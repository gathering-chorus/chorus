// @test-type: unit — signal:api is fixture-data
// @domain: tests
//! #4531 — timing checks run in the nightly only (Jeff 2026-10-14 18:25: "we
//! must not run perf tests in demo"). A card run set WERK_TEST_NIGHTLY=1 too,
//! so suites could not tell the lanes apart; WERK_TEST_LANE can.
use werk_test::{lane_name, LANE_ENV};


#[test]
fn the_lanes_have_different_names() {
    assert_eq!(lane_name(true), "nightly");
    assert_eq!(lane_name(false), "card");
    // NEGATIVE PROOF: the two lanes can never read as one to a suite.
    assert_ne!(lane_name(true), lane_name(false));
    assert_eq!(LANE_ENV, "WERK_TEST_LANE");
}
