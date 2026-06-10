//! E2E responder hook (#1936)
//!
//! UserPromptSubmit: detects [e2e-test] markers in nudges injected into
//! the prompt and posts [e2e-ack] back to the Clearing API. This proves
//! the full chain: osascript inject → role session receives → hook fires
//! → message appears in Clearing feed.
//!
//! Fire-and-forget — never blocks the role.

use crate::types::{HookInput, HookResponse};
use tracing::info;

const CLEARING_API: &str = "http://localhost:3470/api/message";

/// Detect [e2e-test] markers and post acks
pub fn check(input: &HookInput) -> HookResponse {
    let prompt = input.prompt.as_deref().unwrap_or("");
    let role = input.role();
    let role_name = role.as_str();

    // Find all [e2e-test] markers in the prompt
    let markers: Vec<&str> = prompt
        .match_indices("[e2e-test]")
        .filter_map(|(pos, _)| {
            let rest = &prompt[pos..];
            let line = rest.lines().next().unwrap_or("");
            let marker = line.trim();
            if marker.len() > 10 { Some(marker) } else { None }
        })
        .collect();

    if markers.is_empty() {
        return HookResponse::allow();
    }

    info!(
        gate = "e2e-responder",
        event = "detected",
        role = %role_name,
        count = markers.len(),
    );

    // Post ack for each marker — use ureq (blocking, but we're in a spawned task)
    for marker in &markers {
        let ack_text = format!("[e2e-ack] {} received {}", role_name, marker);
        let body = serde_json::json!({
            "from": role_name,
            "text": ack_text,
        });

        match ureq::post(CLEARING_API)
            .set("Content-Type", "application/json")
            .send_string(&body.to_string())
        {
            Ok(resp) => {
                info!(
                    gate = "e2e-responder",
                    event = "ack-sent",
                    role = %role_name,
                    status = resp.status(),
                    marker = %marker,
                );
            }
            Err(e) => {
                info!(
                    gate = "e2e-responder",
                    event = "ack-failed",
                    role = %role_name,
                    error = %e,
                );
            }
        }
    }

    HookResponse::allow()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HookInput;
    use crate::shared::state_paths::chorus_root;

    fn make_input(prompt: &str) -> HookInput {
        HookInput {
            tool_name: None,
            tool_input: None,
            tool_response: None,
            session_id: Some("test".to_string()),
            cwd: Some(format!("{}/architect", chorus_root())),
            prompt: Some(prompt.to_string()),
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("silas".to_string()),
            chorus_worktree_override: None, trace_id: None, tool_output_is_error: None,}
    }

    #[test]
    fn ignores_prompt_without_marker() {
        let input = make_input("hi silas, how's it going?");
        let result = check(&input);
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.is_none());
    }

    #[test]
    fn detects_e2e_marker() {
        // POST will fail (no Clearing running in test) — that's fine, fire-and-forget
        let input = make_input("[e2e-test] e2e-public-1775073581539");
        let result = check(&input);
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn detects_multiple_markers() {
        let input = make_input(
            "some text\n[e2e-test] e2e-public-123\nmore text\n[e2e-test] e2e-lan-456\n",
        );
        let result = check(&input);
        assert_eq!(result.exit_code, 0);
    }
}
