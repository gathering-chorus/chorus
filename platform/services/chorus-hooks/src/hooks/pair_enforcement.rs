//! Pair Enforcement (#1673)
//!
//! When /pair starts, ensure the target role also loads the /pair skill.
//! A nudge summary is not sufficient — both sides need the full protocol.

use crate::shared::state_paths::chorus_root;
use crate::types::{HookInput, HookResponse};
use std::process::Command;
use tracing::info;

/// #2435 wedge 6 — pair-enforcement extracts to its own CLI (`pair-enforce`).
/// Previously called `nudge.sh` (which never existed — the call has been silently
/// no-op-ing). In-band control signals ("force target to load /pair now") are a
/// different semantic than coordination chat; conflating them in one CLI made
/// the delivery contract unstable. The new primitive has its own event types
/// (pair.enforce.emitted/delivered/failed), its own focus-gate, and zero shared
/// code with the nudge path.
fn pair_enforce_script() -> String {
    format!("{}/platform/scripts/pair-enforce", chorus_root())
}

/// PreToolUse: when /pair skill is invoked, signal the target role to also load /pair
pub async fn check(input: &HookInput) -> HookResponse {
    let skill = input.get_tool_input_str("skill");
    if skill != "pair" {
        return HookResponse::allow();
    }

    let args = input.get_tool_input_str("args");
    let role = input.role();

    // Extract target role from args (e.g., "/pair kade" or "/pair wren on #1665")
    let target = args.split_whitespace()
        .find(|w| matches!(*w, "wren" | "silas" | "kade"))
        .unwrap_or("");

    if target.is_empty() || target == role.as_str() {
        return HookResponse::allow();
    }

    info!(from = role.as_str(), target = target, "pair-enforcement: nudging target to load /pair");

    // Nudge the target role with the /pair skill prefix so the full protocol loads
    let card = args.split_whitespace()
        .find(|w| w.starts_with('#'))
        .map(|w| w.trim_start_matches('#'))
        .unwrap_or("");

    let nudge_msg = if card.is_empty() {
        format!("/pair {} — {} initiated pairing. Load /pair to get the full navigator/driver protocol.", role.as_str(), role.as_str())
    } else {
        format!("/pair {} on #{} — {} initiated pairing. Load /pair to get the full navigator/driver protocol.", role.as_str(), card, role.as_str())
    };

    let ns = pair_enforce_script();
    let _ = Command::new(&ns)
        .args([target, &nudge_msg])
        .env("DEPLOY_ROLE", role.as_str())
        .output();

    HookResponse::warn_stderr(&format!(
        "Pair enforcement: signalled {} to load /pair skill",
        target
    ))
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
            chorus_worktree_override: None, trace_id: None, tool_output_is_error: None,}
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

    /// #2435 wedge 6 — pair-enforcement extracts to its own primitive.
    /// The script path MUST be pair-enforce (not nudge / nudge.sh). Enforces
    /// zero-shared-code between the two semantics: coordination chat (nudge)
    /// and in-band control (pair.enforce) stop sharing a delivery CLI.
    #[test]
    fn uses_pair_enforce_script_not_nudge() {
        let path = pair_enforce_script();
        assert!(
            path.ends_with("/platform/scripts/pair-enforce"),
            "pair_enforcement must invoke pair-enforce, not nudge. Got: {}", path
        );
        assert!(
            !path.contains("nudge"),
            "pair_enforcement must not share a CLI with nudge. Got: {}", path
        );
    }

    /// The extracted script must exist on disk — shipping a dangling reference
    /// is the bug we're fixing (nudge.sh didn't exist, pair_enforcement has been
    /// silently no-op-ing for some time).
    #[test]
    fn pair_enforce_script_exists_on_disk() {
        let path = pair_enforce_script();
        assert!(
            std::path::Path::new(&path).exists(),
            "pair-enforce script must exist at {} (the whole point of the extraction)", path
        );
    }
}
