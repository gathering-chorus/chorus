//! Demo Show — dispatch only (#2864)
//!
//! PostToolUse hook after /demo skill completes.
//! All gate logic lives in skills/demo/gates/show-gate.sh (owned by Wren).
//! This hook dispatches: script reads spine for card.demo.started +
//! jeff.input.delivered window + demo.preflight.passed, emits
//! demo.show.completed on success or demo.show.failed with reason.
//!
//! PostToolUse semantics: never blocks. accept_gate.rs is the consumer that
//! refuses /acp without a demo.show.completed event for the card.
//!
//! Auto-mode behavior: identical to interactive mode. No `if (auto_mode)`
//! branch anywhere — the spine event is recorded the same way regardless
//! of harness mode.

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
    let card_id = args
        .split_whitespace()
        .find(|s| s.chars().all(|c| c.is_ascii_digit()))
        .unwrap_or("");

    if card_id.is_empty() {
        return HookResponse::allow();
    }

    let role = input.role();

    info!(card = %card_id, "demo-show: dispatching to show-gate.sh");

    let script = format!("{}/skills/demo/gates/show-gate.sh", chorus_root());
    let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/jeffbridwell".to_string());
    let output = Command::new("bash")
        .args(["-l", &script, card_id, role.as_str()])
        .env("CHORUS_ROOT", chorus_root())
        .env("HOME", &home)
        .env(
            "PATH",
            format!(
                "{}/CascadeProjects/chorus/platform/scripts:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin",
                home
            ),
        )
        .output();

    match output {
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
            if !stderr.is_empty() {
                info!(card = %card_id, "demo-show: {}", stderr);
                return HookResponse::warn_stderr(&stderr);
            }
            HookResponse::allow()
        }
        Err(e) => {
            warn!("demo-show: failed to run show-gate.sh: {}", e);
            HookResponse::allow() // Don't block on dispatch failure
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HookInput;
    use serde_json::json;

    fn make_skill(skill: &str, args: &str) -> HookInput {
        HookInput {
            tool_name: Some("Skill".to_string()),
            tool_input: Some(json!({"skill": skill, "args": args})),
            tool_response: None,
            session_id: Some("test".to_string()),
            cwd: Some(format!("{}/architect", chorus_root())),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("wren".to_string()),
            chorus_worktree_override: None,
        }
    }

    #[tokio::test]
    async fn allows_non_demo_skills() {
        let input = make_skill("acp", "2864");
        let r = check(&input).await;
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn allows_demo_with_no_card_id() {
        let input = make_skill("demo", "");
        let r = check(&input).await;
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn dispatches_for_demo_skill_with_card() {
        // Smoke test only — actual spine query is integration territory.
        let input = make_skill("demo", "2864");
        let r = check(&input).await;
        // PostToolUse never blocks; allow regardless of script outcome.
        assert_eq!(r.exit_code, 0);
    }
}
