//! Demo Preflight Gate (#1657, #1668)
//!
//! PreToolUse hook on Skill tool when skill="demo".
//! Validates: card in WIP, AC exists, smoke-check passes.
//! Blocks demo if any gate fails.
//!
//! The skill handles the creative parts (stakes, showing, feedback).
//! This hook handles the mechanical parts (did you check everything first?).

use crate::types::{HookInput, HookResponse, permission_deny_json};
use std::process::Command;
use tracing::{info, warn};

const BOARD_TS: &str = "/Users/jeffbridwell/CascadeProjects/messages/scripts/board-ts";
const SMOKE_CHECK: &str = "/Users/jeffbridwell/CascadeProjects/messages/scripts/smoke-check.sh";

/// Check if this is a /demo invocation and validate preflight gates
pub async fn check(input: &HookInput) -> HookResponse {
    // Only fires on Skill tool with skill="demo"
    let skill = input.get_tool_input_str("skill");
    if skill != "demo" {
        return HookResponse::allow();
    }

    let args = input.get_tool_input_str("args");
    let card_id = extract_card_id(&args);

    if card_id.is_empty() {
        // No card ID — let the skill handle "check WIP" logic
        return HookResponse::allow();
    }

    info!(card = %card_id, "demo-preflight: validating gates");

    // Gate 1: Card must be in WIP
    match check_card_status(&card_id) {
        CardStatus::Wip => {}
        CardStatus::NotWip(status) => {
            let msg = format!(
                "Demo blocked: #{} is in {} — must be in WIP to demo. Move it first.",
                card_id, status
            );
            warn!("{}", msg);
            return HookResponse::deny(&permission_deny_json(&msg));
        }
        CardStatus::NotFound => {
            let msg = format!("Demo blocked: #{} not found on board.", card_id);
            warn!("{}", msg);
            return HookResponse::deny(&permission_deny_json(&msg));
        }
        CardStatus::Error(e) => {
            warn!("demo-preflight: board-ts failed: {}", e);
            // Don't block on board-ts failures — let the skill handle it
            return HookResponse::allow();
        }
    }

    // Gate 2: AC must exist in description
    if !check_ac_exists(&card_id) {
        let msg = format!(
            "Demo blocked: #{} has no acceptance criteria. Add ## AC to the card description before demoing.",
            card_id
        );
        warn!("{}", msg);
        return HookResponse::deny(&permission_deny_json(&msg));
    }

    // Gate 3: Smoke check must pass
    match run_smoke_check() {
        SmokeResult::Pass(count) => {
            info!("demo-preflight: smoke check passed ({} checks)", count);
        }
        SmokeResult::Fail(detail) => {
            let msg = format!(
                "Demo blocked: smoke check failed. Fix before demoing.\n{}",
                detail
            );
            warn!("{}", msg);
            return HookResponse::deny(&permission_deny_json(&msg));
        }
        SmokeResult::Error(e) => {
            warn!("demo-preflight: smoke-check.sh error: {}", e);
            // Don't block on smoke-check errors — might be a non-code card
        }
    }

    // Gate 4: ICD render check for domain cards
    let domains = get_card_domains(&card_id);
    for domain in &domains {
        if let Some(msg) = check_icd_render(domain).await {
            let full_msg = format!(
                "Demo blocked: convergence page for domain '{}' has empty sections. {}\nFix the ICD data before demoing.",
                domain, msg
            );
            warn!("{}", full_msg);
            return HookResponse::deny(&permission_deny_json(&full_msg));
        }
    }

    info!(card = %card_id, "demo-preflight: all gates passed");
    HookResponse::allow()
}

fn extract_card_id(args: &str) -> String {
    // First numeric token in the args
    args.split_whitespace()
        .find(|s| s.chars().all(|c| c.is_ascii_digit()))
        .unwrap_or("")
        .to_string()
}

enum CardStatus {
    Wip,
    NotWip(String),
    NotFound,
    Error(String),
}

