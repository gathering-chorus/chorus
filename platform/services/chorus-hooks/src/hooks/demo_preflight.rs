//! Demo Preflight Gate — dispatch only (#1657, #1668, #1809)
//!
//! PreToolUse hook on Skill tool when skill="demo".
//! All gate logic lives in skills/demo/gates/preflight.sh (owned by Wren).
//! This hook dispatches and maps exit codes: 0 = allow, 1 = deny (stderr = message).

use crate::shared::state_paths::chorus_root;
use crate::types::{HookInput, HookResponse, permission_deny_json};
use std::process::Command;
use tracing::{info, warn};

pub async fn check(input: &HookInput) -> HookResponse {
    let skill = input.get_tool_input_str("skill");
    if skill != "demo" {
        return HookResponse::allow();
    }

    let args = input.get_tool_input_str("args");
    let card_id = args.split_whitespace()
        .find(|s| s.chars().all(|c| c.is_ascii_digit()))
        .unwrap_or("");

    if card_id.is_empty() {
        return HookResponse::allow();
    }

    info!(card = %card_id, "demo-preflight: dispatching to preflight.sh");

    let script = format!("{}/platform/skills/demo/gates/preflight.sh", chorus_root());
    let output = Command::new("bash")
        .args([&script, card_id])
        .output();

    match output {
        Ok(o) if o.status.success() => {
            info!(card = %card_id, "demo-preflight: all gates passed");
            HookResponse::allow()
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
            let msg = if stderr.is_empty() {
                format!("Demo blocked: preflight gate failed for #{}.", card_id)
            } else {
                stderr
            };
            warn!("{}", msg);
            HookResponse::deny(&permission_deny_json(&msg))
        }
        Err(e) => {
            warn!("demo-preflight: failed to run preflight.sh: {}", e);
            HookResponse::allow() // Don't block on dispatch failure
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HookInput;
    use serde_json::json;

    fn make_skill(skill: &str) -> HookInput {
        HookInput {
            tool_name: Some("Skill".to_string()),
            tool_input: Some(json!({"skill": skill})),
            tool_response: None,
            session_id: Some("test".to_string()),
            cwd: Some(format!("{}/architect", chorus_root())),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("silas".to_string()),
        }
    }

    #[tokio::test]
    async fn allows_non_demo_skills() {
        let input = make_skill("acp");
        let r = check(&input).await;
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn allows_non_skill_tools() {
        let input = HookInput {
            tool_name: Some("Bash".to_string()),
            tool_input: Some(json!({"command": "echo test"})),
            tool_response: None,
            session_id: Some("test".to_string()),
            cwd: Some(format!("{}/architect", chorus_root())),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("silas".to_string()),
        };
        let r = check(&input).await;
        assert_eq!(r.exit_code, 0);
    }
}
