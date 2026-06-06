//! Nudge Blast Radius (#1658)
//!
//! PreToolUse hook on Bash when invoking nudge.sh.
//! Warns (doesn't block) if the target role is WIP on a card,
//! making the interrupt cost visible before sending.

use crate::shared::state_paths::chorus_root;
use crate::types::{HookInput, HookResponse};
use std::process::Command;
use tracing::warn;

fn role_state_script_path() -> String {
    format!("{}/platform/scripts/role-state", chorus_root())
}
const ROLES: &[&str] = &["wren", "silas", "kade"];

/// Check if a bash command is invoking nudge and warn about blast radius
pub async fn check(input: &HookInput) -> HookResponse {
    let command = input.get_tool_input_str("command");

    // Detect nudge invocations
    if !command.contains("nudge.sh") && !command.contains("/nudge ") && !command.contains("/nudge\"") {
        return HookResponse::allow();
    }

    // Parse target role from command args
    let target = match extract_target_role(&command) {
        Some(r) => r,
        None => return HookResponse::allow(),
    };

    // Don't warn when nudging yourself
    let sender = input.role();
    if sender.as_str() == target {
        return HookResponse::allow();
    }

    // Query target role's state
    let state = query_role_state(&target);
    if state.is_empty() {
        return HookResponse::allow();
    }

    // Parse JSON response
    let parsed: serde_json::Value = match serde_json::from_str(&state) {
        Ok(v) => v,
        Err(_) => return HookResponse::allow(),
    };

    let role_state = parsed.get("state").and_then(|v| v.as_str()).unwrap_or("");
    let card = parsed.get("card").and_then(|v| v.as_u64());

    if role_state == "building" {
        if let Some(card_id) = card {
            let msg = format!(
                "⚠ {} is WIP on #{} — this nudge will interrupt. Sending anyway.",
                target, card_id
            );
            warn!("{}", msg);
            return HookResponse::warn_stderr(&msg);
        }
    }

    HookResponse::allow()
}

fn extract_target_role(command: &str) -> Option<String> {
    // nudge.sh <role> "message" — role is first arg after nudge command
    for role in ROLES {
        // Check for role as a standalone word after nudge
        let patterns = [
            format!("nudge.sh {}", role),
            format!("nudge.sh \"{}\"", role),
            format!("/nudge {}", role),
            format!("/nudge \"{}\"", role),
        ];
        for pattern in &patterns {
            if command.contains(pattern) {
                return Some(role.to_string());
            }
        }
    }

    // Fallback: find role name as word after nudge.sh or /nudge
    let parts: Vec<&str> = command.split_whitespace().collect();
    for (i, part) in parts.iter().enumerate() {
        if (part.contains("nudge.sh") || *part == "nudge" || part.ends_with("/nudge"))
            && i + 1 < parts.len()
        {
            let candidate = parts[i + 1].trim_matches('"').trim_matches('\'');
            if ROLES.contains(&candidate) {
                return Some(candidate.to_string());
            }
        }
    }

    None
}

fn query_role_state(role: &str) -> String {
    let role_state_path = role_state_script_path();
    let output = Command::new(&role_state_path)
        .args(["query", role])
        .output();

    match output {
        Ok(o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HookInput;
    use serde_json::json;

    fn make_input(tool: &str) -> HookInput {
        HookInput {
            tool_name: Some(tool.to_string()),
            tool_input: Some(json!({"command": "echo test", "file_path": "/tmp/test", "skill": "demo"})),
            tool_response: None,
            session_id: Some("test".to_string()),
            cwd: Some(format!("{}/architect", chorus_root())),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("silas".to_string()),
            chorus_worktree_override: None, trace_id: None,}
    }

    #[tokio::test]
    async fn allows_non_matching_tool() {
        let input = make_input("Read");
        let r = check(&input).await;
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn allows_normal_input() {
        let input = make_input("Bash");
        let r = check(&input).await;
        assert_eq!(r.exit_code, 0);
    }
}
