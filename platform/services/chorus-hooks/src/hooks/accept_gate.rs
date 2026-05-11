//! Accept Gate (#1657, #1671, #2864)
//!
//! PreToolUse hook on Skill tool when skill="acp".
//! Validates: demo evidence exists, prevents self-accept on code cards.
//! Jeff always overrides (DEC-048).
//!
//! Evolution of demo evidence:
//! - Original: brief-file artifact (retired by #2177)
//! - #2177: `demo:preflight-pass` card comment substring
//! - #2864: `demo.show.completed` spine event for the card_id (proves Jeff
//!   watched, not just that the agent posted a comment). Comment retained
//!   as transitional fallback until comment-shape decommission.

use crate::shared::state_paths::chorus_root;
use crate::types::{HookInput, HookResponse, permission_deny_json, Role};
use std::process::Command;
use tracing::{info, warn};

fn board_ts() -> String { format!("{}/platform/scripts/cards", chorus_root()) }

/// #2897: Read the /demo trace_id for a card (written by demo_preflight.rs on
/// /demo entry) so dispatched gate scripts inherit it as CHORUS_TRACE_ID env.
/// Returns empty string when no trace file exists — chorus_log.rs's fallback
/// (read from /tmp/demo-trace-${card}.txt) still works in that case.
fn read_demo_trace(card_id: &str) -> String {
    let path = format!("/tmp/demo-trace-{}.txt", card_id);
    std::fs::read_to_string(&path)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// #2864: Show-gate dispatch — runs skills/demo/gates/show-gate.sh which
/// queries Loki for the demo chain (card.demo.started + jeff.input.delivered
/// window + demo.preflight.passed) and emits demo.show.completed/failed.
/// Returns true if the gate script exits 0 (all preconditions present).
pub fn demo_show_passes(card_id: &str, role: &str) -> bool {
    let script = format!("{}/skills/demo/gates/show-gate.sh", chorus_root());
    let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/jeffbridwell".to_string());
    let output = Command::new("bash")
        .args(["-l", &script, card_id, role])
        .env("CHORUS_ROOT", chorus_root())
        .env("HOME", &home)
        .env(
            "PATH",
            format!(
                "{}/CascadeProjects/chorus/platform/scripts:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin",
                home
            ),
        )
        .env("CHORUS_TRACE_ID", read_demo_trace(card_id))
        .output();

    match output {
        Ok(o) => o.status.success(),
        Err(e) => {
            warn!(card = %card_id, "demo-show: dispatch failed (Command::output Err) — {}; defaulting to pass to avoid blocking on infra failure", e);
            true
        }
    }
}

/// #2177: Demo evidence comment on card. Retained as transitional fallback
/// per #2864 until comment-shape is decommissioned.
pub fn demo_evidence_exists(card_view: &str) -> bool {
    card_view.contains("demo:preflight-pass")
}

/// #2893: Chain-orchestration dispatch — runs skills/demo/gates/chain-orchestration.sh
/// which enforces Step 2's gate chain. For type:enhance / new / fix: requires
/// all 5 gate-pass comments. For type:chore / swat: allows the skip only if
/// blast radius is small (<= 3 files AND <= 200 lines vs origin/main); refuses
/// as mistagged otherwise. Returns (passed, stderr_message).
pub fn demo_chain_passes(card_id: &str, role: &str) -> (bool, String) {
    let script = format!("{}/skills/demo/gates/chain-orchestration.sh", chorus_root());
    let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/jeffbridwell".to_string());
    let output = Command::new("bash")
        .args(["-l", &script, card_id, role])
        .env("CHORUS_ROOT", chorus_root())
        .env("HOME", &home)
        .env(
            "PATH",
            format!(
                "{}/CascadeProjects/chorus/platform/scripts:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin",
                home
            ),
        )
        .env("CHORUS_TRACE_ID", read_demo_trace(card_id))
        .output();

    match output {
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
            (o.status.success(), stderr)
        }
        Err(_) => (true, String::new()),
    }
}

/// #2893: Happy-path dispatch — runs skills/demo/gates/happy-path.sh which
/// parses the card's AC for checkable URL/file/event references and runs
/// derived checks. Returns (passed, stderr_message). Skipped (passed=true)
/// when no checkable references are found — abstract AC is not penalized.
pub fn demo_happy_path_passes(card_id: &str, role: &str) -> (bool, String) {
    let script = format!("{}/skills/demo/gates/happy-path.sh", chorus_root());
    let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/jeffbridwell".to_string());
    let output = Command::new("bash")
        .args(["-l", &script, card_id, role])
        .env("CHORUS_ROOT", chorus_root())
        .env("HOME", &home)
        .env(
            "PATH",
            format!(
                "{}/CascadeProjects/chorus/platform/scripts:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin",
                home
            ),
        )
        .env("CHORUS_TRACE_ID", read_demo_trace(card_id))
        .output();

    match output {
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
            (o.status.success(), stderr)
        }
        Err(_) => (true, String::new()),
    }
}

