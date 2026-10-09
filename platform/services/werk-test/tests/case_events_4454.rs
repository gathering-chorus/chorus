// @domain: tests
//! #4454 — every test case is an event on the run's trace, with its time and,
//! when it failed, its reason. Jeff, 2026-10-07: "unless we have this level of
//! detailed log and trace we are just guessing".

use werk_test::{
    bats_case_times, batch_line, case_event_args, case_event_args_at, parse_nextest_case_runs, run_trace_id, case_tsv_times, nextest_case_times, parse_bats_cases,
    CaseResult,
};

// bats -T output, as bats 1.13 prints it
const BATS_T: &str = "1..3\nok 1 a ok in 6ms\nok 2 b skip in 8ms # skip why\nnot ok 3 c bad in 5ms\n# (in test file t.bats, line 4)\n";

fn case(name: &str, result: &str) -> CaseResult {
    CaseResult { file_path: "platform/tests/t.bats".into(), test_name: name.into(), result: result.into() }
}

#[test]
fn bats_timing_keeps_the_case_names_and_verdicts() {
    assert_eq!(
        parse_bats_cases(BATS_T),
        vec![("a ok".into(), "pass".into()), ("b skip".into(), "skip".into()), ("c bad".into(), "fail".into())]
    );
}

// NEGATIVE PROOF: the timing suffix is really there to strip — a parse that
// ignored it would store "a ok in 6ms", a name the registry never holds.
#[test]
fn the_raw_timing_line_is_not_the_case_name() {
    assert!(BATS_T.contains("ok 1 a ok in 6ms"));
    assert!(!parse_bats_cases(BATS_T).iter().any(|(n, _)| n.contains(" in ") || n.ends_with("ms")));
}

#[test]
fn each_runner_reports_its_case_times() {
    let b = bats_case_times(BATS_T);
    assert_eq!((b.get("a ok"), b.get("b skip"), b.get("c bad")), (Some(&6), Some(&8), Some(&5)));

    let n = nextest_case_times("        PASS [   0.012s] werk-test::units a_case\n        FAIL [   1.500s] werk-test::units b_case\n");
    assert_eq!((n.get("a_case"), n.get("b_case")), (Some(&12), Some(&1500)));

    let j = case_tsv_times("/w/a.test.ts\tsuite does x\tpassed\t41\n/w/a.test.ts\tsuite no time\tpassed\t\n");
    assert_eq!(j.get("suite does x"), Some(&41));
    assert_eq!(j.get("suite no time"), None, "no duration is absent, never 0");
}

#[test]
fn a_failed_case_event_carries_its_reason_time_and_level() {
    let a = case_event_args(&case("c bad", "fail"), Some(5), "platform/tests/t.bats", "assertion",
        "expected 2 got 3", "nightly", "4454", "tr-1");
    assert_eq!(a[0], "test.case.failed");
    for want in ["level=error", "reason=expected 2 got 3", "failure_kind=assertion", "elapsed_ms=5",
                 "case=c bad", "file=platform/tests/t.bats", "trace=tr-1", "card=4454"] {
        assert!(a.iter().any(|x| x == want), "missing {want} in {a:?}");
    }
}

// NEGATIVE PROOF: a passed case is not stamped as a failure and carries no
// reason — if every case read as failed, the red would name nothing.
#[test]
fn a_passed_case_event_is_info_with_no_reason() {
    let a = case_event_args(&case("a ok", "pass"), Some(6), "u", "", "", "nightly", "4454", "tr-1");
    assert_eq!(a[0], "test.case.passed");
    assert!(!a.iter().any(|x| x == "level=error" || x.starts_with("reason=")), "{a:?}");
    let s = case_event_args(&case("b skip", "skip"), None, "u", "", "", "nightly", "4454", "tr-1");
    assert_eq!(s[0], "test.case.skipped");
    assert!(!s.iter().any(|x| x.starts_with("elapsed_ms=")), "no time given, none invented: {s:?}");
}

#[test]
fn a_batch_line_is_one_event_even_when_a_value_has_a_tab_or_newline() {
    let line = batch_line(&["test.case.failed".into(), "nightly".into(), "reason=a\tb\nc".into()]);
    assert_eq!(line.split('\t').count(), 3);
    assert!(!line.contains('\n'));
}

// The one door a test child's own events take to the spine: test.* only.
#[test]
fn a_childs_test_events_are_forwarded() {
    let got = werk_test::forwardable_test_events(
        "test.fixture.ready\ttests\tport=1\ntest.case.started\ttests\tcase=a\n");
    assert_eq!(got.len(), 2);
}

