// @domain: tests
//! #4885 — cicd run 3 refused the card: "proving/fitness/run-fitness.sh is claimed by no unit".
//! The fitness job's two scripts are launchd job bodies, like alert-runner.sh: named exactly as
//! scope-irrelevant. Any other proving/ path still escapes loudly (#4000's rule).
use werk_test::scope_irrelevant;

#[test]
fn the_fitness_job_scripts_are_job_bodies_not_build_inputs() {
    assert!(scope_irrelevant("proving/fitness/run-fitness.sh"));
    assert!(scope_irrelevant("proving/fitness/write-snapshot.sh"));
    assert!(scope_irrelevant("proving/scripts/alert-runner.sh"));
}

#[test]
fn negative_proof_another_proving_path_is_still_claimed_by_no_unit() {
    assert!(!scope_irrelevant("proving/fitness/new-check.sh"));
    assert!(!scope_irrelevant("proving/fitness/run-fitness.sh.bak"));
    assert!(!scope_irrelevant("proving/other/run-fitness.sh"));
}
