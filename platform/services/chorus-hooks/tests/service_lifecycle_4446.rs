//! #4446 — the shared start / stop / failure helper every chorus Rust daemon uses.
//! Fixtures are real `launchctl print` output, 2026-10-06.

mod sl {
    include!("../../shared/service_lifecycle.rs");
}
use sl::*;

const KILLED: &str = "gui/501/com.chorus.hooks = {
\tstate = running
\truns = 48
\tpid = 38837
\tlast terminating signal = Killed: 9
\tendpoints = {
\t\tlast exit code = 0
\t}
}";
const TERMINATED: &str = "gui/501/com.chorus.athena-make = {\n\truns = 20\n\tlast terminating signal = Terminated: 15\n}";
const EX_CONFIG: &str = "gui/501/com.chorus.fixture-ex-config = {\n\truns = 3325\n\tlast exit code = 78: EX_CONFIG\n}";
const FIRST_RUN: &str = "gui/501/com.chorus.api = {\n\truns = 1\n\tpid = 52021\n}";

fn names(v: &[LifecycleEvent]) -> Vec<&str> {
    v.iter().map(|(e, _)| *e).collect()
}

#[test]
fn reads_the_previous_runs_end_from_launchctl() {
    assert_eq!(parse_last_exit(KILLED), Some(LastExit { exit_code: None, signal: Some("Killed: 9".into()) }));
    assert_eq!(parse_last_exit(EX_CONFIG).unwrap().exit_code, Some(78));
    assert_eq!(parse_last_exit(FIRST_RUN), None, "no previous run on record");
}

#[test]
fn a_start_after_a_kill_reports_the_kill_first() {
    let prev = parse_last_exit(KILLED);
    let ev = start_events("com.chorus.hooks", 4242, "abc123def456", prev.as_ref());
    assert_eq!(names(&ev), ["service.failed", "service.started"]);
    assert!(ev[0].1.contains(&("signal", "Killed: 9".into())));
    assert!(ev[1].1.contains(&("pid", "4242".into())));
    assert!(ev[1].1.contains(&("version", "abc123def456".into())));
}

#[test]
fn a_start_after_an_error_exit_reports_the_exit_code() {
    let ev = start_events("com.chorus.fixture-ex-config", 1, "v", parse_last_exit(EX_CONFIG).as_ref());
    assert!(ev[0].1.contains(&("exit_code", "78".into())));
}

#[test]
fn negative_proof_a_clean_previous_end_is_not_a_failure() {
    // SIGTERM is a deploy or a kickstart: a start, nothing failed.
    assert_eq!(names(&start_events("x", 1, "v", parse_last_exit(TERMINATED).as_ref())), ["service.started"]);
    assert_eq!(names(&start_events("x", 1, "v", None)), ["service.started"]);
    // a first run: launchd prints "(never exited)" — not a failure
    let never = parse_last_exit("x = {\n\tlast exit code = (never exited)\n}");
    assert_eq!(names(&start_events("x", 1, "v", never.as_ref())), ["service.started"]);
}

#[test]
fn stop_and_failure_carry_the_reason() {
    let (e, f) = stop_event("com.chorus.hooks", 7, "SIGTERM");
    assert_eq!(e, "service.stopped");
    assert!(f.contains(&("reason", "SIGTERM".into())));
    let (e, f) = failed_event("com.chorus.hooks", 7, "bind: address in use", 1);
    assert_eq!(e, "service.failed");
    assert!(f.contains(&("exit_code", "1".into())));
}

#[test]
fn negative_proof_an_inherited_label_is_not_launchd_starting_us() {
    // Everything a service starts inherits XPC_SERVICE_NAME; only a process
    // whose parent is launchd is that service. This test's parent is cargo.
    // (The positive half runs under real launchd jobs in 4446-service-lifecycle.bats.)
    std::env::set_var("XPC_SERVICE_NAME", "com.chorus.api");
    assert_eq!(launchd_label(), None);
    std::env::remove_var("XPC_SERVICE_NAME");
}

#[test]
fn a_failure_reason_is_the_runs_last_stderr_line() {
    assert_eq!(failure_reason(4, "starting\nboom: disk full\n\n"), "exited 4: boom: disk full");
}

#[test]
fn negative_proof_a_run_with_no_stderr_says_only_its_exit_code() {
    assert_eq!(failure_reason(4, ""), "exited 4");
    assert_eq!(failure_reason(4, "  \n\n"), "exited 4");
}

#[test]
fn reads_the_stderr_path_from_launchctl() {
    let p = "gui/501/com.chorus.alert-runner = {\n\tstdout path = /a.log\n\tstderr path = /b.log\n}";
    assert_eq!(stderr_path(p).as_deref(), Some("/b.log"));
    assert_eq!(stderr_path(FIRST_RUN), None);
}
