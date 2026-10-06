use crate::state::AppState;
use crate::types::{permission_deny_json, HookInput, HookResponse};
use std::path::Path;
use tracing::info;


/// #2311 rescope: binary gate. .pending exists AND .done missing → deny
/// all Write/Edit/Bash with zero exemptions. Protocol contract check no
/// longer fires on the Read handler — it runs inline in SessionStart
/// (commands/session.rs) so context is injected via hookSpecificOutput,
/// not via "please read the file" prose. Read is plain-allow.
pub async fn check(input: &HookInput, state: &AppState) -> HookResponse {
    check_with_dir(input, state, &crate::shared::state_paths::session_init_dir()).await
}

/// Internal entry point parameterized on the session-init dir. Production
/// `check()` passes `state_paths::session_init_dir()`. Tests pass a tmpdir to escape the
/// daemon-vs-test race on the global /tmp/claude-session-init path (#2558).
///
/// Migration note (#2524 "always hermetic" tier): cleanest long-term shape
/// is to move the deny/allow tests inline as `#[cfg(test)] mod tests` in
/// src/, at which point `check_with_dir` can drop `pub` and live as a
/// private fn. Today's tests live in tests/ (integration tier) and need
/// the pub function with a dir param. The `#[doc(hidden)]` marker says
/// "testability surface, not stable API" — fine until inline migration.
#[doc(hidden)]
pub async fn check_with_dir(input: &HookInput, state: &AppState, init_dir: &str) -> HookResponse {
    let role = input.role();
    let role_str = role.as_str();

    if role_str == "unknown" {
        return HookResponse::allow();
    }

    let tool = input.tool_name_str();
    let pending = format!("{}/{}.pending", init_dir, role_str);
    let done = format!("{}/{}.done", init_dir, role_str);

    // Read is always allowed. Additionally, Reading the role's own
    // /tmp/session-start-<role>.md when .pending is armed and .done is
    // missing is the in-session recovery path (#2311): boot didn't complete
    // under an older binary, and reading the boot context IS completion.
    // #3288: the stamp-compare that used to gate this write is retired —
    // CLAUDE.md is regenerated from live fragments at SessionStart, so there
    // is no drift for a runtime check to detect.
    if tool == "Read" {
        let file_path = input.get_tool_input_str("file_path");
        let expected = crate::shared::state_paths::session_start_file(role_str, ".md");
        if file_path == expected
            && Path::new(&pending).exists()
            && !Path::new(&done).exists()
        {
            let _ = tokio::fs::create_dir_all(init_dir).await;
            let _ = tokio::fs::write(&done, "").await;
            state.mark_session_init_done(role_str).await;
            info!(
                gate = "session-init",
                role = role_str,
                "In-session recovery: .done written via Read handler."
            );
        }
        return HookResponse::allow();
    }

    // Write/Edit/Bash: binary gate check.
    if tool == "Write" || tool == "Edit" || tool == "Bash" {
        // No pending marker = no session gate active.
        if !Path::new(&pending).exists() {
            return HookResponse::allow();
        }

        // Done marker exists or in-memory flag set — boot completed.
        if Path::new(&done).exists() || state.is_session_init_done(role_str).await {
            return HookResponse::allow();
        }

        // Gate active — deny. No exemptions.
        return HookResponse::deny(&permission_deny_json(&format!(
            "Session init gate: SessionStart boot did not complete for role '{}'. \
             Check {}/{}.done — if missing, the SessionStart \
             hook did not fire (see session.bootstrap.* spine events), or read \
             {} to complete boot in-session. This is a \
             binary gate: no Bash exemptions.",
            role_str, init_dir, role_str, crate::shared::state_paths::session_start_file(role_str, ".md")
        )));
    }

    HookResponse::allow()
}

// #3288: retired_read_handler_protocol_check, write_protocol_violation_banner,
// and log_protocol_violation removed with the stamp-compare layer — CLAUDE.md
// regenerates from live fragments at SessionStart, so there is no runtime
// drift class left to banner. Committed-state coherence is CI's job
// (`claudemd-gen check-version`).

#[cfg(test)]
mod tests {
    use crate::shared::state_paths::chorus_root;
    use super::*;
    use crate::types::HookInput;
    use serde_json::json;

    fn make_input(tool: &str, role_dir: &str) -> HookInput {
        HookInput {
            tool_use_id: None,
            tool_name: Some(tool.to_string()),
            tool_input: Some(json!({"command": "echo test", "file_path": "/tmp/test"})),
            tool_response: None,
            session_id: Some("test".to_string()),
            cwd: Some(format!("{}/{}", chorus_root(), role_dir)),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: None,
            card_type: None,
            trace_id: None, tool_output_is_error: None,}
    }

    #[tokio::test]
    async fn allows_read_always() {
        let state = AppState::new();
        let input = make_input("Read", "architect");
        let r = check(&input, &state).await;
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn allows_bash_when_no_pending_marker() {
        let state = AppState::new();
        let input = make_input("Bash", "architect");
        let r = check(&input, &state).await;
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn allows_unknown_role() {
        let state = AppState::new();
        let input = HookInput {
            tool_use_id: None,
            tool_name: Some("Bash".to_string()),
            tool_input: Some(serde_json::json!({"command": "echo test"})),
            tool_response: None,
            session_id: Some("test".to_string()),
            cwd: Some("/Users/jeffbridwell/some/unknown/path".to_string()),
            prompt: None, stop_hook_active: None, hook_type: None,
            deploy_role: None,
            card_type: None,
            trace_id: None, tool_output_is_error: None,};
        let r = check(&input, &state).await;
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn session_boot_blocked_when_smoke_fails_pending() {
        // #4432 — its own marker dir; this used to arm the LIVE wren.pending.
        let state = AppState::new();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("wren.pending"), "").unwrap();

        let input = HookInput {
            tool_use_id: None,
            tool_name: Some("Edit".to_string()),
            tool_input: Some(json!({"file_path": "/tmp/test.rs", "old_string": "x", "new_string": "y"})),
            tool_response: None,
            session_id: Some("test-boot".to_string()),
            cwd: Some(format!("{}/roles/wren", chorus_root())),
            prompt: None, stop_hook_active: None, hook_type: None,
            deploy_role: Some("wren".to_string()),
            card_type: None,
            trace_id: None, tool_output_is_error: None,};
        let r = check_with_dir(&input, &state, dir.path().to_str().unwrap()).await;
        assert!(r.stdout.is_some(), "Edit should be blocked when session init not complete");
    }
}
