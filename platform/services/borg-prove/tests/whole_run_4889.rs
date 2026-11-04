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
