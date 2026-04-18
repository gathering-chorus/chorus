//! Role-state card-field invariants per state (#2058 + #2168 AC-8).
//!
//! Semantic (current, set by #2168 AC-8):
//!   building / blocked / waiting — preserve card (role still points at a card;
//!     "waiting on #N for review" is a valid state).
//!   idle / observing — clear card (role has no card-of-its-own).
//!
//! #2058's original concern — tiles show stale card IDs — is now handled by
//! source-stamping (declared vs inferred) + tile logic that surfaces ALL active
//! WIP rather than mirroring a single cleared field.

use std::fs;
use std::process::Command;

const SCAN_DIR: &str = "/tmp/claude-team-scan";
const SHIM: &str = env!("CARGO_BIN_EXE_chorus-hook-shim");

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
fn waiting_preserves_card_from_previous_building() {
    // Set building with a card
    role_state(&["kade", "building", "card=2058"]);
    let content = read_state("kade");
    assert!(content.contains("\"card\":2058"), "building should have card, got: {}", content);

    // Transition to waiting — card preserved (#2168 AC-8: "waiting on #N" is valid)
    role_state(&["kade", "waiting"]);
    let content = read_state("kade");
    assert!(content.contains("\"card\":2058"), "waiting should preserve card, got: {}", content);
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
