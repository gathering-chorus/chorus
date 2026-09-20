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

// The real print output for com.chorus.athena-validate at 08:33 on 2026-09-20,
// seconds after the kickstart a deploy issues. The job sweeps the whole graph
// and takes minutes; launchctl has no exit code to report yet. The first cut of
// this fix read that as a failure, so the 15s up-check would have rolled the
// deploy back anyway — the same bug entered from the other side.
const SCHEDULED_IN_FLIGHT: &str = "gui/501/com.chorus.athena-validate = {\n\tpath = /Users/x/Library/LaunchAgents/com.chorus.athena-validate.plist\n\tstate = running\n\tprogram = /Users/x/.chorus/bin/athena-validate\n\truns = 1\n\tlast exit code = (never exited)\n\tevent triggers = {\n\t\tstream = com.apple.launchd.calendarinterval\n\t}\n}\n";

#[test]
fn a_scheduled_job_still_running_is_a_successful_deploy() {
    assert_eq!(job_kind(SCHEDULED_IN_FLIGHT), JobKind::Scheduled);
    assert!(
        deploy_took(SCHEDULED_IN_FLIGHT),
        "a long sweep mid-run is the deploy having taken, not a dead job"
    );
}

#[test]
fn a_running_job_is_judged_on_this_run_not_the_last_one() {
    // The case that actually blocks Wren: athena-validate had exited 127 seven
    // times on the stale plist. The deploy fixes the plist and kickstarts it;
    // while the new run is in flight launchctl still reports the PREVIOUS
    // run's code. Judging the deploy on it rolls back the run that fixed it.
    let running_after_a_bad_run =
        SCHEDULED_IN_FLIGHT.replace("(never exited)", "127");
    assert!(deploy_took(&running_after_a_bad_run));
    // And the same output with the job stopped is the failure it looks like.
    let stopped_after_a_bad_run =
        running_after_a_bad_run.replace("\tstate = running\n", "\tstate = not running\n");
    assert!(!deploy_took(&stopped_after_a_bad_run));
}

#[test]
fn negative_proof_an_unrecognised_exit_value_is_not_waved_through() {
    // The permissive first cut passed anything that did not parse as a number.
    // A check that accepts what it cannot read cannot separate the two states
    // it exists to separate, so the unknown value must fail while not running.
    let garbage = SCHEDULED_IN_FLIGHT
        .replace("\tstate = running\n", "\tstate = not running\n")
        .replace("(never exited)", "who knows");
    assert!(!deploy_took(&garbage));
}