/// #2893: Stakes-brief lint dispatch — runs skills/demo/gates/stakes-brief-lint.sh
/// which reads the brief from Bridge and lints for "Why this matters" presence +
/// absence of mechanics-first openers. Emits demo.stakes.passed/failed.
/// Invoked at /acp time (PostToolUse on /demo fires before Step 5c POSTs the
/// brief to Bridge, same reason show-gate moved here per #2864).
/// Returns (passed, stderr_message).
pub fn demo_stakes_passes(card_id: &str, role: &str) -> (bool, String) {
    let script = format!("{}/skills/demo/gates/stakes-brief-lint.sh", chorus_root());
    let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/jeffbridwell".to_string());
    let output = Command::new("bash")
        .args(["-l", &script, card_id, role])
        .env("CHORUS_ROOT", chorus_root())
        .env("HOME", &home)
        .env(
            "PATH",
            format!(
                "{}/CascadeProjects/chorus/platform/scripts:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin",
                home
            ),
        )
        .env("CHORUS_TRACE_ID", read_demo_trace(card_id))
        .output();

    match output {
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
            (o.status.success(), stderr)
        }
        Err(e) => {
            warn!(card = %card_id, "demo-stakes-lint: dispatch failed (Command::output Err) — {}; defaulting to pass to avoid blocking on infra failure", e);
            (true, String::new())
        }
    }
}

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

    // Fetch card view once — reused for evidence + owner + code-card check.
    let card_view = fetch_card_view(&card_id);

    // Gate 1: Demo evidence (#2864).
    // Primary: show-gate.sh queries spine for the demo chain (card.demo.started +
    // jeff.input.delivered window + demo.preflight.passed). Exit 0 = chain present.
    // Fallback: `demo:preflight-pass` card comment (transitional until decommission).
    let has_show_pass = demo_show_passes(&card_id, role.as_str());
    let has_comment = demo_evidence_exists(&card_view);
    if !has_show_pass && !has_comment {
        if role == Role::Unknown {
            // Unknown role = Jeff running from root dir, OR a misconfigured session-start.
            // Safety valve: allow rather than block.
            info!("accept-gate: show-gate failed AND no demo:preflight-pass comment — but unknown role, allowing (may be Jeff)");
        } else {
            let msg = format!(
                "Accept blocked: no demo evidence for #{}. show-gate.sh did not pass (no card.demo.started + jeff.input.delivered window + demo.preflight.passed for this card) AND no demo:preflight-pass comment. Run /demo {} first.",
                card_id, card_id
            );
            warn!("{}", msg);
            return HookResponse::deny(&permission_deny_json(&msg));
        }
    } else if has_show_pass {
        info!(card = %card_id, "accept-gate: show-gate passed (full demo chain on spine, Jeff watched)");
    } else {
        info!(card = %card_id, "accept-gate: only comment evidence — show chain incomplete (transitional fallback)");
    }

    // Gate 1b: Stakes-brief lint (#2893). Refuses /acp if the demo brief lacked
    // a "Why this matters" line or opened with a mechanics-first anti-pattern.
    // Skipped when only comment-fallback is available (no show-pass) — the brief
    // is queried from Bridge which requires a real demo run.
    if has_show_pass && role != Role::Unknown {
        let (stakes_pass, stakes_stderr) = demo_stakes_passes(&card_id, role.as_str());
        if !stakes_pass {
            let msg = format!(
                "Accept blocked: stakes-brief lint failed for #{}. {} Rewrite the demo brief — must contain 'Why this matters' and must not open with mechanics-first phrasing ('I built', 'The API now', 'Here's what changed', etc.).",
                card_id, stakes_stderr
            );
            warn!("{}", msg);
            return HookResponse::deny(&permission_deny_json(&msg));
        }
        info!(card = %card_id, "accept-gate: stakes-brief lint passed");
    }

    // Gate 1c: AC happy-path check (#2893). Parses AC for checkable URL / file /
    // spine-event references and runs derived checks. Refuses /acp on any failure;
    // skipped on cards with only abstract AC (so the gate doesn't false-fail).
    if role != Role::Unknown {
        let (hp_pass, hp_stderr) = demo_happy_path_passes(&card_id, role.as_str());
        if !hp_pass {
            let msg = format!(
                "Accept blocked: AC happy-path check failed for #{}. {} Fix the failing AC reference or correct the AC text.",
                card_id, hp_stderr
            );
            warn!("{}", msg);
            return HookResponse::deny(&permission_deny_json(&msg));
        }
        info!(card = %card_id, "accept-gate: happy-path passed (or skipped on abstract AC)");
    }

    // Gate 1d: Chain-orchestration (#2893). Step 2's gate chain was prose-only;
    // the model self-orchestrated and silent type:chore bypass was the failure
    // mode #2893 itself surfaced. Now enforced: non-chore cards require all 5
    // gate-pass comments; chore/swat cards require small blast radius or are
    // refused as mistagged.
    if role != Role::Unknown {
        let (chain_pass, chain_stderr) = demo_chain_passes(&card_id, role.as_str());
        if !chain_pass {
            let msg = format!(
                "Accept blocked: gate-chain check failed for #{}. {} Either complete the missing gates, or — if this is genuinely small housekeeping — keep the chore/swat tag but reduce the scope.",
                card_id, chain_stderr
            );
            warn!("{}", msg);
            return HookResponse::deny(&permission_deny_json(&msg));
        }
        info!(card = %card_id, "accept-gate: chain-orchestration passed (or honored chore/swat skip)");
    }

    // Gate 2: Self-accept check on code cards (reuse card_view).
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

/// Fetch card view once — reused for evidence + owner + code-card classification
fn fetch_card_view(card_id: &str) -> String {
    let bts = board_ts();
    let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/jeffbridwell".to_string());
    let output = Command::new("bash")
        .args(["-l", bts.as_str(), "view", card_id])
        .env("CHORUS_ROOT", chorus_root())
        .env("HOME", &home)
        .env("PATH", format!("{}/CascadeProjects/chorus/platform/scripts:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin", home))
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
            cwd: Some(format!("{}/{}", chorus_root(), role_dir)),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: None,
            chorus_worktree_override: None,}
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
            cwd: Some(format!("{}/architect", chorus_root())),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("silas".to_string()),
            chorus_worktree_override: None,};
        let r = check(&input).await;
        assert_eq!(r.exit_code, 0);
    }
}
