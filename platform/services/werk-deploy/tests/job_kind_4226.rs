// #4226 — a scheduled job's success is a clean exit, not a live pid.
// Fixtures are real `launchctl print` shapes so the two readings can be told
// apart without a machine in a particular state.
use werk_deploy::{deploy_took, job_kind, JobKind};

const DAEMON_UP: &str = "\
gui/501/com.chorus.api = {
	state = running
	pid = 64992
	RunAtLoad = 1
}";

const DAEMON_DOWN: &str = "\
gui/501/com.chorus.api = {
	state = spawn scheduled
	last exit code = 1
	RunAtLoad = 1
}";

const SCHEDULED_FINISHED: &str = "\
gui/501/com.chorus.athena-validate = {
	state = waiting
	program = /Users/jeffbridwell/.chorus/bin/athena-validate
	last exit code = 0
	StartCalendarInterval = {
		Hour = 7
	}
}";

const SCHEDULED_FAILED: &str = "\
gui/501/com.chorus.athena-validate = {
	state = waiting
	program = /Users/jeffbridwell/.chorus/bin/athena-validate
	last exit code = 1
	StartCalendarInterval = {
		Hour = 7
	}
}";

#[test]
fn a_calendar_job_is_not_a_daemon() {
    assert_eq!(job_kind(SCHEDULED_FINISHED), JobKind::Scheduled);
    assert_eq!(job_kind(DAEMON_UP), JobKind::Daemon);
}

#[test]
fn a_finished_scheduled_job_is_a_successful_deploy() {
    // The exact state that rolled back Wren's #4167 at 08:08.
    assert!(deploy_took(SCHEDULED_FINISHED));
}

#[test]
fn a_daemon_still_has_to_be_running() {
    assert!(deploy_took(DAEMON_UP));
    assert!(!deploy_took(DAEMON_DOWN));
}

#[test]
fn negative_proof_a_scheduled_job_that_exited_nonzero_still_fails() {
    // Without this the change would not relax a check, it would delete one:
    // every scheduled deploy would pass whatever happened.
    assert!(!deploy_took(SCHEDULED_FAILED));
    assert_ne!(deploy_took(SCHEDULED_FINISHED), deploy_took(SCHEDULED_FAILED));
}

#[test]
fn a_scheduled_job_that_has_never_run_is_not_a_failure() {
    let never_run = SCHEDULED_FINISHED.replace("\tlast exit code = 0\n", "");
    assert!(deploy_took(&never_run), "scheduled-but-not-yet-run is the normal state after a load");
}
