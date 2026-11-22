// @domain: code — placed by chorus-crawl's domain rules (#4538)
//! #4925 — the browser flows are registered one row per spec file, and the nightly's
//! ui stage stored no case rows, so all 15 read "no result" every night (Abby, 11-22).
//! Silas's proof: one failing spec turns its OWN row red, not just the lane.
use werk_test::flow_file_results;

const EVENTS: &str = "\
test.case.started\ttests\tfile=proving/flows/a.spec.cjs\tcase=loads
test.case.passed\ttests\tfile=proving/flows/a.spec.cjs\tcase=loads\tunit=playwright
test.case.passed\ttests\tfile=proving/flows/b.spec.cjs\tcase=signs in\tunit=playwright
test.case.failed\ttests\tfile=proving/flows/b.spec.cjs\tcase=shows the room\treason=expected 4 got 0\tunit=playwright
test.case.passed\ttests\tfile=proving/flows/b.spec.cjs\tcase=signs out\tunit=playwright
test.case.skipped\ttests\tfile=proving/flows/c.spec.cjs\tcase=needs a room\tunit=playwright
test.fixture.ready\ttests\tfixture=own-clearing
";

fn row(file: &str) -> (String, String, String) {
    let all = flow_file_results(EVENTS);
    let (c, why) = all.iter().find(|(c, _)| c.file_path == file).unwrap_or_else(|| panic!("no row for {file}: {all:?}"));
    (c.test_name.clone(), c.result.clone(), why.clone())
}

#[test]
fn one_row_per_spec_file_named_as_the_registry_names_it() {
    let all = flow_file_results(EVENTS);
    assert_eq!(all.len(), 3, "{all:?}");
    assert_eq!(row("proving/flows/a.spec.cjs"), ("a.spec.cjs".into(), "pass".into(), String::new()));
}

#[test]
fn negative_proof_one_failing_case_turns_its_own_spec_red_and_no_other() {
    let (_, result, why) = row("proving/flows/b.spec.cjs");
    assert_eq!(result, "fail", "a spec with a failed case passed");
    assert!(why.contains("shows the room") && why.contains("expected 4 got 0"), "reason lost: {why}");
    assert_eq!(row("proving/flows/a.spec.cjs").1, "pass", "a clean spec took another spec's red");
}

#[test]
fn negative_proof_a_spec_that_only_skipped_is_a_skip_never_a_pass() {
    assert_eq!(row("proving/flows/c.spec.cjs").1, "skip");
}

#[test]
fn the_nightly_ui_stage_stores_the_flow_rows() {
    // run-time, never env!() — the #4030 guard. The guard that the rows reach the run record: the ui stage hands them to store_unit
    let main = std::fs::read_to_string(std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("src/main.rs")).unwrap();
    let ui = main.split("── ui: the playwright flows").nth(1).expect("the ui stage is gone from main.rs");
    let stage = &ui[..ui.find("nightly-unit|ui|").expect("the ui unit line is gone")];
    assert!(stage.contains("flow_file_results(") && stage.contains("store_unit(\"proving/flows\""),
        "the ui stage no longer stores per-spec rows");
}

/// #4925 — an empty `unclassified` stage is the goal state (every test typed): a measured
/// zero, never a skipped suite. NEGATIVE: any other empty stage still says ABSENT, a skip.
#[test]
fn an_empty_unclassified_stage_is_a_measured_zero_and_others_stay_absent() {
    let u = werk_test::nightly_stage_absent_line("unclassified");
    assert!(u.contains("|pass|") && u.contains("none unclassified"), "{u}");
    let s = werk_test::nightly_stage_absent_line("security");
    assert!(s.contains("|skip|") && s.contains("ABSENT"), "{s}");
}
