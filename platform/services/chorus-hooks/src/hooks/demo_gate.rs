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
#[allow(dead_code)]
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
#[allow(dead_code)]
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

/// Check if the command contains --proven flag (#1916)
#[allow(dead_code)]
fn has_proven_flag(input: &HookInput) -> bool {
    let tool = input.tool_name_str();
    if tool == "Bash" {
        let cmd = input.get_tool_input_str("command");
        return cmd.contains("--proven");
    }
    if tool == "Skill" {
        let args = input.get_tool_input_str("args");
        return args.contains("--proven");
    }
    false
}

#[allow(dead_code)]
pub fn check(input: &HookInput) -> HookResponse {
    if !is_done_action(input) {
        return HookResponse::allow();
    }

    let card_id = match extract_card_id(input) {
        Some(id) => id,
        None => return HookResponse::allow(),
    };

    // #1916: --proven bypass for retroactive closure of cards
    // proven by distributed work across multiple cards
    if has_proven_flag(input) {
        info!(card = %card_id, "demo-gate: --proven flag detected, skipping evidence check");
        return HookResponse::allow();
    }

    info!(card = %card_id, "demo-gate: dispatching to done-gate.sh");

    let script = format!("{}/skills/demo/gates/done-gate.sh", chorus_root());
    let role = input.role();
    let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/jeffbridwell".to_string());
    let output = Command::new("bash")
        .args(["-l", &script, &card_id, role.as_str()])
        .env("CHORUS_ROOT", chorus_root())
        .env("HOME", &home)
        .env("PATH", format!("{}/CascadeProjects/chorus/platform/scripts:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin", home))
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
            cwd: Some(format!("{}/roles/kade", chorus_root())),
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

    // --- #1916: --proven bypass ---

    #[test]
    fn detects_proven_flag_in_bash() {
        let input = make_input("Bash", "command", "bash cards done 1783 --proven \"1815 1898\"");
        assert!(has_proven_flag(&input));
    }

    #[test]
    fn no_proven_flag_in_normal_done() {
        let input = make_input("Bash", "command", "bash cards done 1783");
        assert!(!has_proven_flag(&input));
    }

    #[test]
    fn proven_flag_in_acp_args() {
        let mut input = make_input("Skill", "skill", "acp");
        input.tool_input = Some(serde_json::json!({ "skill": "acp", "args": "1783 --proven 1815 1898" }));
        assert!(has_proven_flag(&input));
    }

    #[test]
    fn proven_flag_allows_done_without_evidence() {
        let input = make_input("Bash", "command", "bash cards done 1783 --proven \"1815 1898\"");
        let r = check(&input);
        // Should allow (no stdout = no deny)
        assert!(r.stdout.is_none(), "proven flag should bypass demo gate");
    }
}
