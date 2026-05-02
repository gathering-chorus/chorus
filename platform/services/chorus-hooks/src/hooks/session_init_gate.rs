use crate::shared::protocol_contract;
use crate::shared::state_paths::chorus_root;
use crate::state::AppState;
use crate::types::{permission_deny_json, HookInput, HookResponse};
use std::path::Path;
use tracing::{info, error};

const INIT_DIR: &str = "/tmp/claude-session-init";

/// #2311 rescope: binary gate. .pending exists AND .done missing → deny
/// all Write/Edit/Bash with zero exemptions. Protocol contract check no
/// longer fires on the Read handler — it runs inline in SessionStart
/// (commands/session.rs) so context is injected via hookSpecificOutput,
/// not via "please read the file" prose. Read is plain-allow.
pub async fn check(input: &HookInput, state: &AppState) -> HookResponse {
    check_with_dir(input, state, INIT_DIR).await
}

/// Internal entry point parameterized on the session-init dir. Production
/// `check()` always passes `INIT_DIR`. Tests pass a tmpdir to escape the
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
    // missing is the in-session recovery path (#2311): re-runs the same
    // protocol_contract::check that SessionStart runs, writing .done on
    // pass. Same one entry point as SessionStart — just reachable from
    // Read for roles whose boot did not complete under an older binary.
    if tool == "Read" {
        let file_path = input.get_tool_input_str("file_path");
        let expected = format!("/tmp/session-start-{}.md", role_str);
        if file_path == expected
            && Path::new(&pending).exists()
            && !Path::new(&done).exists()
        {
            match protocol_contract::check(role_str) {
                Ok(()) => {
                    let _ = tokio::fs::create_dir_all(init_dir).await;
                    let _ = tokio::fs::write(&done, "").await;
                    state.mark_session_init_done(role_str).await;
                    info!(
                        gate = "session-init",
                        role = role_str,
                        "In-session recovery: protocol pass → .done written via Read handler."
                    );
                }
                Err(v) => {
                    write_protocol_violation_banner(role_str, &v).await;
                    log_protocol_violation(role_str, &v);
                }
            }
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
             Check {}/{}.done — if missing, SessionStart \
             hook did not fire or protocol_contract check failed (see \
             session.protocol.violation spine events). This is a binary gate: \
             no Bash exemptions.",
            role_str, init_dir, role_str
        )));
    }

    HookResponse::allow()
}

/// #2311 rescope: formerly ran on Read of session-start.md. Now unused by
/// the PreToolUse handler — SessionStart owns the protocol-check fire
/// point. Kept as a library function in case future maintenance wants to
/// re-enable a Read-triggered re-check; not reachable from `check()`.
#[allow(dead_code)]
async fn retired_read_handler_protocol_check(role_str: &str, state: &AppState) {
    let pending = format!("{}/{}.pending", INIT_DIR, role_str);
    let done = format!("{}/{}.done", INIT_DIR, role_str);
    if !Path::new(&pending).exists() { return; }
    let smoke_ok = run_gate_smoke(role_str, state);
    let protocol_ok = match protocol_contract::check(role_str) {
        Ok(()) => true,
        Err(v) => {
            write_protocol_violation_banner(role_str, &v).await;
            log_protocol_violation(role_str, &v);
            false
        }
    };
    if smoke_ok && protocol_ok {
        let _ = tokio::fs::create_dir_all(INIT_DIR).await;
        let _ = tokio::fs::write(&done, "").await;
        state.mark_session_init_done(role_str).await;
    } else {
        error!(
            gate = "session-init",
            role = role_str,
            smoke_ok,
            protocol_ok,
            "Session boot blocked: gate smoke or protocol contract check failed."
        );
    }
}

/// #2311: Write the PROTOCOL VIOLATION / STALE banner into the role's session-start
/// file so the model sees the failure the next time it reads the context. We prepend
/// to a companion file `/tmp/session-start-<role>-PROTOCOL_VIOLATION.md` AND prepend
/// into the primary session-start file if present.
async fn write_protocol_violation_banner(role: &str, v: &protocol_contract::Violation) {
    let banner = protocol_contract::banner(role, v);
    let companion = format!("/tmp/session-start-{}-PROTOCOL_VIOLATION.md", role);
    let primary = format!("/tmp/session-start-{}.md", role);
    let _ = tokio::fs::write(&companion, &banner).await;
    if let Ok(existing) = tokio::fs::read_to_string(&primary).await {
        // Only prepend if banner isn't already there — idempotent per session.
        if !existing.starts_with("## 🛑 PROTOCOL VIOLATION") && !existing.starts_with("## ⚠ STALE CLAUDE.md") {
            let merged = format!("{}\n{}", banner, existing);
            let _ = tokio::fs::write(&primary, merged).await;
        }
    }
}

