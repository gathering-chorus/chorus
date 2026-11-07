// @domain: deploys — placed by chorus-crawl's domain rules (#4538)
//! #4895 AC6 — a land that changes a TS daemon must restart it. On 2026-11-05 #4889's
//! land reported pulse deploy.completed while pid 91234 kept running the old code until
//! 09:02 the next day: the smoke answered healthy from the old process. The verdict reads
//! the pid launchd reports before and after the kickstart.
use werk_deploy::{pid_from_print, restart_verdict};

const PRINT: &str = "gui/501/com.chorus.pulse = {\n\tactive count = 1\n\tpath = /Users/x/Library/LaunchAgents/com.chorus.pulse.plist\n\tstate = running\n\tpid = 66894\n\tlast exit code = 0\n}";

#[test]
fn the_pid_is_read_from_launchctl_print() {
    assert_eq!(pid_from_print(PRINT), Some(66894));
    assert_eq!(pid_from_print("gui/501/com.chorus.pulse = {\n\tstate = not running\n}"), None);
}

#[test]
fn a_new_pid_after_the_kickstart_is_restarted() {
    assert!(restart_verdict(Some(91234), Some(66894), None).is_ok());
    // not running before, running after: started
    assert!(restart_verdict(None, Some(66894), None).is_ok());
}

// NEGATIVE PROOF (#3734) — the 11-05 state: the kickstart left pid 91234 running.
#[test]
fn negative_proof_the_same_pid_after_the_kickstart_is_not_restarted() {
    let e = restart_verdict(Some(91234), Some(91234), None).unwrap_err();
    assert!(e.contains("not-restarted") && e.contains("91234"), "{e}");
    let e = restart_verdict(Some(91234), Some(91234), Some("Could not find service")).unwrap_err();
    assert!(e.contains("Could not find service"), "the kickstart's own error is named: {e}");
}

#[test]
fn negative_proof_no_process_after_the_kickstart_is_not_restarted() {
    assert!(restart_verdict(Some(91234), None, None).unwrap_err().contains("no process after"));
    assert!(restart_verdict(None, None, None).is_err());
}
