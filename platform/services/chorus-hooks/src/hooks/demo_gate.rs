//! Demo gate — dispatch only (#1814 AC3, #1809)
//!
//! PreToolUse on board-ts done / /acp: blocks Done without demo evidence.
//! All gate logic lives in skills/demo/gates/done-gate.sh (owned by Wren).
//! This hook detects done actions, extracts card IDs, and dispatches.

use crate::shared::state_paths::chorus_root;
use crate::types::{permission_deny_json, HookInput, HookResponse};
use std::process::Command;
use tracing::{info, warn};

/// Check if the tool call is a "done" action
fn is_done_action(input: &HookInput) -> bool {
    let tool = input.tool_name_str();

    if tool == "Skill" {
        let skill = input.get_tool_input_str("skill");
        return skill == "acp";
    }

    if tool == "Bash" {
        let cmd = input.get_tool_input_str("command");
        return cmd.contains("board-ts done") || cmd.contains("cards done")
            || (cmd.contains("board-ts") || cmd.contains("/cards ")) && cmd.contains(" done ");
    }

    false
}

/// Extract card ID from the command
fn extract_card_id(input: &HookInput) -> Option<String> {
    let tool = input.tool_name_str();

    if tool == "Skill" {
        let args = input.get_tool_input_str("args");
        return args.split_whitespace().next().map(|s| s.to_string());
    }

    if tool == "Bash" {
        let cmd = input.get_tool_input_str("command");
        if let Some(pos) = cmd.find("done ") {
            let after = &cmd[pos + 5..];
            return after.split_whitespace().next()
                .filter(|s| s.chars().all(|c| c.is_ascii_digit()))
                .map(|s| s.to_string());
        }
    }

    None
}

pub fn check(input: &HookInput) -> HookResponse {
    if !is_done_action(input) {
        return HookResponse::allow();
    }

    let card_id = match extract_card_id(input) {
        Some(id) => id,
        None => return HookResponse::allow(),
    };

    info!(card = %card_id, "demo-gate: dispatching to done-gate.sh");

    let script = format!("{}/platform/skills/demo/gates/done-gate.sh", chorus_root());
    let role = input.role();
    let output = Command::new("bash")
        .args([&script, &card_id, role.as_str()])
        .output();

    match output {
        Ok(o) if o.status.success() => {
            info!(card = %card_id, "demo-gate: passed");
            HookResponse::allow()
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
            let msg = if stderr.is_empty() {
                format!("Demo gate: no demo evidence for #{}. Run /demo {} before accepting.", card_id, card_id)
            } else {
                stderr
            };
            warn!("{}", msg);
            HookResponse::deny(&permission_deny_json(&msg))
        }
        Err(e) => {
            warn!("demo-gate: failed to run done-gate.sh: {}", e);
            HookResponse::allow()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HookInput;

    fn make_input(tool: &str, key: &str, val: &str) -> HookInput {
        HookInput {
            tool_name: Some(tool.to_string()),
            tool_input: Some(serde_json::json!({ key: val })),
            tool_response: None,
            session_id: None,
            cwd: Some(format!("{}/engineer", chorus_root())),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("kade".to_string()),
        }
    }

    #[test]
    fn allows_non_done_actions() {
        let input = make_input("Edit", "file_path", "/some/file.ts");
        let r = check(&input);
        assert!(r.stdout.is_none());
    }

    #[test]
    fn detects_acp_skill() {
        let input = make_input("Skill", "skill", "acp");
        assert!(is_done_action(&input));
    }

    #[test]
    fn detects_board_done() {
        let input = make_input("Bash", "command", "bash board-ts done 1811");
        assert!(is_done_action(&input));
    }

    #[test]
    fn extracts_card_id_from_acp() {
        let mut input = make_input("Skill", "skill", "acp");
        input.tool_input = Some(serde_json::json!({ "skill": "acp", "args": "1811" }));
        assert_eq!(extract_card_id(&input), Some("1811".to_string()));
    }

    #[test]
    fn extracts_card_id_from_board_done() {
        let input = make_input("Bash", "command", "bash board-ts done 1811");
        assert_eq!(extract_card_id(&input), Some("1811".to_string()));
    }

    #[test]
    fn ignores_non_done_bash() {
        let input = make_input("Bash", "command", "ls -la");
        assert!(!is_done_action(&input));
    }
}
