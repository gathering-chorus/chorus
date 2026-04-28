//! Test: all hooks that spawn `cards`, `npx`, or `python3` must include PATH.
//!
//! Bug (#1993): 6 hooks spawn tools needing node/npm/python in PATH,
//! but the hooks server LaunchAgent has no PATH. Commands fail silently.
//! Same root cause as #1992 (demo_preflight).

use std::process::Command;
use chorus_hooks::shared::state_paths::chorus_root;


fn home() -> String {
    std::env::var("HOME").expect("HOME must be set")
}

fn full_path() -> String {
    format!(
        "{}/CascadeProjects/chorus/platform/scripts:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin",
        home()
    )
}

// --- accept_gate: cards view needs PATH ---

#[test]
fn accept_gate_cards_view_fails_without_path() {
    let cards = format!("{}/platform/scripts/cards", chorus_root());
    let output = Command::new("bash")
        .args([&cards, "view", "1992"])
        .env("CHORUS_ROOT", chorus_root())
        .env_remove("PATH")
        .output()
        .expect("failed to run cards");

    assert!(!output.status.success(),
        "cards view should fail without PATH — proves the bug");
}

// Requires live cards CLI → chorus-api (localhost:3340) → Vikunja stack.
// That stack runs as launchctl LaunchAgents on Mac; CI Linux runner has none.
// Mac dev: runs as part of `cargo test`. CI: ignored.
#[cfg_attr(not(target_os = "macos"), ignore = "needs cards CLI + chorus API + Vikunja stack (Mac LaunchAgents)")]
#[test]
fn accept_gate_cards_view_passes_with_path() {
    let cards = format!("{}/platform/scripts/cards", chorus_root());
    let output = Command::new("bash")
        .args([&cards, "view", "1992"])
        .env("CHORUS_ROOT", chorus_root())
        .env("HOME", home())
        .env("PATH", full_path())
        .output()
        .expect("failed to run cards");

    assert!(output.status.success(),
        "cards view should pass with PATH. stderr: {}",
        String::from_utf8_lossy(&output.stderr));
}

// --- icd_write_gate: npx needs PATH ---

#[test]
fn npx_fails_without_path() {
    let output = Command::new("npx")
        .args(["--version"])
        .env_remove("PATH")
        .output();

    assert!(output.is_err() || !output.unwrap().status.success(),
        "npx should fail without PATH");
}

#[test]
fn npx_passes_with_path() {
    let output = Command::new("npx")
        .args(["--version"])
        .env("PATH", full_path())
        .output()
        .expect("failed to run npx");

    assert!(output.status.success(),
        "npx should pass with PATH. stderr: {}",
        String::from_utf8_lossy(&output.stderr));
}

// --- icd_write_gate: python3 needs PATH ---
// Note: python3 is at /usr/bin/python3 on macOS (Xcode tools), so it resolves
// even without PATH. The fix still adds PATH for consistency and portability.

#[test]
fn python3_passes_with_path() {
    let output = Command::new("python3")
        .args(["--version"])
        .env("PATH", full_path())
        .output()
        .expect("failed to run python3");

    assert!(output.status.success(),
        "python3 should pass with PATH. stderr: {}",
        String::from_utf8_lossy(&output.stderr));
}
