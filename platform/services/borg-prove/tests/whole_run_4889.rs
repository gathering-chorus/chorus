// @test-type: unit
// @domain: proving
//! #4889 (Kade 18:08) — an AC's test check reads the whole newest run of its
//! test, not one case. Loki answered limit=1 before, so the newest single case
//! decided: a pass hid a fail, and a skip was never read.
use borg_prove::{judge, latest_case, Check, Outcome};

fn body(rows: &[(&str, &str, &str)]) -> String { // (ts, event, trace)
    let vals: Vec<String> = rows.iter()
        .map(|(ts, ev, tr)| format!(r#"["{ts}","{{\"event\":\"test.case.{ev}\",\"file\":\"x.bats\",\"trace\":\"{tr}\"}}"]"#))
        .collect();
    format!(r#"{{"data":{{"result":[{{"stream":{{}},"values":[{}]}}]}}}}"#, vals.join(","))
}

fn check() -> Check {
    Check { ac: "ac-1".into(), statement: "s".into(), kind: "test".into(), target: "x.bats".into(), expect: "pass".into() }
}

#[test]
fn every_case_of_the_newest_run_passing_is_a_pass() {
    let b = body(&[("300", "passed", "r2"), ("299", "passed", "r2"), ("100", "failed", "r1")]);
    assert_eq!(latest_case(&b).map(|x| x.1).as_deref(), Some("pass"), "an older run's fail is history");
}

// NEGATIVE PROOF: a pass and a fail in the newest run is Not met, even when the pass is newer
#[test]
fn a_failed_case_in_the_newest_run_is_not_met_even_under_a_newer_pass() {
    let b = body(&[("300", "passed", "r2"), ("250", "failed", "r2")]);
    assert_eq!(latest_case(&b).map(|x| x.1).as_deref(), Some("fail"));
    assert!(matches!(judge(&check(), &Some("fail".into())), Outcome::NotMet(_)));
}

// NEGATIVE PROOF: a skipped case in the newest run measured nothing
#[test]
fn a_skipped_case_in_the_newest_run_is_unmeasured() {
    let b = body(&[("300", "passed", "r2"), ("250", "skipped", "r2")]);
    assert_eq!(latest_case(&b).map(|x| x.1).as_deref(), Some("skip"));
    assert!(matches!(judge(&check(), &Some("skip".into())), Outcome::Unmeasured(_)));
}

// /code-review 18:34: a trace-less case is never grouped with an older run
#[test]
fn a_case_with_no_trace_is_judged_alone_not_with_old_runs() {
    let b = r#"{"data":{"result":[{"stream":{},"values":[
        ["300","{\"event\":\"test.case.passed\",\"file\":\"x.bats\"}"],
        ["100","{\"event\":\"test.case.failed\",\"file\":\"x.bats\"}"]]}]}}"#;
    assert_eq!(latest_case(b).map(|x| x.1).as_deref(), Some("pass"));
}

// NEGATIVE PROOF (/code-review 18:34): a full answer still inside the newest run may have lost cases
#[test]
fn a_full_answer_that_never_leaves_the_newest_run_is_unmeasured() {
    let rows: Vec<(String, &str, &str)> = (0..borg_prove::CASE_LIMIT).map(|i| ((1000 + i).to_string(), "passed", "r9")).collect();
    let refs: Vec<(&str, &str, &str)> = rows.iter().map(|(t, e, r)| (t.as_str(), *e, *r)).collect();
    assert_eq!(latest_case(&body(&refs)).map(|x| x.1).as_deref(), Some("skip"));
}


/// #4889 (code review, run 9) — a deferred check whose reopen failed stays registered.
#[test]
fn a_failed_reopen_keeps_the_deferred_check() {
    use borg_prove::{keeps_deferral, Rerun};
    assert!(keeps_deferral(&Rerun::Reopen, false), "a reopen that did not take must not drop the check");
    assert!(!keeps_deferral(&Rerun::Reopen, true));
    assert!(keeps_deferral(&Rerun::KeepOpen, true));
    assert!(!keeps_deferral(&Rerun::Cleared, false));
}

/// #4889 (code review, run 14) — a card whose every AC is a `check: after` can land: its
/// checks re-run after the land and reopen it on a red. Negative proof: all `by` still
/// proves nothing, and an `after` beside an unmeasured AC is still Unproven.
#[test]
fn a_card_with_only_after_checks_is_proven_and_lands() {
    use borg_prove::{checks_from_card, judge, verdict, Outcome};
    let after = checks_from_card(1, "## AC\n- [ ] a check: after test t/a.bats\n").remove(0);
    let o = judge(&after, &None);
    assert!(matches!(o, Outcome::Deferred(_)), "{o:?}");
    assert_eq!(verdict(&[(after.clone(), o.clone())]), "Proven");
    let by = checks_from_card(1, "## AC\n- [ ] b check: by #4805\n").remove(0);
    assert_eq!(verdict(&[(by.clone(), Outcome::Deferred("later".into()))]), "Unproven");
    assert_eq!(verdict(&[(after, o), (by, Outcome::Unmeasured("x".into()))]), "Unproven");
}