/// #2311: Emit a spine event for the protocol violation so fleet-wide drift is observable.
fn log_protocol_violation(role: &str, v: &protocol_contract::Violation) {
    let fields = protocol_contract::event_fields(role, v);
    let event = match v.reason() {
        "stale" => "session.protocol.stale",
        _ => "session.protocol.violation",
    };
    let mut cmd = std::process::Command::new(
        format!("{}/platform/scripts/chorus-log", chorus_root())
    );
    cmd.arg(event).arg(role);
    for (k, val) in fields {
        if k == "role" { continue; } // already positional
        cmd.arg(format!("{}={}", k, val));
    }
    let _ = cmd.output();
    error!(
        gate = "protocol-contract",
        role = role,
        reason = v.reason(),
        "Session boot blocked: protocol contract failed."
    );
}

/// Gate smoke check (#1929): on session boot, verify critical gates block when they should.
/// Sets temporary fix-card state, sends a synthetic Edit, confirms gates deny.
/// Returns true if all gates pass smoke, false if any gate is broken.
/// If false, the session init gate will NOT mark done — blocking all work.
fn run_gate_smoke(role: &str, state: &AppState) -> bool {
    use crate::hooks::{log_first_gate, memory_gate};

    // Force is_fix_card() to true regardless of live board state — the smoke
    // is verifying gate-blocking logic, not querying chorus-api. Without this,
    // smoke is non-deterministic: passes only when some role has a type:fix
    // WIP card on the board (#2644 AC2).
    let prior_override = std::env::var("CHORUS_TEST_FORCE_FIX_CARD").ok();
    // SAFETY: smoke runs at session boot before workers spawn; tests are serial.
    unsafe { std::env::set_var("CHORUS_TEST_FORCE_FIX_CARD", "1"); }

    // Save current state file
    let state_path = format!("/tmp/claude-team-scan/{}-declared.json", role);
    let backup = std::fs::read_to_string(&state_path).ok();

    // Write temporary fix-card state — gates should fire for fix cards
    let smoke_state = format!(
        r#"{{"role":"{}","state":"building","card":99999,"card_type":"fix","ts":{}}}"#,
        role,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    );
    let _ = std::fs::create_dir_all("/tmp/claude-team-scan");
    let _ = std::fs::write(&state_path, &smoke_state);

    // Synthetic Edit on a cross-domain code file — both gates should DENY:
    // - log_first_gate: no log evidence in session
    // - memory_gate: no search/synthesis in session
    let smoke_cwd = format!("{}/platform/roles/{}",
        chorus_root(),
        match role { "wren" => "wren", "silas" => "silas", _ => "kade" });
    let smoke_session_id = format!("smoke-{}", role);

    // Seed a minimal JSONL file so gates have session data to scan (and find no evidence).
    // Without this, gates fall open on empty session data and smoke can't verify blocking.
    let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/jeffbridwell".to_string());
    let project_key = smoke_cwd.replace('/', "-");
    #[allow(clippy::manual_strip)]
    let project_key = if project_key.starts_with('-') { &project_key[1..] } else { &project_key };
    let jsonl_dir = format!("{}/.claude/projects/-{}", home, project_key);
    let jsonl_path = format!("{}/{}.jsonl", jsonl_dir, smoke_session_id);
    let _ = std::fs::create_dir_all(&jsonl_dir);
    // Neutral content — no log markers, no synthesis markers
    let _ = std::fs::write(&jsonl_path, r#"{"type":"assistant","content":"Starting smoke check session."}"#);

    let smoke_input = HookInput {
        tool_name: Some("Edit".to_string()),
        tool_input: Some(serde_json::json!({
            "file_path": format!("{}/platform/services/smoke-test.rs", chorus_root()),
            "old_string": "x",
            "new_string": "y"
        })),
        tool_response: None,
        session_id: Some(smoke_session_id.clone()),
        cwd: Some(smoke_cwd),
        prompt: None,
        stop_hook_active: None,
        hook_type: None,
        deploy_role: Some(role.to_string()),
        chorus_worktree_override: None,
    };

    let mut all_pass = true;

    // Smoke #1: log_first_gate — should deny (no log evidence)
    let log_result = log_first_gate::check(&smoke_input, state);
    if log_result.stdout.is_none() {
        error!(
            gate = "smoke-check",
            target = "log_first_gate",
            role = role,
            "SMOKE FAILED: log_first_gate allowed a fix-card edit without log inspection."
        );
        eprintln!("⚠ GATE SMOKE FAILED: log_first_gate did not block. Gates may be silently broken.");
        all_pass = false;
    } else {
        info!(gate = "smoke-check", target = "log_first_gate", role = role, "SMOKE PASS");
    }

    // Smoke #2: memory_gate (context synthesis) — should deny (no search/synthesis)
    let mem_result = memory_gate::check(&smoke_input, state);
    if mem_result.stdout.is_none() {
        error!(
            gate = "smoke-check",
            target = "memory_gate",
            role = role,
            "SMOKE FAILED: memory_gate allowed a fix-card edit without context synthesis."
        );
        eprintln!("⚠ GATE SMOKE FAILED: memory_gate did not block. Gates may be silently broken.");
        all_pass = false;
    } else {
        info!(gate = "smoke-check", target = "memory_gate", role = role, "SMOKE PASS");
    }

    // Restore original state and clean up smoke artifacts
    match backup {
        Some(original) => { let _ = std::fs::write(&state_path, original); }
        None => { let _ = std::fs::remove_file(&state_path); }
    }
    let _ = std::fs::remove_file(&jsonl_path);
    // SAFETY: see set_var above — single-threaded smoke path.
    unsafe {
        match prior_override {
            Some(v) => std::env::set_var("CHORUS_TEST_FORCE_FIX_CARD", v),
            None => std::env::remove_var("CHORUS_TEST_FORCE_FIX_CARD"),
        }
    }

    if all_pass {
        info!(gate = "smoke-check", role = role, "All gate smoke checks passed");
    }

    all_pass
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HookInput;
    use serde_json::json;

    fn make_input(tool: &str, role_dir: &str) -> HookInput {
        HookInput {
            tool_name: Some(tool.to_string()),
            tool_input: Some(json!({"command": "echo test", "file_path": "/tmp/test"})),
            tool_response: None,
            session_id: Some("test".to_string()),
            cwd: Some(format!("{}/{}", chorus_root(), role_dir)),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: None,
            chorus_worktree_override: None,}
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
            tool_name: Some("Bash".to_string()),
            tool_input: Some(serde_json::json!({"command": "echo test"})),
            tool_response: None,
            session_id: Some("test".to_string()),
            cwd: Some("/Users/jeffbridwell/some/unknown/path".to_string()),
            prompt: None, stop_hook_active: None, hook_type: None,
            deploy_role: None,
            chorus_worktree_override: None,};
        let r = check(&input, &state).await;
        assert_eq!(r.exit_code, 0);
    }

    #[test]
    fn smoke_check_passes_when_gates_block() {
        // With fix-card state and no log/synthesis evidence,
        // both gates should deny → smoke returns true.
        // Must use a real role name — is_fix_card() only checks kade/silas/wren.
        let state = AppState::new();
        let state_path = "/tmp/claude-team-scan/wren-declared.json";
        let backup = std::fs::read_to_string(state_path).ok();

        let result = run_gate_smoke("wren", &state);
        assert!(result, "smoke should pass when gates correctly block");

        // Restore any pre-existing state
        match backup {
            Some(original) => { let _ = std::fs::write(state_path, original); }
            None => { let _ = std::fs::remove_file(state_path); }
        }
    }

    #[test]
    fn smoke_check_restores_original_state() {
        let state = AppState::new();
        let state_path = "/tmp/claude-team-scan/kade-declared.json";
        let _ = std::fs::create_dir_all("/tmp/claude-team-scan");
        let pre_existing = std::fs::read_to_string(state_path).ok();

        let original = r#"{"role":"kade","state":"building","card":42,"card_type":"new","ts":1234}"#;
        std::fs::write(state_path, original).unwrap();

        let _result = run_gate_smoke("kade", &state);

        let restored = std::fs::read_to_string(state_path).unwrap();
        assert_eq!(restored, original, "original state should be restored after smoke");

        match pre_existing {
            Some(orig) => { let _ = std::fs::write(state_path, orig); }
            None => { let _ = std::fs::remove_file(state_path); }
        }
    }

    #[tokio::test]
    async fn session_boot_blocked_when_smoke_fails_pending() {
        let state = AppState::new();
        let _ = std::fs::create_dir_all(INIT_DIR);
        let pending = format!("{}/wren.pending", INIT_DIR);
        let done = format!("{}/wren.done", INIT_DIR);
        let had_pending = Path::new(&pending).exists();
        let had_done = Path::new(&done).exists();

        std::fs::write(&pending, "").unwrap();
        let _ = std::fs::remove_file(&done);

        let input = HookInput {
            tool_name: Some("Edit".to_string()),
            tool_input: Some(json!({"file_path": "/tmp/test.rs", "old_string": "x", "new_string": "y"})),
            tool_response: None,
            session_id: Some("test-boot".to_string()),
            cwd: Some(format!("{}/roles/wren", chorus_root())),
            prompt: None, stop_hook_active: None, hook_type: None,
            deploy_role: Some("wren".to_string()),
            chorus_worktree_override: None,};
        let r = check(&input, &state).await;
        assert!(r.stdout.is_some(), "Edit should be blocked when session init not complete");

        if !had_pending { let _ = std::fs::remove_file(&pending); }
        if had_done { let _ = std::fs::write(&done, ""); }
    }
}
