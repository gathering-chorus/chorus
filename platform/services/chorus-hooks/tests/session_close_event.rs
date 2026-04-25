//! #1848 — Session close must emit session.role.ended with duration
//! #2071 — Stale nudge warning via alert rule

use std::fs;
use std::process::Command;
use chorus_hooks::shared::state_paths::chorus_root;

const SHIM: &str = env!("CARGO_BIN_EXE_chorus-hook-shim");
fn chorus_log() -> String { format!("{}/platform/logs/chorus.log", chorus_root()) }
fn log_tail(n: usize) -> String {
    let content = fs::read_to_string(chorus_log()).unwrap_or_default();
    content.lines().rev().take(n).collect::<Vec<_>>().join("\n")
}

fn log_tail_after_marker(marker: &str) -> String {
    let content = fs::read_to_string(chorus_log()).unwrap_or_default();
    let lines: Vec<&str> = content.lines().collect();
    // Find last occurrence of marker, return everything after it
    if let Some(pos) = lines.iter().rposition(|l| l.contains(marker)) {
        lines[pos..].join("\n")
    } else {
        String::new()
    }
}

// === #1848: session.role.ended ===

#[test]
fn session_close_emits_role_ended_with_duration() {
    // First start a session so we have a timestamp
    let _ = Command::new(SHIM)
        .args(["session-start", "kade"])
        .output();

    // Then close it
    let output = Command::new(SHIM)
        .args(["session-close", "kade"])
        .output()
        .expect("session-close should run");

    let _stdout = String::from_utf8_lossy(&output.stdout);
    // Look for session.role.ended after the close.started event
    let log = log_tail_after_marker("protocol.close.started");

    assert!(
        log.contains("session.role.ended"),
        "should emit session.role.ended on close, log after close.started: {}",
        log
    );
    assert!(
        log.contains("duration"),
        "session.role.ended should include duration, log: {}",
        log
    );
}

// === #2071: nudge.stale alert rule ===

#[test]
fn nudge_stale_alert_rule_exists() {
    let rule_path = &format!("{}/proving/domains/alerts/nudge-stale.yml", chorus_root());
    assert!(
        std::path::Path::new(rule_path).exists(),
        "nudge-stale.yml alert rule must exist at {}",
        rule_path
    );
}
