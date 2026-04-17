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
//! **Integration (exercises the real binary, hermetic via dry-run)** — per #2166:
//! - `inject_delivers_to_silas` / `_to_wren` / `_to_kade` — exec
//!   chorus-inject with CHORUS_INJECT_DRY_RUN=1. Validates argv parse,
//!   role lookup, escape, and success exit without firing osascript.
//! - `nudge_e2e_delivers` — exec nudge script end-to-end with the same
//!   env seam propagated; shim routes through chorus-inject (#2077) so
//!   the dry-run fires all the way down.
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

// --- AC3 + AC4: delivery path per role (dry-run — hermetic, no side effects) ---

#[test]
fn inject_delivers_to_silas() {
    let output = Command::new(INJECT_BIN)
        .env("CHORUS_INJECT_DRY_RUN", "1")
        .args(["silas", "[cargo-test] AC4 silas inject"])
        .output()
        .expect("failed to run chorus-inject");
    assert!(output.status.success(), "dry-run inject should succeed for silas");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("DRY-RUN inject role=silas pattern=silas"),
        "dry-run stdout should describe what would inject: {}", stdout
    );
}

#[test]
fn inject_delivers_to_wren() {
    let output = Command::new(INJECT_BIN)
        .env("CHORUS_INJECT_DRY_RUN", "1")
        .args(["wren", "[cargo-test] AC4 wren inject"])
        .output()
        .expect("failed to run chorus-inject");
    assert!(output.status.success(), "dry-run inject should succeed for wren");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("DRY-RUN inject role=wren pattern=wren"),
        "dry-run stdout should describe what would inject: {}", stdout
    );
}

#[test]
fn inject_delivers_to_kade() {
    let output = Command::new(INJECT_BIN)
        .env("CHORUS_INJECT_DRY_RUN", "1")
        .args(["kade", "[cargo-test] AC4 kade inject"])
        .output()
        .expect("failed to run chorus-inject");
    assert!(output.status.success(), "dry-run inject should succeed for kade");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("DRY-RUN inject role=kade pattern=kade"),
        "dry-run stdout should describe what would inject: {}", stdout
    );
}

// --- AC5: nudge e2e regression (dry-run via shim env-gate, #2166) ---

#[test]
fn nudge_e2e_delivers() {
    // CHORUS_INJECT_DRY_RUN fires the shim's dry-run branch (nudge.rs), which
    // prints "DRY-RUN: would inject to <target>..." and skips osascript.
    let output = Command::new("bash")
        .env("CHORUS_INJECT_DRY_RUN", "1")
        .args([NUDGE_SCRIPT, "silas", "[cargo-test] AC5 nudge e2e", "--force"])
        .output()
        .expect("failed to run nudge");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("DRY-RUN: would inject to silas"),
        "nudge e2e under CHORUS_INJECT_DRY_RUN should dry-run at shim level, got: {}",
        stdout
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