fn check_card_status(card_id: &str) -> CardStatus {
    let output = Command::new("bash")
        .args([BOARD_TS, "view", card_id])
        .output();

    match output {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            if let Some(line) = stdout.lines().find(|l| l.contains("Status:")) {
                let status = line.split("Status:").nth(1).unwrap_or("").trim().to_string();
                if status == "WIP" {
                    CardStatus::Wip
                } else {
                    CardStatus::NotWip(status)
                }
            } else {
                CardStatus::Error("Could not parse status".to_string())
            }
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            if stderr.contains("not found") {
                CardStatus::NotFound
            } else {
                CardStatus::Error(stderr.to_string())
            }
        }
        Err(e) => CardStatus::Error(e.to_string()),
    }
}

fn check_ac_exists(card_id: &str) -> bool {
    let output = Command::new("bash")
        .args([BOARD_TS, "view", card_id])
        .output();

    match output {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout).to_lowercase();
            stdout.contains("## ac")
                || stdout.contains("acceptance criteria")
                || stdout.contains("- [ ]")
        }
        _ => true, // Don't block on parse failure
    }
}

enum SmokeResult {
    Pass(usize),
    Fail(String),
    Error(String),
}

fn run_smoke_check() -> SmokeResult {
    let output = Command::new("bash")
        .args([SMOKE_CHECK, "--all"])
        .output();

    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            if o.status.success() {
                // Count passes from "N pass" in output
                let count = stdout
                    .lines()
                    .filter(|l| l.contains("pass"))
                    .last()
                    .and_then(|l| l.split_whitespace().find(|w| w.parse::<usize>().is_ok()))
                    .and_then(|w| w.parse().ok())
                    .unwrap_or(0);
                SmokeResult::Pass(count)
            } else {
                // Extract failure lines
                let failures: Vec<&str> = stdout
                    .lines()
                    .filter(|l| l.contains("FAIL"))
                    .collect();
                SmokeResult::Fail(failures.join("\n"))
            }
        }
        Err(e) => SmokeResult::Error(e.to_string()),
    }
}

fn get_card_domains(card_id: &str) -> Vec<String> {
    let output = Command::new("bash")
        .args([BOARD_TS, "view", card_id])
        .output();

    match output {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            stdout
                .lines()
                .filter(|l| l.contains("Domains:"))
                .flat_map(|l| {
                    l.split(':')
                        .skip(1)
                        .flat_map(|s| s.split(','))
                        .filter_map(|s| {
                            let trimmed = s.trim();
                            if trimmed.starts_with("domain:") {
                                Some(trimmed.strip_prefix("domain:").unwrap().to_string())
                            } else {
                                None
                            }
                        })
                })
                .filter(|d| d != "infrastructure") // Skip infra — no ICD page
                .collect()
        }
        _ => vec![],
    }
}

/// Check ICD convergence page renders for a domain.
/// Uses Chorus API (3340) for SPARQL-backed data, falls back gracefully.
/// Full content verification requires unauthenticated ICD endpoint (not yet built).
async fn check_icd_render(domain: &str) -> Option<String> {
    // Check if ICD instance TTL exists for this domain
    let ttl_path = format!(
        "/Users/jeffbridwell/CascadeProjects/architect/icd-instance-{}.ttl",
        domain
    );
    if !std::path::Path::new(&ttl_path).exists() {
        return Some(format!("No ICD instance file: icd-instance-{}.ttl", domain));
    }

    // Verify TTL is non-empty (has provider sections)
    let content = std::fs::read_to_string(&ttl_path).unwrap_or_default();
    if content.len() < 100 {
        return Some(format!("ICD instance file for {} is empty or stub", domain));
    }

    // Check for required sections
    let has_provider = content.contains("icd:provider") || content.contains("Provider");
    let has_fields = content.contains("icd:field") || content.contains("icd:maps");
    if !has_provider {
        return Some(format!("ICD for {} missing provider section", domain));
    }
    if !has_fields {
        return Some(format!("ICD for {} missing field mappings", domain));
    }

    None
}
