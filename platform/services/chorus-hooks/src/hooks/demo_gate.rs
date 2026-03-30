//! Demo gate (#1814 AC3)
//! PreToolUse on board-ts done: blocks Done without demo evidence.
//! Checks for demo spine event or demo brief in product-manager/briefs/.
//! Jeff's direction: "block done without demo evidence."

use crate::state::AppState;
use crate::types::{permission_deny_json, HookInput, HookResponse};

/// Check if the tool call is a "done" action
fn is_done_action(input: &HookInput) -> bool {
    let tool = input.tool_name_str();

    // Skill invocation of /acp
    if tool == "Skill" {
        let skill = input.get_tool_input_str("skill");
        return skill == "acp";
    }

    // Bash calling board-ts done
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
        // /acp 1811
        return args.split_whitespace().next().map(|s| s.to_string());
    }

    if tool == "Bash" {
        let cmd = input.get_tool_input_str("command");
        // board-ts done 1811
        if let Some(pos) = cmd.find("done ") {
            let after = &cmd[pos + 5..];
            return after.split_whitespace().next()
                .filter(|s| s.chars().all(|c| c.is_ascii_digit()))
                .map(|s| s.to_string());
        }
    }

    None
}

/// Check for demo evidence: demo brief or demo spine event in session (uses shared cache #1861)
fn has_demo_evidence(input: &HookInput, card_id: &str, state: &AppState) -> bool {
    // Check 1: Demo brief exists in product-manager/briefs/
    let today = chrono_today();
    let brief_pattern = format!("demo-{}", card_id);
    let briefs_dir = "/Users/jeffbridwell/CascadeProjects/chorus/product-manager/briefs";
    if let Ok(entries) = std::fs::read_dir(briefs_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.contains(&brief_pattern) {
                return true;
            }
        }
    }
    // Also check archive subdirectory for today
    let archive_dir = format!("{}/archive/{}", briefs_dir, today);
    if let Ok(entries) = std::fs::read_dir(&archive_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.contains(&brief_pattern) {
                return true;
            }
        }
    }

    // Check 2: Demo spine event in session JSONL (cached)
    let session_id = match &input.session_id {
        Some(id) => id.clone(),
        None => return false,
    };

    let cwd = input.cwd.as_deref().unwrap_or("");
    let lines = state.session_cache.get_tail(&session_id, cwd, 300);

    for line in &lines {
        let lower = line.to_lowercase();
        if lower.contains("demo") && lower.contains(card_id) {
            return true;
        }
    }

    false
}

fn chrono_today() -> String {
    // Simple date without chrono dependency
    let output = std::process::Command::new("date")
        .args(["+%Y-%m-%d"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    output.trim().to_string()
}

pub fn check(input: &HookInput, state: &AppState) -> HookResponse {
    if !is_done_action(input) {
        return HookResponse::allow();
    }

    let card_id = match extract_card_id(input) {
        Some(id) => id,
        None => return HookResponse::allow(), // Can't determine card = allow
    };

    if has_demo_evidence(input, &card_id, state) {
        return HookResponse::allow();
    }

    HookResponse::deny(&permission_deny_json(&format!(
        "Demo gate: no demo evidence for #{card_id}. \
         Run /demo {card_id} before accepting. \
         DEC-048: deploy, demo to Jeff, then accept."
    )))
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
            cwd: Some("/Users/jeffbridwell/CascadeProjects/chorus/engineer".to_string()),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("kade".to_string()),
        }
    }

    fn state() -> AppState { AppState::new() }

    #[test]
    fn allows_non_done_actions() {
        let input = make_input("Edit", "file_path", "/some/file.ts");
        let r = check(&input, &state());
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

    #[test]
    fn allows_done_without_session() {
        // No session + no demo brief = would block, but no card ID extractable from skill without args
        let input = make_input("Skill", "skill", "acp");
        // No args = can't extract card ID = allow
        let r = check(&input, &state());
        assert!(r.stdout.is_none());
    }
}