// NEGATIVE PROOF: a test cannot use the door to write anything else on the
// live spine — a card or deploy event from a child is dropped, as is a line
// with no role.
#[test]
fn a_child_cannot_forward_a_non_test_event() {
    let got = werk_test::forwardable_test_events(
        "card.moved\ttests\tcard=1\ndeploy.completed\tkade\nservice.started\tsystem\ntest.case.started\n\n");
    assert!(got.is_empty(), "{got:?}");
}

#[test]
fn a_run_with_no_trace_mints_one_and_a_given_trace_is_kept() {
    // #4454 reopen: launchd gives the nightly no CHORUS_TRACE_ID.
    assert_eq!(run_trace_id(None, 1791560000000, 42), "nightly-1791560000000-42");
    assert_eq!(run_trace_id(Some("tr-card-run"), 1, 1), "tr-card-run", "a card run's trace is not replaced");
}

#[test]
fn negative_proof_an_empty_or_blank_trace_is_never_kept() {
    // the 10-09 03:00 state: the variable unset or empty -> every case event had trace=""
    for given in [Some(""), Some("   ")] {
        let t = run_trace_id(given, 7, 8);
        assert!(!t.trim().is_empty() && t.starts_with("nightly-"), "blank trace survived: {t:?}");
    }
}

// #4454 reopen — chorus-hooks builds hooks/ and shared/ into its library AND
// its program, so nextest runs each of those tests twice (10-09 10:21 run: 662
// repeats, observer.rs 63 tests → 126 events). Two runs are two events; the
// build target and the full path are what tell them apart.
const NEXTEST_TWICE: &str = "    PASS [   0.020s] chorus-hooks hooks::observer::tests::test_truncate_short\n\
    PASS [   0.021s] chorus-hooks::bin/chorus-hooks hooks::observer::tests::test_truncate_short\n\
    FAIL [   0.100s] chorus-hooks::pulse_roles_4077 counts_roles\n";

#[test]
fn nextest_runs_keep_the_build_target() {
    let runs = parse_nextest_case_runs(NEXTEST_TWICE);
    assert_eq!(runs.len(), 3);
    assert_eq!(runs[0], ("hooks::observer::tests::test_truncate_short".into(), "chorus-hooks".into(), "pass".into()));
    assert_eq!(runs[1].1, "chorus-hooks::bin/chorus-hooks");
    assert_eq!(runs[2], ("counts_roles".into(), "chorus-hooks::pulse_roles_4077".into(), "fail".into()));
    // same order and verdicts as the path parser the join uses, so the two line up
    let paths = werk_test::parse_nextest_case_paths(NEXTEST_TWICE);
    assert_eq!(paths.iter().map(|(p, r)| (p.clone(), r.clone())).collect::<Vec<_>>(),
               runs.iter().map(|(p, _, r)| (p.clone(), r.clone())).collect::<Vec<_>>());
}

#[test]
fn a_cargo_case_event_names_its_target_and_path() {
    let c = CaseResult { file_path: "platform/services/chorus-hooks/src/hooks/observer.rs".into(),
                         test_name: "test_truncate_short".into(), result: "pass".into() };
    let a = case_event_args_at(&c, Some(21), "chorus-hooks", "", "", "nightly", "4454", "tr-1",
        "chorus-hooks::bin/chorus-hooks", "hooks::observer::tests::test_truncate_short");
    for want in ["target=chorus-hooks::bin/chorus-hooks", "path=hooks::observer::tests::test_truncate_short",
                 "case=test_truncate_short"] {
        assert!(a.iter().any(|x| x == want), "missing {want} in {a:?}");
    }
}

// NEGATIVE PROOF: the two runs of one test are told apart — if target were
// dropped, both events would carry the same keys and read as one test logged twice.
#[test]
fn the_two_runs_of_one_test_differ_by_target() {
    let c = CaseResult { file_path: "f.rs".into(), test_name: "t".into(), result: "pass".into() };
    let runs = parse_nextest_case_runs(NEXTEST_TWICE);
    let a = case_event_args_at(&c, None, "u", "", "", "nightly", "4454", "tr-1", &runs[0].1, &runs[0].0);
    let b = case_event_args_at(&c, None, "u", "", "", "nightly", "4454", "tr-1", &runs[1].1, &runs[1].0);
    assert_ne!(a, b);
    // a case with no target (bats, jest) carries no empty target field
    let plain = case_event_args(&c, None, "u", "", "", "nightly", "4454", "tr-1");
    assert!(!plain.iter().any(|x| x.starts_with("target=") || x.starts_with("path=")), "{plain:?}");
}
