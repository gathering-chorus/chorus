//! Accept Gate (#1657, #1671)
//!
//! PreToolUse hook on Skill tool when skill="acp".
//! Validates: demo brief exists, prevents self-accept on code cards.
//! Jeff always overrides (DEC-048).

use crate::types::{HookInput, HookResponse, permission_deny_json, Role};
use std::process::Command;
use tracing::{info, warn};

const BOARD_TS: &str = "/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards";
const BRIEFS_DIR: &str = "/Users/jeffbridwell/CascadeProjects/chorus/product-manager/briefs";

/// Check if this is an /acp invocation and validate acceptance gates
pub async fn check(input: &HookInput) -> HookResponse {
    let skill = input.get_tool_input_str("skill");
    if skill != "acp" {
        return HookResponse::allow();
    }

    let args = input.get_tool_input_str("args");
    let card_id = extract_card_id(&args);
    let role = input.role();

    if card_id.is_empty() {
        return HookResponse::allow();
    }

    info!(card = %card_id, role = role.as_str(), "accept-gate: validating");

    // Gate 1: Demo brief must exist (unless Jeff is accepting)
    if !demo_brief_exists(&card_id) {
        // Jeff can always accept — he's seen it live
        if role == Role::Unknown {
            // Unknown role = Jeff running from root dir, OR a misconfigured session-start.
            // Either way, safer to allow than block — this is a safety valve, not just a Jeff path.
            info!("accept-gate: no demo brief but unknown role — allowing (may be Jeff)");
        } else {
            let msg = format!(
                "Accept blocked: no demo brief found for #{}. Run /demo {} first.",
                card_id, card_id
            );
            warn!("{}", msg);
            return HookResponse::deny(&permission_deny_json(&msg));
        }
    }

    // Gate 2: Self-accept check on code cards
    // Fetch card once — reuse for owner + code-card check (Kade feedback: avoid double subprocess)
    let card_view = fetch_card_view(&card_id);
    let card_owner = parse_owner(&card_view);
    if !card_owner.is_empty() && card_owner == role.as_str() {
        // Opt-OUT model (Wren feedback): default to code card, only exempt chunk:strategy.
        // Missing tags block rather than allow — safer than opt-in.
        if !is_exempt_card(&card_view) {
            let msg = format!(
                "Accept blocked: {} cannot self-accept code card #{} (DEC-048). Ask Jeff or Wren to /acp.",
                role.as_str(), card_id
            );
            warn!("{}", msg);
            return HookResponse::deny(&permission_deny_json(&msg));
        }
    }

    info!(card = %card_id, "accept-gate: all gates passed");
    HookResponse::allow()
}

fn extract_card_id(args: &str) -> String {
    args.split_whitespace()
        .find(|s| s.chars().all(|c| c.is_ascii_digit()))
        .unwrap_or("")
        .to_string()
}

fn demo_brief_exists(card_id: &str) -> bool {
    let pattern = format!("*demo*{}*", card_id);
    let output = Command::new("find")
        .args([BRIEFS_DIR, "-name", &pattern, "-maxdepth", "1"])
        .output();

    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            !stdout.trim().is_empty()
        }
        Err(_) => true, // Don't block on find failure
    }
}

/// Fetch card view once — reused for owner + code-card classification
fn fetch_card_view(card_id: &str) -> String {
    let output = Command::new("bash")
        .args([BOARD_TS, "view", card_id])
        .output();

    match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => String::new(),
    }
}

fn parse_owner(card_view: &str) -> String {
    card_view
        .lines()
        .find(|l| l.contains("Owner:"))
        .and_then(|l| l.split("Owner:").nth(1))
        .map(|s| s.trim().to_lowercase())
        .unwrap_or_default()
}

/// Opt-OUT model: cards are assumed to be code unless explicitly exempt.
/// Only chunk:strategy (pure docs/decisions) is exempt from self-accept block.
/// This means an untagged card is treated as code — missing tags block, not allow.
fn is_exempt_card(card_view: &str) -> bool {
    let lower = card_view.to_lowercase();
    let domains_line = lower
        .lines()
        .find(|l| l.contains("domains:"))
        .unwrap_or("");
    // Exempt: chunk:strategy only. Everything else (including untagged) is code.
    domains_line.contains("chunk:strategy") && !["chunk:ops", "chunk:app", "chunk:memory",
        "chunk:spine", "chunk:music", "chunk:senses", "chunk:sexuality", "chunk:convergence"]
        .iter()
        .any(|c| domains_line.contains(c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HookInput;
    use serde_json::json;

    fn make_skill(skill: &str, role_dir: &str) -> HookInput {
        HookInput {
            tool_name: Some("Skill".to_string()),
            tool_input: Some(json!({"skill": skill})),
            tool_response: None,
            session_id: Some("test".to_string()),
            cwd: Some(format!("/Users/jeffbridwell/CascadeProjects/{}", role_dir)),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: None,
        }
    }

    #[tokio::test]
    async fn allows_non_acp_skills() {
        let input = make_skill("demo", "architect");
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
            cwd: Some("/Users/jeffbridwell/CascadeProjects/architect".to_string()),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("silas".to_string()),
        };
        let r = check(&input).await;
        assert_eq!(r.exit_code, 0);
    }
}
