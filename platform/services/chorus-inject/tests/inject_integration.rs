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

#[cfg(target_os = "macos")]
fn chorus_root() -> String {
    std::env::var("CHORUS_ROOT").ok().filter(|s| !s.is_empty())
        .expect("CHORUS_ROOT must be set and non-empty")
}


// CARGO_BIN_EXE_chorus-inject points to the cargo-built binary for the current
// profile — debug during `cargo test`, which is what tarpaulin instruments.
// Previously this was a hardcoded release-binary path; that path is still
// used by scripts that assume an installed release build (nudge e2e below
// still shells out through the release binary via the nudge script).
const INJECT_BIN: &str = env!("CARGO_BIN_EXE_chorus-inject");
#[cfg(target_os = "macos")]
fn nudge_script() -> String { format!("{}/platform/scripts/nudge", chorus_root()) }
// AC1 (source-gate) moved to inject_source_gate.rs per #2155.

// #2804 — bypass-gate observability test (Kade gemba 2026-05-08). Asserts
// that CHORUS_INJECT_BYPASS_GATE=1 invocations land a chorus_inject.bypass_invoked
// event in chorus.log so illegitimate bypass surfaces in data, not via incident.
#[test]
fn bypass_gate_emits_spine_event() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let log_path = tmp.path().join("chorus.log");
    let log_str = log_path.to_str().unwrap();

    // CHORUS_INJECT_DRY_RUN keeps osascript out of the test; bypass+dry-run
    // is the same shape build-signed.sh + test-runners use.
    let output = Command::new(INJECT_BIN)
        .env("CHORUS_INJECT_BYPASS_GATE", "1")
        .env("CHORUS_INJECT_DRY_RUN", "1")
        .env("CHORUS_LOG_FILE", log_str)
        .args(["wren", "[bypass-test] should land in spine"])
        .output()
        .expect("failed to run chorus-inject");
    assert!(output.status.success(), "bypass+dry-run should succeed");

    let log_content = std::fs::read_to_string(&log_path).unwrap_or_default();
    assert!(
        log_content.contains("chorus_inject.bypass_invoked"),
        "bypass invocation must land chorus_inject.bypass_invoked event. Got: {}",
        log_content,
    );
    assert!(
        log_content.contains("\"role\":\"chorus-inject\""),
        "spine event must carry role=chorus-inject. Got: {}",
        log_content,
    );
}

// #2804 — pulse-internal path does NOT emit bypass spine event (it's the
// canonical caller, not a bypass). Verifies the gate distinguishes the two.
#[test]
fn pulse_internal_path_does_not_emit_bypass_event() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let log_path = tmp.path().join("chorus.log");
    let log_str = log_path.to_str().unwrap();

    let output = Command::new(INJECT_BIN)
        .env("_NUDGE_PULSE_INTERNAL", "1")
        .env("CHORUS_INJECT_DRY_RUN", "1")
        .env("CHORUS_LOG_FILE", log_str)
        .args(["wren", "[pulse-internal-test] should NOT emit bypass event"])
        .output()
        .expect("failed to run chorus-inject");
    assert!(output.status.success(), "pulse-internal+dry-run should succeed");

    let log_content = std::fs::read_to_string(&log_path).unwrap_or_default();
    assert!(
        !log_content.contains("chorus_inject.bypass_invoked"),
        "pulse-internal path is canonical, not bypass — must NOT emit bypass_invoked. Got: {}",
        log_content,
    );
}

// --- AC3 + AC4: delivery path per role (dry-run — hermetic, no side effects) ---

#[test]
fn inject_delivers_to_silas() {
    // #2804 — chorus-inject rejects shell-direct calls without the
    // pulse-internal env. Tests use CHORUS_INJECT_BYPASS_GATE to bypass.
    let output = Command::new(INJECT_BIN)
        .env("CHORUS_INJECT_BYPASS_GATE", "1")
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
    // #2804 — chorus-inject rejects shell-direct calls without the
    // pulse-internal env. Tests use CHORUS_INJECT_BYPASS_GATE to bypass.
    let output = Command::new(INJECT_BIN)
        .env("CHORUS_INJECT_BYPASS_GATE", "1")
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
    // #2804 — chorus-inject rejects shell-direct calls without the
    // pulse-internal env. Tests use CHORUS_INJECT_BYPASS_GATE to bypass.
    let output = Command::new(INJECT_BIN)
        .env("CHORUS_INJECT_BYPASS_GATE", "1")
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
    // #2804 — chorus-inject rejects shell-direct calls without the
    // pulse-internal env. Tests use CHORUS_INJECT_BYPASS_GATE to bypass.
    let output = Command::new(INJECT_BIN)
        .env("CHORUS_INJECT_BYPASS_GATE", "1")
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
    // #2804 — chorus-inject rejects shell-direct calls without the
    // pulse-internal env. Tests use CHORUS_INJECT_BYPASS_GATE to bypass.
    let output = Command::new(INJECT_BIN)
        .env("CHORUS_INJECT_BYPASS_GATE", "1")
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
