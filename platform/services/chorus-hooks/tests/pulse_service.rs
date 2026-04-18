//! Tests for #1881 — Pulse service: structured team state JSON
//! What Jeff sees: one JSON payload with full team state on every prompt cycle.

use std::fs;
use std::path::Path;

/// Pulse output file exists after running the command
#[test]
fn pulse_writes_output_file() {
    let _ = fs::remove_file("/tmp/pulse-latest.json");
    let output = std::process::Command::new(
        env!("CARGO_BIN_EXE_chorus-hook-shim")
    ).arg("pulse").output();

    assert!(output.is_ok(), "pulse command should execute");
    assert!(Path::new("/tmp/pulse-latest.json").exists(), "pulse should write /tmp/pulse-latest.json");
}

/// Pulse JSON has all required top-level keys
#[test]
fn pulse_contains_required_sections() {
    // Run pulse
    let _ = std::process::Command::new(
        env!("CARGO_BIN_EXE_chorus-hook-shim")
    ).arg("pulse").output();

    let content = fs::read_to_string("/tmp/pulse-latest.json")
        .expect("pulse output should be readable");
    let v: serde_json::Value = serde_json::from_str(&content)
        .expect("pulse output should be valid JSON");

    let obj = v.as_object().expect("pulse should be a JSON object");
    assert!(obj.contains_key("timestamp"), "must have timestamp");
    assert!(obj.contains_key("roles"), "must have roles");
    assert!(obj.contains_key("events"), "must have events");
    assert!(obj.contains_key("alerts"), "must have alerts");
    assert!(obj.contains_key("nudges"), "must have nudges");
    assert!(obj.contains_key("health"), "must have health");
    assert!(obj.contains_key("board"), "must have board");
    assert!(obj.contains_key("index_freshness"), "must have index_freshness");
    assert!(obj.contains_key("elapsed_ms"), "must have elapsed_ms");
}

/// Pulse includes all 3 roles
#[test]
fn pulse_has_all_three_roles() {
    let _ = std::process::Command::new(
        env!("CARGO_BIN_EXE_chorus-hook-shim")
    ).arg("pulse").output();

    let content = fs::read_to_string("/tmp/pulse-latest.json").unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();

    let roles = v.get("roles").and_then(|r| r.as_object()).expect("roles should be object");
    assert!(roles.contains_key("wren"), "must have wren");
    assert!(roles.contains_key("silas"), "must have silas");
    assert!(roles.contains_key("kade"), "must have kade");
}

/// Pulse completes under budget.
/// Original budget was 200ms (#1896). Real-world stand-alone is 500-600ms as of
/// 2026-04-17; pulse picked up more sources without budget retuning. Under
/// parallel cargo test load (the nightly sweep runs ~20 test binaries
/// concurrently), pulse competes for CPU and can hit 1000-1500ms.
/// 2000ms here is a regression-catcher under contention, not a perf target —
/// #2158 is the card to drive the stand-alone number back toward 200ms.
#[test]
fn pulse_runs_under_budget() {
    let _ = std::process::Command::new(
        env!("CARGO_BIN_EXE_chorus-hook-shim")
    ).arg("pulse").output();

    let content = fs::read_to_string("/tmp/pulse-latest.json").unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();

    let elapsed = v.get("elapsed_ms").and_then(|e| e.as_u64()).unwrap_or(9999);
    assert!(elapsed < 2000, "pulse must complete in <2000ms under parallel load (#2158 to tighten), got {}ms", elapsed);
}

/// Nudge section shows per-role pending counts
#[test]
fn pulse_nudges_per_role() {
    let _ = std::process::Command::new(
        env!("CARGO_BIN_EXE_chorus-hook-shim")
    ).arg("pulse").output();

    let content = fs::read_to_string("/tmp/pulse-latest.json").unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();

    let nudges = v.get("nudges").and_then(|n| n.as_object()).expect("nudges should be object");
    for role in &["wren", "silas", "kade"] {
        let role_nudge = nudges.get(*role).expect(&format!("{} should have nudge entry", role));
        assert!(role_nudge.get("pending").is_some(), "{} should have pending count", role);
        assert!(role_nudge.get("stale").is_some(), "{} should have stale flag", role);
    }
}

/// Alerts section lists what fired today
#[test]
fn pulse_alerts_fired_today() {
    let _ = std::process::Command::new(
        env!("CARGO_BIN_EXE_chorus-hook-shim")
    ).arg("pulse").output();

    let content = fs::read_to_string("/tmp/pulse-latest.json").unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();

    let alerts = v.get("alerts").expect("must have alerts");
    assert!(alerts.get("fired_today").is_some(), "must have fired_today list");
    assert!(alerts.get("count").is_some(), "must have alert count");
}
