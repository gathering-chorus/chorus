// @domain: builds — placed by chorus-crawl's domain rules (#4538)
//! #4412 run 13 — a changed browser flow spec under proving/flows/ was refused
//! as "a test the pipeline has no runner for", though the UI lane runs that
//! folder (ui_lane_fires counts proving/flows/, playwright.config testDir).

#[test]
fn a_flow_spec_under_proving_flows_is_runnable() {
    assert!(werk_test::runnable_test("proving/flows/clearing-room-key-3865.spec.cjs", false));
    assert!(werk_test::runnable_test("proving/flows/clearing-lifecycle.spec.cjs", false));
    assert!(werk_test::runnable_test("proving/flows/nested/a.spec.ts", false));
}

#[test]
fn negative_proof_a_spec_outside_proving_flows_or_a_package_is_still_refused() {
    assert!(!werk_test::runnable_test("proving/other/a.spec.cjs", false));
    assert!(!werk_test::runnable_test("designing/x/a.spec.cjs", false));
    // a non-spec file in proving/flows/ is not a test the UI lane runs
    assert!(!werk_test::runnable_test("proving/flows/helpers.cjs", false));
}
