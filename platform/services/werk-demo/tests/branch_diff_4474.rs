// @test-type: unit
// @domain: builds
//! #4474 reopen (Silas, 2026-10-11 15:09/15:10). werk.yml passed GO where
//! werk-demo reads the role, so role=false: every v1 demo-ready nudge went to
//! "false", and every in-run gate diffed chorus-werk/false-<card> — an empty
//! diff the reviewer could only pass, because unwrap_or_default hid the error.
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use werk_demo::branch_diff;

fn tmp(tag: &str) -> PathBuf {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let d = std::env::temp_dir().join(format!("wd-4474-{}-{}", tag, n));
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn negative_proof_a_werk_that_is_not_there_refuses_never_reads_as_empty() {
    let missing = tmp("gone").join("false-4474");
    let err = branch_diff(missing.to_str().unwrap()).unwrap_err();
    assert!(err.contains("no werk at"), "{err}");
}

#[test]
fn a_werk_git_cannot_diff_refuses_too() {
    let d = tmp("nogit");
    let ok = Command::new("git").args(["init", "-q"]).current_dir(&d).status().unwrap().success();
    assert!(ok);
    // no origin/main: git diff origin/main...HEAD fails
    let err = branch_diff(d.to_str().unwrap()).unwrap_err();
    assert!(err.contains("git diff failed"), "{err}");
}

#[test]
fn the_v2_pipeline_hands_werk_demo_the_card_and_never_the_go() {
    // #4515: werk.yml (v1) is retired. #4502: the v2 cicd pipeline calls each
    // demo step with the card and the role; the go is a separate human step
    // and never an argument.
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let yml = std::fs::read_to_string(format!("{dir}/../../../platform/pipelines/cicd.yaml")).unwrap();
    // #4502 — the steps live in the demo pipeline (Jeff 10-12 21:18), which
    // cicd starts from the card's own werk file as one step
    let demo = std::fs::read_to_string(format!("{dir}/../../../platform/pipelines/demo.yaml")).unwrap();
    for step in ["validate ${CARD} ${ROLE}", "run-gate ${CARD} ${ROLE} code", "run-review ${CARD} ${ROLE} 3",
                 "session-smoke ${CARD} ${ROLE}", "announce ${CARD} ${ROLE}"] {
        assert!(demo.contains(&format!("command: \"werk-demo {step}\"")), "missing demo step {step}");
        assert!(!yml.contains(&format!("werk-demo {step}")), "cicd runs the demo pipeline, not its steps: {step}");
    }
    assert!(yml.contains("start ${CHORUS_WERK_BASE}/${ROLE}-${CARD}/platform/pipelines/demo.yaml -- CARD=${CARD} ROLE=${ROLE}"), "cicd starts the card's demo");
    assert!(!yml.contains("werk-demo card=${CARD}"), "the all-in-one demo call is gone");
    assert!(!yml.contains("werk-demo card=${CARD} go"), "the go is a step, never werk-demo's argument");
    assert!(!yml.contains("${GO}"), "GO read as an argument is how role=false happened");
}
