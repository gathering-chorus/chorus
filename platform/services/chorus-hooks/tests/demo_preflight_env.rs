//! Test: preflight.sh needs PATH to run the cards CLI.
//!
//! Bug: demo_preflight.rs spawns preflight.sh with only CHORUS_ROOT in env.
//! The cards CLI is a bash wrapper around TypeScript — needs node in PATH.
//! Without PATH, `cards view` fails, preflight reads failure as "card not found",
//! and blocks every /demo invocation. 31 consecutive false denials on real cards.
//!
//! Fix: add .env("PATH", ...) to the Command spawn, matching search_hierarchy.rs.

use std::process::Command;

fn chorus_root() -> String {
    std::env::var("CHORUS_ROOT")
        .unwrap_or_else(|_| "/Users/jeffbridwell/CascadeProjects/chorus".to_string())
}

fn home() -> String {
    std::env::var("HOME")
        .unwrap_or_else(|_| "/Users/jeffbridwell".to_string())
}

fn full_path() -> String {
    format!(
        "{}/CascadeProjects/chorus/platform/scripts:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin",
        home()
    )
}

#[test]
fn preflight_fails_without_path() {
    // Current behavior: no PATH → cards CLI fails → preflight blocks
    // Use a known WIP card (1995 is WIP right now)
    let script = format!("{}/skills/demo/gates/preflight.sh", chorus_root());
    let output = Command::new("bash")
        .args([&script, "1995"])
        .env("CHORUS_ROOT", chorus_root())
        .env_remove("PATH")
        .output()
        .expect("failed to run preflight.sh");

    // Without PATH, this should fail — proving the bug
    assert!(
        !output.status.success(),
        "preflight.sh should fail without PATH — this proves the bug. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn preflight_passes_with_path() {
    // Fixed behavior: with PATH → cards CLI works → preflight passes for valid WIP card
    let script = format!("{}/skills/demo/gates/preflight.sh", chorus_root());
    let output = Command::new("bash")
        .args([&script, "1995"])
        .env("CHORUS_ROOT", chorus_root())
        .env("HOME", home())
        .env("PATH", full_path())
        .output()
        .expect("failed to run preflight.sh");

    assert!(
        output.status.success(),
        "preflight.sh should pass with PATH set for WIP card. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
