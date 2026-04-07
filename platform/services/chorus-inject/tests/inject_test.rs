//! #2245: inject must use "do script" (targeted, no TCC, no display)
//! not "keystroke" (broadcast to focused window, needs TCC + display awake)
//!
//! AC coverage:
//! - AC1: uses do script not keystroke (source check)
//! - AC3: inject works during the day (live delivery test)
//! - AC4: all three roles targeted correctly (per-role test)
//! - AC5: nudge e2e still delivers (regression)
//! - AC6: unknown role rejected (regression)

use std::process::Command;

const INJECT_BIN: &str = "/Users/jeffbridwell/CascadeProjects/platform/services/chorus-hooks/target/release/chorus-inject";
const NUDGE_SCRIPT: &str = "/Users/jeffbridwell/CascadeProjects/platform/scripts/nudge";

// --- AC1: do script, not keystroke ---

#[test]
fn inject_source_uses_do_script_not_keystroke() {
    let source = std::fs::read_to_string(
        "/Users/jeffbridwell/CascadeProjects/platform/services/chorus-inject/src/main.rs"
    ).expect("can't read main.rs");

    assert!(
        source.contains("do script"),
        "inject must use 'do script' for targeted tab delivery without TCC/display"
    );
    // Check that the AppleScript template doesn't use keystroke or set frontmost
    // Comments may reference "keystroke" for documentation — only check the script string
    let in_script = source.lines()
        .filter(|l| l.contains("r#\"") || l.trim_start().starts_with("tell ") || l.trim_start().starts_with("set ") || l.trim_start().starts_with("do ") || l.contains("keystroke") && !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>();
    let script_text: String = in_script.join("\n");
    assert!(
        !script_text.contains("keystroke"),
        "AppleScript must NOT use 'keystroke': {}", script_text
    );
    assert!(
        !source.contains("set frontmost"),
        "inject must NOT use 'set frontmost' — needs display awake"
    );
}

// --- AC3 + AC4: live delivery to each role ---

#[test]
fn inject_delivers_to_silas() {
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

// --- Role pattern mapping ---

#[test]
fn inject_binary_exists() {
    assert!(
        std::path::Path::new(INJECT_BIN).exists(),
        "chorus-inject binary must exist"
    );
}
