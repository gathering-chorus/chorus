//! Ops awareness — surface system state on PostToolUse (#2003 AC3)
//!
//! Checks: hook server health, API health, alert status, seeds pipeline.
//! Surfaces degraded state via stderr — not blocking, surfacing.
//! Fast: file checks + one HTTP call to localhost.

use crate::types::{HookInput, HookResponse};
use tracing::info;

/// Check ops state and surface any issues
pub async fn check(input: &HookInput) -> HookResponse {
    // Only check every Nth tool call to avoid noise — check on Bash and Read
    let tool = input.tool_name_str();
    if tool != "Bash" && tool != "Read" {
        return HookResponse::allow();
    }

    let mut issues: Vec<String> = Vec::new();

    // 1. Hook server — check PID file / launchctl (fast, no network)
    let hooks_pid = std::process::Command::new("launchctl")
        .args(["list", "com.chorus.hooks"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    if !hooks_pid.contains("\"PID\"") {
        issues.push("Hook server (com.chorus.hooks) has no PID — may be crashed".to_string());
    }

    // 2. API health — one HTTP call (sub-50ms to localhost)
    if let Ok(resp) = ureq::get("http://localhost:3340/api/chorus/health")
        .timeout(std::time::Duration::from_millis(200))
        .call()
    {
        if let Ok(body) = resp.into_json::<serde_json::Value>() {
            let status = body.get("status").and_then(|v| v.as_str()).unwrap_or("unknown");
            if status != "healthy" {
                issues.push(format!("Chorus API status: {}", status));
            }
        }
    } else {
        issues.push("Chorus API unreachable at localhost:3340".to_string());
    }

    // 3. Alert cooldown files — check if any alert fired today
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let alert_patterns = [
        (format!("/tmp/alert-daily-review-{}", today), "Daily review alert fired"),
        (format!("/tmp/alert-hook-server-{}-", today), "Hook server alert fired"),
    ];
    for (pattern, desc) in &alert_patterns {
        // For hour-scoped cooldowns, check any matching file
        if std::path::Path::new(pattern).exists() {
            issues.push(desc.to_string());
        }
    }

    // 4. Seeds pipeline — check via API (fast, localhost)
    if let Ok(resp) = ureq::get("http://localhost:3340/api/chorus/search?q=seed+write+failure&mode=fts&limit=1")
        .timeout(std::time::Duration::from_millis(200))
        .call()
    {
        if let Ok(body) = resp.into_json::<serde_json::Value>() {
            let total = body.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
            if total > 0 {
                // Check if the most recent hit is from today
                if let Some(results) = body.get("results").and_then(|r| r.as_array()) {
                    if let Some(first) = results.first() {
                        let ts = first.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
                        if ts.starts_with(&today) {
                            issues.push("Seed write failure detected today".to_string());
                        }
                    }
                }
            }
        }
    }

    if issues.is_empty() {
        return HookResponse::allow();
    }

    let role_name = format!("{:?}", input.role()).to_lowercase();
    info!(
        gate = "ops-awareness",
        event = "state-surface",
        role = %role_name,
        issues = issues.len(),
    );

    let mut msg = String::from("\n<ops-state>\n");
    for issue in &issues {
        msg.push_str(&format!("⚠ {}\n", issue));
    }
    msg.push_str("</ops-state>");

    // Use block_with_stderr (exit 2) — PostToolUse exit 0 stderr is not surfaced to the role
    HookResponse::block_with_stderr(&msg)
}
