//! #2058 — role-state must clear card field on waiting/idle transitions.
//!
//! Bug: transitioning to waiting/idle carries forward the card from the
//! previous building state. Clearing tiles show stale card IDs.

use std::fs;
use std::process::Command;

const SCAN_DIR: &str = "/tmp/claude-team-scan";
const SHIM: &str = "/Users/jeffbridwell/CascadeProjects/platform/services/chorus-hooks/target/release/chorus-hook-shim";

fn role_state(args: &[&str]) -> std::process::Output {
    Command::new(SHIM)
        .arg("role-state")
        .args(args)
        .output()
        .expect("failed to run chorus-hook-shim role-state")
}

fn read_state(role: &str) -> String {
    let path = format!("{}/{}-declared.json", SCAN_DIR, role);
    fs::read_to_string(&path).unwrap_or_default()
}

#[test]
fn waiting_clears_card_from_previous_building() {
    // Set building with a card
    role_state(&["kade", "building", "card=2058"]);
    let content = read_state("kade");
    assert!(content.contains("\"card\":2058"), "building should have card, got: {}", content);

    // Transition to waiting — card must be gone
    role_state(&["kade", "waiting"]);
    let content = read_state("kade");
    assert!(!content.contains("\"card\""), "waiting should have no card field, got: {}", content);
}

#[test]
fn idle_clears_card_from_previous_building() {
    role_state(&["kade", "building", "card=2058"]);
    role_state(&["kade", "idle"]);
    let content = read_state("kade");
    assert!(!content.contains("\"card\""), "idle should have no card field, got: {}", content);
}

#[test]
fn building_still_sets_card_correctly() {
    role_state(&["kade", "building", "card=9999"]);
    let content = read_state("kade");
    assert!(content.contains("\"card\":9999"), "building should set card, got: {}", content);
}
