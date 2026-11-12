// @test-type: unit — runs a fake policy-validate in a temp home; no store, no model, no network
// @domain: cicd
// @card: #4959
//! #4959 AC4: every demo review runs the policies for its gate. A policy above its
//! baseline fails the review and is never downgraded to a note; a policy run that
//! could not measure is an error, never a pass. The checker is the CANONICAL one,
//! so a card cannot rewrite the check run on it; until it is on main, the review
//! says so as a note (the card that adds it cannot be gated by it).
//! Exercises werk-demo src/lib.rs policy_verdict, policy_review, with_policies.
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use werk_demo::{policy_review, policy_verdict, with_policies};

fn tmp_home(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("policy-gates-4959-{}-{}", std::process::id(), tag));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn home_with(tag: &str, script: &str) -> std::path::PathBuf {
    let d = tmp_home(tag);
    let p = d.join("platform/scripts/policy-validate");
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, script).unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    d
}

#[test]
fn exit_0_passes_with_the_summary() {
    let (r, f) = policy_verdict(Some(0), "PASS a 0 <= 0\npolicy-validate: 1 checks, 0 above baseline, 0 could not run\n");
    assert_eq!(r, "pass", "{f}");
    assert!(f.contains("policies: 1 checks"), "{f}");
}

#[test]
fn negative_proof_exit_1_fails_and_names_each_policy_over_its_baseline() {
    let (r, f) = policy_verdict(Some(1), "FAIL raw-auth 3 > 2 (arch)\nPASS b 0 <= 0\npolicy-validate: 2 checks, 1 above baseline, 0 could not run\n");
    assert_eq!(r, "fail");
    assert!(f.contains("FAIL raw-auth 3 > 2"), "{f}");
    assert!(!f.contains("PASS b"), "only the failures are findings: {f}");
}

#[test]
fn negative_proof_exit_2_or_a_crash_is_an_error_never_a_pass() {
    assert_eq!(policy_verdict(Some(2), "BROKEN x: query failed\n").0, "error");
    assert_eq!(policy_verdict(None, "").0, "error");
    assert_eq!(policy_verdict(Some(127), "").0, "error");
}

#[test]
fn the_review_runs_the_canonical_checker_with_its_gate() {
    let home = home_with("arch", "#!/bin/sh\n[ \"$1\" = --gate ] && [ \"$2\" = arch ] || exit 2\necho 'FAIL p 1 > 0 (arch)'\nexit 1\n");
    let (r, f) = policy_review(&home, None, "arch");
    assert_eq!(r, "fail", "{f}");
    assert!(f.contains("FAIL p 1 > 0"), "{f}");
}

#[test]
fn negative_proof_no_checker_on_canonical_or_in_the_werk_is_an_error_never_a_pass() {
    let home = tmp_home("absent");
    let werk = tmp_home("absent-werk");
    let (r, f) = policy_review(&home, Some(&werk), "code");
    assert_eq!(r, "error", "{f}");
    assert!(!Path::new(&home.join("platform/scripts/policy-validate")).exists());
}

#[test]
fn the_canonical_checker_wins_over_the_werks_copy() {
    // a card that rewrites policy-validate to always pass is not what runs on it
    let home = home_with("canon", "#!/bin/sh
echo 'FAIL p 1 > 0 (code)'
exit 1
");
    let werk = home_with("werk-lies", "#!/bin/sh
echo 'policy-validate: 0 checks, 0 above baseline, 0 could not run'
exit 0
");
    assert_eq!(policy_review(&home, Some(&werk), "code").0, "fail");
}

#[test]
fn only_while_canonical_has_none_does_the_werks_copy_run_and_it_says_so() {
    let home = tmp_home("boot");
    let werk = home_with("boot-werk", "#!/bin/sh
echo 'policy-validate: 0 checks, 0 above baseline, 0 could not run'
exit 0
");
    let (r, f) = policy_review(&home, Some(&werk), "code");
    assert_eq!(r, "pass", "{f}");
    assert!(f.contains("the werk's copy"), "{f}");
}

#[test]
fn a_recorded_policy_line_never_carries_into_the_ledger() {
    let prior = "policies: FAIL p 1 > 0 | a.rs:1 — b is undefined | [part 1/2 fail] policies: 0 checks";
    assert_eq!(werk_demo::without_policies(prior), "a.rs:1 — b is undefined");
}

#[test]
fn negative_proof_a_policy_failure_survives_a_passing_review() {
    // the model review passed (or its findings became notes): a policy over its
    // baseline still fails the gate
    let (r, f) = with_policies("pass", "note: a.rs:1 — style", ("fail".into(), "policies: FAIL p 1 > 0".into()));
    assert_eq!(r, "fail", "{f}");
    assert!(f.contains("note: a.rs:1") && f.contains("FAIL p 1 > 0"), "{f}");
}

#[test]
fn a_policy_error_makes_the_gate_unmeasured() {
    let (r, _) = with_policies("pass", "", ("error".into(), "policies: BROKEN".into()));
    assert_eq!(r, "error");
}

#[test]
fn passing_policies_leave_the_review_verdict_alone() {
    assert_eq!(with_policies("fail", "x", ("pass".into(), "policies: 0 checks".into())).0, "fail");
    assert_eq!(with_policies("pass", "", ("pass".into(), "policies: 0 checks".into())).0, "pass");
}

#[test]
fn validate_runs_the_data_and_product_policies_and_passes_when_both_do() {
    let home = home_with("dp-pass", "#!/bin/sh\necho \"policy-validate: 0 checks, 0 above baseline, 0 could not run (gate $2)\"\nexit 0\n");
    let r = werk_demo::deterministic_gates(&home, None);
    assert!(r.is_ok(), "{r:?}");
    let s = r.unwrap();
    assert!(s.contains("data") && s.contains("product"), "{s}");
}

#[test]
fn negative_proof_validate_refuses_when_a_product_policy_is_over_its_baseline() {
    let home = home_with("dp-fail", "#!/bin/sh\n[ \"$2\" = product ] && { echo 'FAIL jeff-only-go 1 > 0 (product)'; exit 1; }\nexit 0\n");
    let e = werk_demo::deterministic_gates(&home, None).unwrap_err();
    assert!(e.contains("product") && e.contains("FAIL jeff-only-go 1 > 0"), "{e}");
}

#[test]
fn negative_proof_validate_refuses_when_a_data_policy_cannot_measure() {
    let home = home_with("dp-err", "#!/bin/sh\n[ \"$2\" = data ] && { echo 'BROKEN q: query failed'; exit 2; }\nexit 0\n");
    let e = werk_demo::deterministic_gates(&home, None).unwrap_err();
    assert!(e.contains("data") && e.contains("could not measure"), "{e}");
}

#[test]
fn negative_proof_every_failure_is_labelled_so_none_leaks_into_the_ledger() {
    let (_, f) = policy_verdict(Some(1), "FAIL a 1 > 0 (code)\nFAIL b 2 > 0 (code)\n");
    assert_eq!(werk_demo::without_policies(&f), "", "{f}");
}

#[test]
fn the_checker_runs_in_the_cards_werk() {
    let home = home_with("cwd", "#!/bin/sh\n[ -f card-marker ] || { echo 'FAIL ran-outside-the-werk 1 > 0'; exit 1; }\necho 'policy-validate: 1 checks, 0 above baseline, 0 could not run'\n");
    let werk = tmp_home("cwd-werk");
    fs::write(werk.join("card-marker"), "").unwrap();
    let (r, f) = policy_review(&home, Some(&werk), "code");
    assert_eq!(r, "pass", "{f}");
}
