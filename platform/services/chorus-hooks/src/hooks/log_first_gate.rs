//! Log-first gate (#1879)
//! PreToolUse on Edit/Write: blocks code fixes without prior log inspection.
//!
//! When a role is fixing a bug, the logs almost always reveal the root cause
//! faster than reading code. Today: Wren read 550 lines of observer.rs to
//! understand stale cards, then checked logs and found the actual cause was
//! different. This gate enforces: check logs before coding a fix.
//!
//! Only fires in defect-fix contexts (reuses memory_gate's is_defect_fix).
//! New feature work is exempt.

use crate::state::AppState;
use crate::types::{permission_deny_json, HookInput, HookResponse};
use tracing::info;

/// Log file patterns that count as "checked the logs"
const LOG_INSPECTION_MARKERS: &[&str] = &[
    "chorus.log",
    "hooks.log",
    "command-errors.log",
    "permission-prompts.log",
    "chorus-log",
    "loki",
    "localhost:3102",
    "/var/log/chorus",
    "journalctl",
    "spine-events",
];

/// File extensions that count as code (same as memory_gate)
fn is_code_file(path: &str) -> bool {
    crate::shared::file_classification::is_source_code(path)
}

/// Check if the card context is a fix — reads type: label from board (#1909)
fn is_defect_fix() -> bool {
    crate::types::is_fix_card()
}

/// Build/lint error markers — when the session contains these, the fix is for
/// a build error (ESLint, tsc, cargo), not a runtime bug. Logs won't help. (#2042)
const BUILD_ERROR_MARKERS: &[&str] = &[
    "eslint",
    "no-undef",
    "tsc",
    "typescript",
    "ts(",           // tsc error format: TS(2304)
    "cargo build",
    "cargo test",
    "pre-commit",
    "lint error",
    "lint failed",
    "compile error",
    "syntax error",
];

/// Check if the session contains evidence of a build/lint error being fixed.
/// If so, the log-first gate is irrelevant — skip it.
fn is_build_error_context(input: &HookInput, state: &AppState) -> bool {
    let session_id = match &input.session_id {
        Some(id) => id.clone(),
        None => return false,
    };

    let cwd = input.cwd.as_deref().unwrap_or("");
    let lines = state.session_cache.get_tail(&session_id, cwd, 100);

    for line in &lines {
        let lower = line.to_lowercase();
        for marker in BUILD_ERROR_MARKERS {
            if lower.contains(marker) {
                return true;
            }
        }
    }

    false
}

/// Scan session JSONL for evidence of log inspection AND synthesis.
/// Two checks (both required for fix cards):
/// 1. Did the role read a log file? (motion check)
/// 2. Did the role produce a "Log evidence:" statement connecting findings to the domain?
///    (synthesis check — prevents performative log opens)
fn has_log_evidence(input: &HookInput, state: &AppState) -> bool {
    let session_id = match &input.session_id {
        Some(id) => id.clone(),
        None => return true, // No session = can't check, allow
    };

    let cwd = input.cwd.as_deref().unwrap_or("");
    let lines = state.session_cache.get_tail(&session_id, cwd, 300);
    if lines.is_empty() {
        // Fall closed: if we know it's a fix card but have no session data,
        // require log inspection. Original #1879 fell open here but that
        // meant the gate never actually blocked (#1926 finding).
        return false;
    }

    // If context_inject already ran and found log-related hits, count as log_read (#2225)
    let mut has_log_read = if state.has_context_results_sync(&session_id) {
        // Context inject ran — Chorus may have surfaced log-related context
        // Still require synthesis, but credit the search
        true
    } else {
        false
    };
    let mut has_log_synthesis = false;

    for line in &lines {
        let lower = line.to_lowercase();

        // Check 1: log file reads or greps
        if !has_log_read {
            for marker in LOG_INSPECTION_MARKERS {
                if lower.contains(marker) {
                    has_log_read = true;
                    break;
                }
            }
            if !has_log_read
                && (lower.contains("tail") || lower.contains("grep") || lower.contains("rg "))
                && lower.contains(".log")
            {
                has_log_read = true;
            }
        }

        // Check 2: "Log evidence:" synthesis in assistant output
        // Must contain what the logs revealed, not just that logs were opened.
        if !has_log_synthesis && lower.contains("assistant")
            && (lower.contains("log evidence:") || lower.contains("logs show")
                || lower.contains("log shows") || lower.contains("from the logs:")
                || lower.contains("log output:") || lower.contains("the log reveals"))
            {
                has_log_synthesis = true;
            }

        if has_log_read && has_log_synthesis {
            break;
        }
    }

    has_log_read && has_log_synthesis
}

