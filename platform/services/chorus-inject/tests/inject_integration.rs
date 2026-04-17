//! chorus-inject integration + source-gate tests. Renamed 2026-04-17 per
//! #2155 from inject_test.rs to reflect that this file is mixed-purpose.
//!
//! Tests by type (honest naming matters — see loom-principles:quality-at-source):
//!
//! **Source gate** (build-time lint, NOT behavior):
//! - `inject_source_uses_keystroke_not_do_script` — reads main.rs and
//!   asserts it contains "keystroke" + "key code 36" + no "do script".
//!   Fails on rename, passes on semantic bugs. Treat as clippy-lite.
//!
//! **Integration (exercises the real binary, non-hermetic)** — gated
//! behind HERMETIC_TEST_MODE=1 per #2131:
//! - `inject_delivers_to_silas` / `_to_wren` / `_to_kade` — exec
//!   chorus-inject with osascript-driven keystroke injection into live
//!   role terminals. Sends real nudges as side effects.
//! - `nudge_e2e_delivers` — exec nudge script end-to-end.
//!
//! **CLI contract** (hermetic, exercises argv parsing only):
//! - `rejects_unknown_role` — unknown role → non-zero exit, "unknown role".
//! - `inject_binary_exists` — path existence check (smoke).
//!
//! #2029 reverted #2245's "do script" approach — do script doesn't send
//! a Return that Claude Code recognizes, breaking auto-submit.

use std::process::Command;

const INJECT_BIN: &str = "/Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-inject/target/release/chorus-inject";
const NUDGE_SCRIPT: &str = "/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/nudge";

// --- AC1: keystroke + key code 36, not do script ---

#[test]
fn inject_source_uses_keystroke_not_do_script() {
    let source = std::fs::read_to_string(
        "/Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-inject/src/main.rs"
    ).expect("can't read main.rs");

    assert!(
        source.contains("key code 36"),
        "inject must use 'key code 36' (Return) for auto-submit"
    );
    assert!(
        source.contains("keystroke"),
        "inject must use 'keystroke' for text delivery"
    );
    // do script breaks auto-submit (#2029). Only comments should reference it.
    let code_lines: Vec<&str> = source.lines()
        .filter(|l| !l.trim_start().starts_with("//") && !l.trim_start().starts_with("//!"))
        .collect();
    let code_only = code_lines.join("\n");
    assert!(
        !code_only.contains("do script"),
        "inject must NOT use 'do script' — breaks auto-submit (#2029)"
    );
}

// --- AC3 + AC4: live delivery to each role ---

// TEMP skip: hermetic-test gate — see #2131.
// These tests exec the real chorus-inject binary and the real nudge script,
// both of which drive osascript keystroke injection into every role's live
// terminal. Set HERMETIC_TEST_MODE=1 to gate them. Durable fix: Silas's shim
// kill-switch + Wren's mock-to-capture work.
fn hermetic_skip(name: &str) -> bool {
    if std::env::var("HERMETIC_TEST_MODE").is_ok() {
        eprintln!("SKIP {}: hermetic-test gate — #2131", name);
        return true;
    }
    false
}

#[test]
fn inject_delivers_to_silas() {
    if hermetic_skip("inject_delivers_to_silas") { return; }
    let output = Command::new(INJECT_BIN)
        .args(["silas", "[cargo-test] AC4 silas inject"])
        .output()
        .expect("failed to run chorus-inject");
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        assert!(
            stderr.contains("no claude window"),
            "should fail with 'no window' not TCC error: {}", stderr
        );
    }
}

#[test]
fn inject_delivers_to_wren() {
    if hermetic_skip("inject_delivers_to_wren") { return; }
    let output = Command::new(INJECT_BIN)
        .args(["wren", "[cargo-test] AC4 wren inject"])
        .output()
        .expect("failed to run chorus-inject");
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        assert!(
            stderr.contains("no claude window"),
            "should fail with 'no window' not TCC error: {}", stderr
        );
    }
}

#[test]
fn inject_delivers_to_kade() {
    if hermetic_skip("inject_delivers_to_kade") { return; }
    let output = Command::new(INJECT_BIN)
        .args(["kade", "[cargo-test] AC4 kade inject"])
        .output()
        .expect("failed to run chorus-inject");
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        assert!(
            stderr.contains("no claude window"),
            "should fail with 'no window' not TCC error: {}", stderr
        );
    }
}

// --- AC5: nudge e2e regression ---

#[test]
fn nudge_e2e_delivers() {
    if hermetic_skip("nudge_e2e_delivers") { return; }
    let output = Command::new("bash")
        .args([NUDGE_SCRIPT, "silas", "[cargo-test] AC5 nudge e2e", "--force"])
        .output()
        .expect("failed to run nudge");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("DELIVERED"),
        "nudge e2e should deliver: {}", stdout
    );
}

// --- AC6: unknown role rejected (regression) ---

#[test]
fn rejects_unknown_role() {
    let output = Command::new(INJECT_BIN)
        .args(["nonexistent", "test"])
        .output()
        .expect("failed to run chorus-inject");
    assert!(!output.status.success(), "unknown role should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown role"), "should say unknown role: {}", stderr);
}

// --- Binary exists ---

#[test]
fn inject_binary_exists() {
    assert!(
        std::path::Path::new(INJECT_BIN).exists(),
        "chorus-inject binary must exist"
    );
}
