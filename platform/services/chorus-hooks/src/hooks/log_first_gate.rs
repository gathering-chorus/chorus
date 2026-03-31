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
    let code_exts = [".rs", ".ts", ".tsx", ".js", ".jsx", ".py", ".sh"];
    code_exts.iter().any(|ext| path.ends_with(ext))
}

/// Check if the card context is a fix — reads type: label from board (#1909)
fn is_defect_fix() -> bool {
    crate::types::is_fix_card()
}

/// Scan session JSONL for evidence of log inspection (uses shared cache #1861)
fn has_log_evidence(input: &HookInput, state: &AppState) -> bool {
    let session_id = match &input.session_id {
        Some(id) => id.clone(),
        None => return true, // No session = can't check, allow
    };

    let cwd = input.cwd.as_deref().unwrap_or("");
    let lines = state.session_cache.get_tail(&session_id, cwd, 300);
    if lines.is_empty() {
        return true; // Can't read = allow
    }

    for line in &lines {
        let lower = line.to_lowercase();

        // Check for log file reads or greps
        for marker in LOG_INSPECTION_MARKERS {
            if lower.contains(marker) {
                return true;
            }
        }

        // Check for tail/grep of log files
        if (lower.contains("tail") || lower.contains("grep") || lower.contains("rg "))
            && lower.contains(".log")
        {
            return true;
        }
    }

    false
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
            "Log-first gate: you're fixing a bug but haven't checked the logs. \
             Check the logs before fixing. What do chorus.log and hooks.log say about this failure? \
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

    fn make_input(tool: &str, file_path: &str) -> HookInput {
        HookInput {
            tool_name: Some(tool.to_string()),
            tool_input: Some(serde_json::json!({
                "file_path": file_path
            })),
            tool_response: None,
            session_id: None,
            cwd: Some("/Users/jeffbridwell/CascadeProjects/chorus/engineer".to_string()),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("kade".to_string()),
        }
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
