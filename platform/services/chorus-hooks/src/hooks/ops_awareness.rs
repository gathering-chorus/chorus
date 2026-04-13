//! Ops awareness — surface system state on PostToolUse (#2003 AC3)
//!
//! Checks: hook server health, API health, alert status, seeds pipeline.
//! Surfaces degraded state via stderr — not blocking, surfacing.
//! Cooldown: checks at most once per 60 seconds to avoid noise.

use crate::types::{HookInput, HookResponse};
use std::sync::atomic::{AtomicI64, Ordering};
use tracing::info;

/// Last check timestamp (unix seconds) — cooldown prevents spam
static LAST_CHECK: AtomicI64 = AtomicI64::new(0);
const COOLDOWN_SECS: i64 = 60;

/// Check ops state and surface any issues
pub async fn check(input: &HookInput) -> HookResponse {
    // Only check on Bash and Read
    let tool = input.tool_name_str();
    if tool != "Bash" && tool != "Read" {
        return HookResponse::allow();
    }

    // Cooldown — at most once per 60 seconds
    let now = chrono::Utc::now().timestamp();
    let last = LAST_CHECK.load(Ordering::Relaxed);
    if now - last < COOLDOWN_SECS {
        return HookResponse::allow();
    }
    LAST_CHECK.store(now, Ordering::Relaxed);

    let mut issues: Vec<String> = Vec::new();

    // 1. Hook server — check PID via launchctl (fast, no network)
    let hooks_pid = std::process::Command::new("launchctl")
        .args(["list", "com.chorus.hooks"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    if !hooks_pid.contains("\"PID\"") {
        issues.push("Hook server (com.chorus.hooks) has no PID — may be crashed".to_string());
    }

    // 2. API health — run blocking ureq off the tokio thread pool (#1981)
    // ureq is synchronous; running it inline starves the async runtime under concurrent
    // PostToolUse calls, causing spurious timeouts on a 120ms API.
    let api_result = tokio::task::spawn_blocking(|| {
        ureq::get("http://localhost:3340/api/chorus/health")
            .timeout(std::time::Duration::from_millis(1000))
            .call()
            .ok()
            .and_then(|resp| resp.into_json::<serde_json::Value>().ok())
    }).await;

    match api_result {
        Ok(Some(body)) => {
            let status = body.get("status").and_then(|v| v.as_str()).unwrap_or("unknown");
            if status != "healthy" {
                issues.push(format!("Chorus API status: {}", status));
            }
        }
        _ => {
            issues.push("Chorus API unreachable at localhost:3340".to_string());
        }
    }

    // 3. Alert cooldown files — check if any alert fired today
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let alert_patterns = [
        (format!("/tmp/alert-daily-review-{}", today), "Daily review alert fired"),
        (format!("/tmp/alert-hook-server-{}-", today), "Hook server alert fired"),
        (format!("/tmp/alert-tunnel-{}-", today), "Tunnel alert fired — seeds may not arrive"),
    ];
    for (pattern, desc) in &alert_patterns {
        if std::path::Path::new(pattern).exists() {
            issues.push(desc.to_string());
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

    // Surface, don't block — health state is advisory, not a gate (#1981)
    HookResponse::warn_stderr(&msg)
}
