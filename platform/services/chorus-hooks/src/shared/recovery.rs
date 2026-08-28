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
    let rebuild = cmd.contains("build-signed.sh chorus-hooks");
    let agent_state_deploy = cmd.contains("agent-state.sh")
        && cmd.contains("chorus-hooks")
        && cmd.contains("deploy");

    is_hooks_restart_command(cmd) || rebuild || agent_state_deploy
}

/// #4025 — the commands that RESTART the hooks daemon (SIGTERM it and let
/// launchd bring it back): `launchctl kickstart -k / bootout / bootstrap` of
/// com.chorus.hooks, or `agent-state.sh chorus-hooks restart|start`. A deploy
/// (`agent-state.sh chorus-hooks deploy`, `chorus-deploy chorus-hooks`) is NOT
/// in this set — a new binary legitimately restarts the service.
///
/// Two callers, two opposite answers, by design:
///   - the shim's connect-failure carve-out (daemon DOWN) lets these through —
///     that is the recovery path (#2790/#3631);
///   - the live daemon's infra_guardrails DENIES them — a daemon that is
///     answering the request is provably alive, and every exit -15 death on
///     2026-08-27/28 (13:53, 21:03, 06:50, 09:40) was a role running exactly
///     this after a 15s timeout under load. Busy is not dead.
pub fn is_hooks_restart_command(cmd: &str) -> bool {
    let launchctl_restart = (cmd.contains("kickstart")
        || cmd.contains("bootstrap")
        || cmd.contains("bootout"))
        && cmd.contains("com.chorus.hooks");
    let agent_state_restart = cmd.contains("agent-state.sh")
        && cmd.contains("chorus-hooks")
        && (cmd.contains("restart") || cmd.contains(" start"))
        && !cmd.contains("deploy");
    launchctl_restart || agent_state_restart
}
