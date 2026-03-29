//! Pair Enforcement (#1673)
//!
//! When /pair starts, ensure the target role also loads the /pair skill.
//! A nudge summary is not sufficient — both sides need the full protocol.

use crate::types::{HookInput, HookResponse};
use std::process::Command;
use tracing::info;

const NUDGE: &str = "/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/nudge.sh";

/// PreToolUse: when /pair skill is invoked, nudge the target role to also load /pair
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

    let _ = Command::new(NUDGE)
        .args([target, &nudge_msg])
        .output();

    HookResponse::warn_stderr(&format!(
        "Pair enforcement: nudged {} to load /pair skill",
        target
    ))
}
