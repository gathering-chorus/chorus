//! #4446 — athena-validate's first line is `service_lifecycle::run_as_job()`.
//! Under launchd it re-runs the sweep as a child and logs a failed run as
//! service.failed. Run by hand or from a suite it must step aside: return, so
//! the sweep runs in this process with its own exit code, and write nothing.

mod service_lifecycle {
    #![allow(dead_code)]
    include!("../../shared/service_lifecycle.rs");
}

#[test]
fn run_by_hand_the_job_wrapper_steps_aside_4446() {
    // An inherited launchd label is not launchd starting us: this test's parent
    // is cargo, so the wrapper must return instead of re-running the binary.
    std::env::set_var("XPC_SERVICE_NAME", "com.chorus.athena-validate");
    assert_eq!(service_lifecycle::launchd_label(), None);
    service_lifecycle::run_as_job(); // returns: a re-run would exit this process
    std::env::remove_var("XPC_SERVICE_NAME");
}
