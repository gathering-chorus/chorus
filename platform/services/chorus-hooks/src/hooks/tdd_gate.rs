//! TDD gate (#1814, refined per Jeff's feedback)
//!
//! Two enforcement points:
//! 1. PreToolUse on Edit/Write of production code: blocks unless a test file
//!    was edited FIRST in this session. Tests before code, not tests before done.
//! 2. PreToolUse on demo/done/acp: blocks unless tests were actually RUN.
//!
//! Jeff's direction: "Running tests just to get past the gate is performative.
//! TDD means tests come before code, not after."

use crate::state::AppState;
use crate::types::{permission_deny_json, HookInput, HookResponse};

/// Test file patterns — delegated to shared module (#2076)
fn is_test_file(path: &str) -> bool {
    crate::shared::file_classification::is_test_file(path)
}

/// Production code — delegated to shared module (#2076)
fn is_production_code(path: &str) -> bool {
    crate::shared::file_classification::is_production_code(path)
}

/// Check if the current tool call is a demo or done action
fn is_demo_or_done(input: &HookInput) -> bool {
    let tool = input.tool_name_str();

    if tool == "Skill" {
        let skill = input.get_tool_input_str("skill");
        return skill == "demo" || skill == "acp";
    }

    if tool == "Bash" {
        let cmd = input.get_tool_input_str("command");
        return cmd.contains("board-ts done") || cmd.contains("cards done")
            || (cmd.contains("board-ts") || cmd.contains("/cards ")) && cmd.contains(" done ");
    }

    false
}

/// Check if the current tool call is editing production code
fn is_code_edit(input: &HookInput) -> bool {
    let tool = input.tool_name_str();
    if tool != "Edit" && tool != "Write" {
        return false;
    }
    let file_path = input.get_tool_input_str("file_path");
    is_production_code(&file_path)
}

/// Scan session for test file edits BEFORE the current point
fn has_test_file_edit(input: &HookInput, state: &AppState) -> bool {
    let session_id = match &input.session_id {
        Some(id) => id.clone(),
        None => return true,
    };

    let cwd = input.cwd.as_deref().unwrap_or("");
    let lines = state.session_cache.get_tail(&session_id, cwd, 500);
    if lines.is_empty() {
        return false; // No session data = no evidence of tests = block
    }

    for line in &lines {
        let lower = line.to_lowercase();
        // Look for Edit/Write tool calls on test files
        if (lower.contains("\"edit\"") || lower.contains("\"write\""))
            && (lower.contains(".test.") || lower.contains(".spec.")
                || lower.contains("/tests/") || lower.contains("/test/")
                || lower.contains("_test.") || lower.contains(".feature"))
        {
            return true;
        }
    }

    false
}

/// Scan session JSONL for evidence of test runs
fn has_test_run(input: &HookInput, state: &AppState) -> bool {
    let session_id = match &input.session_id {
        Some(id) => id.clone(),
        None => return true,
    };

    let cwd = input.cwd.as_deref().unwrap_or("");
    let lines = state.session_cache.get_tail(&session_id, cwd, 500);
    if lines.is_empty() {
        return true;
    }

    for line in &lines {
        let lower = line.to_lowercase();
        if (lower.contains("cargo test") || lower.contains("npx jest")
            || lower.contains("npm test") || lower.contains("npm run test")
            || lower.contains("npx cucumber") || lower.contains("vitest"))
            && lower.contains("\"bash\"")
        {
            return true;
        }
    }

    false
}

/// Card-type-specific test guidance — tells the role what kind of test to write
fn test_guidance(card_type: &str) -> &'static str {
    match card_type {
        "fix" => "Write a test that reproduces the bug from the AC. It should FAIL now (proving the bug exists) and PASS after your fix. Test what Jeff sees break, not internals.",
        "new" => "Write a test for the first AC item. Describe what Jeff experiences — page loads, message appears, data shows up. The test should FAIL because the feature doesn't exist yet.",
        "enhance" => "Write a test that verifies the new behavior alongside existing behavior. Both the enhancement and the original must pass. Start from the AC.",
        _ => "Write a test that covers the first AC item. It should FAIL before you write the code.",
    }
}

pub fn check(input: &HookInput, state: &AppState) -> HookResponse {
    // Skip TDD for chore and swat cards
    let card_type = crate::types::card_type_for_role(input.role().as_str());
    if card_type == "chore" || card_type == "swat" {
        return HookResponse::allow();
    }

    // Gate 1: Production code edit — require test file edit first
    if is_code_edit(input) {
        if !has_test_file_edit(input, state) {
            let file_path = input.get_tool_input_str("file_path");
            let fname = file_path.rsplit('/').next().unwrap_or(&file_path);
            let guidance = test_guidance(&card_type);
            return HookResponse::deny(&permission_deny_json(&format!(
                "TDD gate: you're editing production code ({}) but haven't written a test yet in this session. \
                 {} \
                 DEC-1674: AC → tests (red) → code → tests (green) → demo.",
                fname, guidance
            )));
        }
    }

    // Gate 2: Demo/done/acp — require test runs
    if is_demo_or_done(input) {
        if !has_test_run(input, state) {
            return HookResponse::deny(&permission_deny_json(
                "TDD gate: no test runs detected in this session. \
                 Run tests (cargo test, npx jest, npx cucumber-js) before demo/done. \
                 DEC-1674: AC → tests → code → green → demo."
            ));
        }
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
    fn allows_non_code_tools() {
        let input = make_input("Read", "file_path", "/some/file.ts");
        let r = check(&input, &state());
        assert!(r.stdout.is_none());
    }

    #[test]
    fn test_file_detection() {
        assert!(is_test_file("src/hooks/tdd_gate_test.rs"));
        assert!(is_test_file("src/app.test.ts"));
        assert!(is_test_file("tests/integration.rs"));
        assert!(is_test_file("features/gates/tdd.feature"));
        assert!(!is_test_file("src/main.rs"));
        assert!(!is_test_file("src/app.ts"));
    }

    #[test]
    fn production_code_detection() {
        assert!(is_production_code("src/main.rs"));
        assert!(is_production_code("src/app.ts"));
        assert!(!is_production_code("src/app.test.ts"));
        assert!(!is_production_code("tests/integration.rs"));
        assert!(!is_production_code("README.md"));
        assert!(!is_production_code("target/debug/build.rs"));
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
    fn code_edit_detected() {
        let input = make_input("Edit", "file_path", "/project/src/main.rs");
        assert!(is_code_edit(&input));
    }

    #[test]
    fn test_edit_not_code_edit() {
        let input = make_input("Edit", "file_path", "/project/tests/gate.test.ts");
        assert!(!is_code_edit(&input));
    }

    #[test]
    fn allows_demo_without_session() {
        let input = make_input("Skill", "skill", "demo");
        let r = check(&input, &state());
        assert!(r.stdout.is_none());
    }
}
