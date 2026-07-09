//! #2790/#3631 — when the hook daemon is DOWN, the pre-tool-use shim fails
//! closed (denies every tool call). That includes the command that RESTARTS the
//! daemon → the team is locked out of its own recovery (2026-07-08, 14 hours).
//! The carve-out: while the daemon is unreachable, recognize the documented
//! recovery commands and let them through. Everything else still fails closed.
//!
//! is_recovery_command must be TIGHT — only the actual recovery shapes, never a
//! blanket "contains chorus-hooks".

use chorus_hooks::shared::recovery::is_recovery_command;

fn bash(cmd: &str) -> String {
    format!(r#"{{"tool_name":"Bash","tool_input":{{"command":"{cmd}"}}}}"#)
}

#[test]
fn allows_launchctl_kickstart_of_the_hooks_daemon() {
    assert!(is_recovery_command(&bash(
        "launchctl kickstart -k gui/501/com.chorus.hooks"
    )));
}

#[test]
fn allows_bootstrap_recovery() {
    assert!(is_recovery_command(&bash(
        "launchctl bootstrap gui/501 ~/Library/LaunchAgents/com.chorus.hooks.plist"
    )));
}

#[test]
fn allows_build_signed_rebuild_of_hooks() {
    assert!(is_recovery_command(&bash(
        "~/CascadeProjects/chorus/platform/scripts/build-signed.sh chorus-hooks"
    )));
}

#[test]
fn allows_agent_state_restart_of_hooks() {
    assert!(is_recovery_command(&bash("agent-state.sh restart chorus-hooks")));
}

#[test]
fn denies_unrelated_bash() {
    assert!(!is_recovery_command(&bash("rm -rf /some/path")));
    assert!(!is_recovery_command(&bash("cargo build --release")));
}

#[test]
fn denies_a_mere_mention_of_chorus_hooks_without_a_recovery_verb() {
    // Tight: reading a log or grepping the source is NOT a recovery command and
    // must still fail closed while the daemon is down.
    assert!(!is_recovery_command(&bash("cat ~/Library/Logs/Chorus/chorus-hooks.log")));
    assert!(!is_recovery_command(&bash("grep SOCKET chorus-hooks/src/main.rs")));
}

#[test]
fn denies_non_bash_tools() {
    let write = r#"{"tool_name":"Write","tool_input":{"file_path":"/etc/x","command":"launchctl kickstart com.chorus.hooks"}}"#;
    assert!(!is_recovery_command(write));
}
