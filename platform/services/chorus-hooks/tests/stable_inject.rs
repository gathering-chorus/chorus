//! #2075 — Stable inject binary tests
//!
//! Prior work: injection in process.rs:70-139, called by nudge.rs:333.
//! Current state: shim rebuilds revoke TCC permissions because osascript
//!   inherits parent's security context.
//! Approach: separate chorus-inject binary that owns osascript injection.
//!   Only it needs Accessibility. Shim delegates via Command::new.

use std::process::Command;

const INJECT: &str = "/Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-hooks/target/release/chorus-inject";
const SHIM: &str = "/Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-hooks/target/release/chorus-hook-shim";

#[test]
fn inject_binary_exists() {
    assert!(
        std::path::Path::new(INJECT).exists(),
        "chorus-inject binary must exist at {}",
        INJECT
    );
}

#[test]
fn inject_binary_is_separate_from_shim() {
    let inject_size = std::fs::metadata(INJECT).map(|m| m.len()).unwrap_or(0);
    let shim_size = std::fs::metadata(SHIM).map(|m| m.len()).unwrap_or(0);
    assert_ne!(inject_size, shim_size, "chorus-inject must be a separate binary from chorus-hook-shim");
}

#[test]
fn inject_shows_usage_without_args() {
    let output = Command::new(INJECT)
        .output()
        .expect("chorus-inject should run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage") || stderr.contains("usage"),
        "should print usage without args, got: {}",
        stderr
    );
    assert!(!output.status.success(), "should exit non-zero without args");
}

#[test]
fn inject_fails_gracefully_for_unknown_role() {
    let output = Command::new(INJECT)
        .args(["nonexistent-role", "test message"])
        .output()
        .expect("chorus-inject should run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "should fail for unknown role, stderr: {}",
        stderr
    );
}
