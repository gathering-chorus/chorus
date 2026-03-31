//! Quality gate (#1717)
//!
//! PreToolUse on Skill("demo"): spawns a Claude agent to review work against AC.
//! PostToolUse on Edit/Write: lightweight pattern check for common issues.
//! Blocks demo if AC gaps found. Warns on risky edits.

use crate::state::AppState;
use crate::types::{permission_deny_json, HookInput, HookResponse};
use std::process::Command;
use tracing::{info, warn};

const BOARD_TS: &str = "/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards";

// --- PreToolUse: Agent review before /demo ---

/// Gate /demo invocations — fetch card AC, diff files, review with Claude agent
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
        _ => return HookResponse::allow(), // Can't determine card = allow
    };

    info!(card = %card_id, "quality-gate: pre-demo review starting");

    // Fetch card AC
    let card_view = match Command::new("bash")
        .args([BOARD_TS, "view", &card_id])
        .output()
    {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => {
            warn!("quality-gate: couldn't fetch card {}", card_id);
            return HookResponse::allow(); // Don't block on board failure
        }
    };

    // Extract AC lines
    let ac_lines: Vec<&str> = card_view
        .lines()
        .filter(|l| {
            let trimmed = l.trim();
            trimmed.starts_with("- [ ]") || trimmed.starts_with("- [x]")
        })
        .collect();

    if ac_lines.is_empty() {
        info!("quality-gate: no AC found for #{}, skipping review", card_id);
        return HookResponse::allow();
    }

    // Get recent git diff for this session's changes
    let diff = Command::new("git")
        .args(["diff", "HEAD~3", "--stat"])
        .current_dir("/Users/jeffbridwell/CascadeProjects/chorus")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    // Build review prompt
    let ac_text = ac_lines.join("\n");
    let review_input = format!(
        "Card #{card_id} is ready for demo. Review the AC against the changes.\n\n\
         ## Acceptance Criteria\n{ac_text}\n\n\
         ## Recent Changes (git diff --stat)\n{diff}\n\n\
         Respond with ONLY a JSON object:\n\
         {{\"pass\": true/false, \"gaps\": [\"description of each gap\"]}}\n\
         If all AC items appear addressed by the changes, pass=true with empty gaps.\n\
         If any AC item has no corresponding change, pass=false with specific gaps.\n\
         Be practical — don't block for minor style issues."
    );

    // Call Claude agent for review (budget-limited, fast model)
    let review_result = Command::new("claude")
        .args([
            "-p",
            "--model", "haiku",
            "--permission-mode", "dontAsk",
            "--no-session-persistence",
            "--max-budget-usd", "0.02",
            "--output-format", "json",
            "--disallowedTools", "Bash,Edit,Write,Glob,Grep,WebFetch,WebSearch,NotebookEdit,Task",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .env_remove("CLAUDECODE")
        .spawn()
        .and_then(|mut child| {
            if let Some(ref mut stdin) = child.stdin {
                let _ = std::io::Write::write_all(stdin, review_input.as_bytes());
            }
            child.wait_with_output()
        });

    match review_result {
        Ok(output) if output.status.success() => {
            let response = String::from_utf8_lossy(&output.stdout);
            // Parse the agent's response
            if let Ok(envelope) = serde_json::from_str::<serde_json::Value>(&response) {
                let inner = envelope.get("result")
                    .and_then(|r| r.as_str())
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                    .or_else(|| envelope.get("structured_output").cloned())
                    .unwrap_or(envelope.clone());

                let pass = inner.get("pass").and_then(|v| v.as_bool()).unwrap_or(true);
                let gaps = inner.get("gaps")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join("; "))
                    .unwrap_or_default();

                if !pass && !gaps.is_empty() {
                    warn!(card = %card_id, "quality-gate: BLOCKED — {}", gaps);
                    return HookResponse::deny(&permission_deny_json(&format!(
                        "Quality gate: AC gaps found for #{}. Fix before demo:\n{}",
                        card_id, gaps
                    )));
                }

                info!(card = %card_id, "quality-gate: PASS");
            }
        }
        Ok(_) => {
            info!("quality-gate: claude agent failed, allowing demo");
        }
        Err(e) => {
            info!("quality-gate: claude spawn failed ({}), allowing demo", e);
        }
    }

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
    use serde_json::json;

    fn make_input(tool: &str, key: &str, val: &str) -> HookInput {
        HookInput {
            tool_name: Some(tool.to_string()),
            tool_input: Some(json!({ key: val })),
            tool_response: None,
            session_id: None,
            cwd: Some("/Users/jeffbridwell/CascadeProjects/chorus/architect".to_string()),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("silas".to_string()),
        }
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
