//! #2790/#3631 — recovery-command recognition for the daemon-down carve-out.
//!
//! When the hook daemon is unreachable, the pre-tool-use shim fails closed and
//! denies every tool call — which included the command that restarts the daemon,
//! so the team was locked out of its own recovery for 14h on 2026-07-08. This
//! recognizes the DOCUMENTED recovery commands so they alone pass the down-gate;
//! every other call still fails closed. Deliberately tight: a recovery VERB must
//! be paired with the chorus-hooks target — a bare mention (reading the log,
//! grepping the source) does not qualify.

/// True iff `raw_input` (the hook JSON stdin) is a Bash call running one of the
/// documented chorus-hooks recovery commands.
pub fn is_recovery_command(raw_input: &str) -> bool {
    let v: serde_json::Value = match serde_json::from_str(raw_input) {
        Ok(v) => v,
        Err(_) => return false,
    };
    // Only a Bash tool call can be a recovery command.
    if v.get("tool_name").and_then(|t| t.as_str()) != Some("Bash") {
        return false;
    }
    let cmd = match v
        .get("tool_input")
        .and_then(|i| i.get("command"))
        .and_then(|c| c.as_str())
    {
        Some(c) => c,
        None => return false,
    };

    // A recovery VERB paired with the chorus-hooks target. Not a bare mention.
    let launchctl_restart = (cmd.contains("kickstart")
        || cmd.contains("bootstrap")
        || cmd.contains("bootout"))
        && cmd.contains("com.chorus.hooks");
    let rebuild = cmd.contains("build-signed.sh chorus-hooks");
    let agent_state = cmd.contains("agent-state.sh")
        && cmd.contains("chorus-hooks")
        && (cmd.contains("restart") || cmd.contains("start") || cmd.contains("deploy"));

    launchctl_restart || rebuild || agent_state
}
