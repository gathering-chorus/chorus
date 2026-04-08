//! Demo Provenance — dispatch only (#1670, #1809)
//!
//! PostToolUse hook after /demo skill completes.
//! All provenance logic lives in skills/demo/gates/provenance.sh (owned by Wren).
//! This hook dispatches: script generates brief, emits spine event, returns status on stderr.

use crate::shared::state_paths::chorus_root;
use crate::types::{HookInput, HookResponse};
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

    let role = input.role();

    info!(card = %card_id, "demo-provenance: dispatching to provenance.sh");

    let script = format!("{}/skills/demo/gates/provenance.sh", chorus_root());
    let output = Command::new("bash")
        .args([&script, card_id, role.as_str()])
        .output();

    match output {
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
            if !stderr.is_empty() {
                info!(card = %card_id, "demo-provenance: {}", stderr);
                return HookResponse::warn_stderr(&stderr);
            }
            HookResponse::allow()
        }
        Err(e) => {
            warn!("demo-provenance: failed to run provenance.sh: {}", e);
            HookResponse::allow()
        }
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
        }
    }

    #[tokio::test]
    async fn allows_non_matching_tool() {
        let input = make_input("Read");
        let r = check(&input).await;
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn allows_normal_input() {
        let input = make_input("Skill");
        let r = check(&input).await;
        assert_eq!(r.exit_code, 0);
    }
}
