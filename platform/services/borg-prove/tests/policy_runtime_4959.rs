// @test-type: unit — pure: policy-validate --runtime's answer as borg-prove results; no prod, no board
// @domain: policies
// @card: #4959
//! #4959 AC6: after every land, borg-prove runs the runtime policies against prod. A
//! policy over its baseline is a NotMet row, so record() files one child card for it;
//! a run that could not measure is Unmeasured (the land is not proven); none over = Met.
//! Exercises borg-prove src/lib.rs policy_results, reds, child_title, child_desc.
use borg_prove::{child_desc, child_title, policy_results, reds, verdict, Outcome};

#[test]
fn every_runtime_policy_at_or_below_its_baseline_is_one_met_row() {
    let r = policy_results(4959, Some(0), "PASS bind-local 0 <= 0\npolicy-validate: 1 checks, 0 above baseline, 0 could not run\n");
    assert_eq!(r.len(), 1);
    assert!(matches!(r[0].1, Outcome::Met(_)), "{:?}", r[0].1);
    assert!(reds(&r).is_empty());
}

#[test]
fn negative_proof_each_policy_over_its_baseline_is_its_own_not_met_row_and_child() {
    let out = "FAIL bind-local 1 > 0 (arch)\nFAIL cdhash-matches 2 > 0 (ops)\npolicy-validate: 2 checks, 2 above baseline, 0 could not run\n";
    let r = policy_results(4959, Some(1), out);
    assert_eq!(r.len(), 2, "{r:?}");
    assert!(r.iter().all(|(_, o)| matches!(o, Outcome::NotMet(_))));
    assert_eq!(r[0].0.ac, "ac-4959-policy-bind-local");
    assert_eq!(reds(&r).len(), 2, "record() files one child per red");
    assert_ne!(verdict(&r), "Proven");
    let t = child_title(4959, &r[0].0);
    assert!(t.contains("ac-4959-policy-bind-local"), "{t}");
    let d = child_desc(4959, &r[0].0, &r[0].1);
    assert!(d.contains("check: test platform/tests/4959-borg-prove-policies-live.bats"), "the child passes intake: {d}");
    assert!(d.contains("1 > 0"), "{d}");
}

#[test]
fn negative_proof_a_run_that_could_not_measure_is_unmeasured_never_met() {
    for (code, out) in [(Some(2), "BROKEN x: door down\n"), (None, ""), (Some(127), "")] {
        let r = policy_results(4959, code, out);
        assert_eq!(r.len(), 1);
        assert!(matches!(r[0].1, Outcome::Unmeasured(_)), "{code:?}: {:?}", r[0].1);
        assert_ne!(verdict(&r), "Proven");
    }
}

#[test]
fn negative_proof_exit_1_with_no_fail_line_is_unmeasured_not_a_silent_pass() {
    let r = policy_results(4959, Some(1), "policy-validate: 0 checks\n");
    assert!(matches!(r[0].1, Outcome::Unmeasured(_)), "{:?}", r[0].1);
}
