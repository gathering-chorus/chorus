//! chorus-inject integration tests. Behavior-only — exercises the real
//! chorus-inject binary under hermetic dry-run or pure-CLI paths.
//!
//! Source-grep lints (build-time literal-string checks) live separately in
//! `inject_source_gate.rs`. Every file under `tests/` is either behavior
//! or a build-time lint — never mixed. See loom-principles:quality-at-source
//! and #2155 for the honest-label discipline.
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

fn chorus_root() -> String {
    std::env::var("CHORUS_ROOT").ok().filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/Users/jeffbridwell/CascadeProjects/chorus".to_string())
}


// CARGO_BIN_EXE_chorus-inject points to the cargo-built binary for the current
// profile — debug during `cargo test`, which is what tarpaulin instruments.
// Previously this was a hardcoded release-binary path; that path is still
// used by scripts that assume an installed release build (nudge e2e below
// still shells out through the release binary via the nudge script).
const INJECT_BIN: &str = env!("CARGO_BIN_EXE_chorus-inject");
fn nudge_script() -> String { format!("{}/platform/scripts/nudge", chorus_root()) }
// AC1 (source-gate) moved to inject_source_gate.rs per #2155.

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

// macOS-only: nudge.sh delivers via osascript to Terminal.app. CI runs Linux
// → no Terminal.app, no osascript, stdout is empty and assertion fails.
#[cfg(target_os = "macos")]
#[test]
fn nudge_e2e_delivers() {
    // CHORUS_INJECT_DRY_RUN fires the shim's dry-run branch (nudge.rs:312),
    // which prints "DRY-RUN: would emit nudge to <target>..." and skips
    // osascript. Pre-#2435 the message was "would inject to..."; updated
    // here in #2505 to match the canonical emit-only path.
    let output = Command::new("bash")
        .env("CHORUS_INJECT_DRY_RUN", "1")
        .args([nudge_script().as_str(), "silas", "[cargo-test] AC5 nudge e2e", "--force"])
        .output()
        .expect("failed to run nudge");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("DRY-RUN: would emit nudge to silas"),
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

// --- AC: --count-windows CLI path (hermetic — no matching pattern → "0::") ---

// macOS-only: --count-windows uses osascript to enumerate Terminal.app windows.
// On Linux the binary returns non-zero (no osascript). Hermetic only on Mac.
#[cfg(target_os = "macos")]
#[test]
fn count_windows_cli_returns_zero_for_nonmatching_pattern() {
    // Exercises the PrintOut arm in main.rs via the real binary. Uses a
    // pattern that can't appear in a Terminal window name, so stdout is "0::"
    // regardless of host state.
    let output = Command::new(INJECT_BIN)
        .args(["--count-windows", "zzzz_no_such_window_zzzz"])
        .output()
        .expect("failed to run chorus-inject");
    assert!(output.status.success(), "--count-windows should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.trim().starts_with("0::"),
        "non-matching pattern should return 0::, got: {}",
        stdout
    );
}

// --- Binary exists ---

#[test]
fn inject_binary_exists() {
    assert!(
        std::path::Path::new(INJECT_BIN).exists(),
        "chorus-inject binary must exist"
    );
}
