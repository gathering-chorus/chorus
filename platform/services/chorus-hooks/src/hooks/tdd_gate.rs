//! TDD gate (#1814 AC1)
//! PreToolUse on Skill("demo") and board-ts done: blocks without tests covering AC items.
//! Scans session JSONL for test runs (cargo test, npx jest, npm test).
//! Jeff's direction: "No card is done without tests covering every AC item."

use crate::state::AppState;
use crate::types::{permission_deny_json, HookInput, HookResponse};

/// Check if the current tool call is a demo or done action
fn is_demo_or_done(input: &HookInput) -> bool {
    let tool = input.tool_name_str();

    // Skill invocation of /demo
    if tool == "Skill" {
        let skill = input.get_tool_input_str("skill");
        return skill == "demo" || skill == "acp";
    }

    // Bash calling board-ts done
    if tool == "Bash" {
        let cmd = input.get_tool_input_str("command");
        return cmd.contains("board-ts done") || cmd.contains("cards done")
            || (cmd.contains("board-ts") || cmd.contains("/cards ")) && cmd.contains(" done ");
    }

    false
}

/// Scan session JSONL for evidence of test runs (uses shared cache #1861)
fn has_test_evidence(input: &HookInput, state: &AppState) -> bool {
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
        if (lower.contains("cargo test") || lower.contains("npx jest")
            || lower.contains("npm test") || lower.contains("npm run test")
            || lower.contains("vitest"))
            && lower.contains("\"bash\"")
        {
            return true;
        }
    }

    false
}

pub fn check(input: &HookInput, state: &AppState) -> HookResponse {
    if !is_demo_or_done(input) {
        return HookResponse::allow();
    }

    // Skip TDD for chore cards (#1881) — maintenance/docs don't need tests
    let card_type = crate::types::card_type_for_role(input.role().as_str());
    if card_type == "chore" {
        return HookResponse::allow();
    }

    if !has_test_evidence(input, state) {
        return HookResponse::deny(&permission_deny_json(
            "TDD gate: no test runs detected in this session. \
             Run tests (cargo test, npx jest) before demo/done. \
             DEC-1674: AC → tests → code → green → demo."
        ));
    }

    HookResponse::allow()
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
    fn allows_non_demo_tools() {
        let input = make_input("Edit", "file_path", "/some/file.ts");
        let r = check(&input, &state());
        assert!(r.stdout.is_none());
    }

    #[test]
    fn detects_demo_skill() {
        let input = make_input("Skill", "skill", "demo");
        assert!(is_demo_or_done(&input));
    }

    #[test]
    fn detects_acp_skill() {
        let input = make_input("Skill", "skill", "acp");
        assert!(is_demo_or_done(&input));
    }

    #[test]
    fn detects_board_done() {
        let input = make_input("Bash", "command", "bash board-ts done 1811");
        assert!(is_demo_or_done(&input));
    }

    #[test]
    fn ignores_other_bash() {
        let input = make_input("Bash", "command", "ls -la");
        assert!(!is_demo_or_done(&input));
    }

    #[test]
    fn allows_demo_without_session() {
        // No session ID = can't verify, allow
        let input = make_input("Skill", "skill", "demo");
        let r = check(&input, &state());
        assert!(r.stdout.is_none());
    }
}