pub fn check(input: &HookInput, state: &AppState) -> HookResponse {
    let tool = input.tool_name_str();
    if tool != "Edit" && tool != "Write" {
        return HookResponse::allow();
    }

    let file_path = input.get_tool_input_str("file_path");

    // Only gate code files
    if !is_code_file(&file_path) {
        return HookResponse::allow();
    }

    // Skip generated/build paths
    if file_path.contains("/target/") || file_path.contains("/node_modules/") {
        return HookResponse::allow();
    }

    // Only enforce on defect fixes — new features are exempt
    if !is_defect_fix() {
        return HookResponse::allow();
    }

    // Skip for lint/build fixes — logs don't help with ESLint or tsc errors (#2042)
    if is_build_error_context(input, state) {
        info!(
            gate = "log-first",
            decision = "skip",
            reason = "build/lint error context — logs not relevant",
            role = %format!("{:?}", input.role()).to_lowercase(),
            file = %file_path,
        );
        return HookResponse::allow();
    }

    let role_name = format!("{:?}", input.role()).to_lowercase();

    if !has_log_evidence(input, state) {
        info!(
            gate = "log-first",
            decision = "deny",
            reason = "fix context without log inspection",
            role = %role_name,
            file = %file_path,
        );
        return HookResponse::deny(&permission_deny_json(
            "Log-first gate: you're fixing a bug but haven't checked the logs — or haven't said what you found. \
             1) Read the relevant logs (chorus.log, hooks.log, or Loki). \
             2) State what you found: 'Log evidence: <what the logs revealed about this problem>'. \
             Not just that you opened a log — what it told you. \
             Logs reveal root cause in seconds; reading code without them leads to wrong theories."
        ));
    }

    info!(
        gate = "log-first",
        decision = "allow",
        reason = "log inspection found",
        role = %role_name,
        file = %file_path,
    );
    HookResponse::allow()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HookInput;
    use crate::shared::state_paths::chorus_root;

    fn make_input(tool: &str, file_path: &str) -> HookInput {
        HookInput {
            tool_name: Some(tool.to_string()),
            tool_input: Some(serde_json::json!({
                "file_path": file_path
            })),
            tool_response: None,
            session_id: None,
            cwd: Some(format!("{}/roles/kade", chorus_root())),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("kade".to_string()),
            chorus_worktree_override: None,}
    }

    fn state() -> AppState { AppState::new() }

    #[test]
    fn allows_non_edit_tools() {
        let r = check(&make_input("Read", "/some/file.ts"), &state());
        assert!(r.exit_code == 0);
    }

    #[test]
    fn allows_non_code_files() {
        let r = check(&make_input("Edit", "/some/path/README.md"), &state());
        assert!(r.exit_code == 0);
    }

    #[test]
    fn allows_target_dir() {
        let r = check(&make_input("Edit", "/project/target/debug/build.rs"), &state());
        assert!(r.exit_code == 0);
    }

    #[test]
    fn allows_without_session() {
        // No session ID = can't verify, allow
        let r = check(&make_input("Edit", "/some/file.rs"), &state());
        assert!(r.exit_code == 0);
    }

    #[test]
    fn code_file_detection() {
        assert!(is_code_file("src/main.rs"));
        assert!(is_code_file("app.ts"));
        assert!(is_code_file("script.sh"));
        assert!(!is_code_file("README.md"));
        assert!(!is_code_file("config.toml"));
        assert!(!is_code_file("style.css"));
    }

    // Build error context detection (#2042)
    #[test]
    fn build_error_markers_present() {
        assert!(BUILD_ERROR_MARKERS.contains(&"eslint"));
        assert!(BUILD_ERROR_MARKERS.contains(&"pre-commit"));
        assert!(BUILD_ERROR_MARKERS.contains(&"lint error"));
    }

    #[test]
    fn log_markers_present() {
        // Verify the markers we check are reasonable
        assert!(LOG_INSPECTION_MARKERS.contains(&"chorus.log"));
        assert!(LOG_INSPECTION_MARKERS.contains(&"hooks.log"));
        assert!(LOG_INSPECTION_MARKERS.contains(&"loki"));
    }

    #[test]
    fn non_fix_context_bypasses_gate() {
        // When not in a fix context, gate doesn't fire even without logs
        // (is_defect_fix reads /tmp/role-state-*.txt — in tests these won't exist,
        //  so is_defect_fix() returns false and the gate is bypassed)
        let r = check(&make_input("Edit", "/some/cross-domain/file.rs"), &state());
        assert!(r.exit_code == 0);
    }
}
