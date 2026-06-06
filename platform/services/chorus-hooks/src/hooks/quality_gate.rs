//! Quality gate (#1717)
//!
//! PreToolUse on Skill("demo"): spawns a Claude agent to review work against AC.
//! PostToolUse on Edit/Write: lightweight pattern check for common issues.
//! Blocks demo if AC gaps found. Warns on risky edits.

use crate::state::chorus_log as emit_event;
use crate::types::{HookInput, HookResponse};
use tracing::{info, warn};

// --- PreToolUse: Agent review before /demo ---

/// Gate /demo invocations — emit quality gate event, allow through.
/// The actual agent review runs inside the /demo skill (not inline —
/// the 5s hook timeout is too short for a Claude agent call).
pub async fn pre_demo_check(input: &HookInput) -> HookResponse {
    let tool = input.tool_name_str();
    if tool != "Skill" {
        return HookResponse::allow();
    }

    let skill = input.get_tool_input_str("skill");
    if skill != "demo" {
        return HookResponse::allow();
    }

    let args = input.get_tool_input_str("args");
    let card_id = match args.split_whitespace().next() {
        Some(id) if id.chars().all(|c| c.is_ascii_digit()) => id.to_string(),
        _ => return HookResponse::allow(),
    };

    info!(card = %card_id, "quality-gate: demo initiated");

    // Emit spine event — proves the gate saw the demo request
    let role_name = input.role().as_str().to_string();
    let card_id_clone = card_id.clone();
    tokio::spawn(async move {
        emit_event("quality.gate.started", &role_name, &[("card", &card_id_clone)]).await;
    });

    // The hook emits the event and allows through.
    // Agent review runs in the /demo skill (needs 30-50s, hook timeout is 5s).
    HookResponse::allow()
}

// --- PostToolUse: Lightweight edit checks ---

/// Check Edit/Write results for common quality issues
pub fn post_edit_check(input: &HookInput) -> HookResponse {
    let tool = input.tool_name_str();
    if tool != "Edit" && tool != "Write" {
        return HookResponse::allow();
    }

    let file_path = input.get_tool_input_str("file_path");
    if file_path.is_empty() {
        return HookResponse::allow();
    }

    // Check 1: Test file deletion — warn if removing test files
    if (file_path.contains("/tests/") || file_path.contains(".test.") || file_path.contains("_test."))
        && tool == "Write"
    {
        let content = input.get_tool_input_str("content");
        if content.is_empty() {
            warn!(file = %file_path, "quality-gate: test file appears to be emptied");
        }
    }

    // Check 2: Large deletions in Edit — old_string much bigger than new_string
    if tool == "Edit" {
        let old = input.get_tool_input_str("old_string");
        let new = input.get_tool_input_str("new_string");
        if old.len() > 500 && new.is_empty() {
            warn!(
                file = %file_path,
                deleted_chars = old.len(),
                "quality-gate: large deletion with empty replacement"
            );
        }
    }

    // These are warnings only — never block edits
    HookResponse::allow()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::state_paths::chorus_root;
    use serde_json::json;

    fn make_input(tool: &str, key: &str, val: &str) -> HookInput {
        HookInput {
            tool_name: Some(tool.to_string()),
            tool_input: Some(json!({ key: val })),
            tool_response: None,
            session_id: None,
            cwd: Some(format!("{}/architect", chorus_root())),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("silas".to_string()),
            chorus_worktree_override: None, trace_id: None,}
    }

    #[tokio::test]
    async fn allows_non_demo_skills() {
        let input = make_input("Skill", "skill", "pair");
        let r = pre_demo_check(&input).await;
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn allows_non_skill_tools() {
        let input = make_input("Bash", "command", "echo test");
        let r = pre_demo_check(&input).await;
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn allows_demo_without_card_id() {
        let mut input = make_input("Skill", "skill", "demo");
        input.tool_input = Some(json!({"skill": "demo", "args": ""}));
        let r = pre_demo_check(&input).await;
        assert_eq!(r.exit_code, 0);
    }

    #[test]
    fn post_edit_allows_normal_edits() {
        let mut input = make_input("Edit", "file_path", "/src/main.rs");
        input.tool_input = Some(json!({
            "file_path": "/src/main.rs",
            "old_string": "let x = 1;",
            "new_string": "let x = 2;"
        }));
        let r = post_edit_check(&input);
        assert_eq!(r.exit_code, 0);
    }

    #[test]
    fn post_edit_allows_writes() {
        let mut input = make_input("Write", "file_path", "/src/new.rs");
        input.tool_input = Some(json!({
            "file_path": "/src/new.rs",
            "content": "fn main() {}"
        }));
        let r = post_edit_check(&input);
        assert_eq!(r.exit_code, 0);
    }
}
