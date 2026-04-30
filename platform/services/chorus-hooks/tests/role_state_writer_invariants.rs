//! #2168 AC-13 — Writer-discipline invariants for role-state files.
//!
//! Each file has exactly one writer by architecture:
//!   <role>-declared.json  — written by role_state.rs apply_state,
//!                           source field must always be "declared".
//!   <role>-inferred.json  — written by observer.rs write_inferred_state,
//!                           source field must always be "inferred".
//!
//! Violation of either direction is a cross-domain write bug: the old
//! #2120 reconciler mutated declared.json from observer context, which
//! produced the card_declared=null Clearing lie. The file-split + source-
//! stamping + these invariants close that class.

use std::fs;
use std::process::Command;

/// #2614: returns true (and prints a skip line) when RUN_INTEGRATION is unset.
fn skip_unless_integration(reason: &str) -> bool {
    if std::env::var("RUN_INTEGRATION").is_err() {
        eprintln!("SKIP: axis-4 — {reason} (set RUN_INTEGRATION=1 to run)");
        return true;
    }
    false
}

const SCAN_DIR: &str = "/tmp/claude-team-scan";
const SHIM: &str = env!("CARGO_BIN_EXE_chorus-hook-shim");

fn role_state(args: &[&str]) {
    Command::new(SHIM)
        .arg("role-state")
        .args(args)
        .output()
        .expect("role-state run");
}

fn read_source(path: &str) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;
    parsed.get("source")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

#[test]
fn declared_file_always_source_declared() {
    if skip_unless_integration("mutates real role-state files at /tmp/claude-team-scan/") { return; }
    let role = "kade";
    let path = format!("{}/{}-declared.json", SCAN_DIR, role);

    for state in ["building", "blocked", "waiting", "idle", "observing"] {
        let _ = fs::remove_file(&path);
        role_state(&[role, state, "card=2168"]);
        if let Some(source) = read_source(&path) {
            assert_eq!(
                source, "declared",
                "{}.declared.json after state={} had source={}, must be 'declared'",
                role, state, source
            );
        }
        // Absence of file for the state is also valid (no assertion).
    }
}

#[test]
fn declared_file_never_contains_source_inferred() {
    if skip_unless_integration("mutates real role-state files at /tmp/claude-team-scan/") { return; }
    // Direct invariant: scan all *-declared.json and fail if any have source="inferred".
    if let Ok(entries) = fs::read_dir(SCAN_DIR) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.ends_with("-declared.json") {
                if let Some(source) = read_source(path.to_str().unwrap_or("")) {
                    assert_ne!(
                        source, "inferred",
                        "{} has source='inferred' — reconciler wrote into declared territory",
                        name
                    );
                }
            }
        }
    }
}

#[test]
fn inferred_file_never_contains_source_declared() {
    if skip_unless_integration("mutates real role-state files at /tmp/claude-team-scan/") { return; }
    // Direct invariant: scan all *-inferred.json and fail if any have source="declared".
    if let Ok(entries) = fs::read_dir(SCAN_DIR) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.ends_with("-inferred.json") {
                if let Some(source) = read_source(path.to_str().unwrap_or("")) {
                    assert_ne!(
                        source, "declared",
                        "{} has source='declared' — human writer wrote into inferred territory",
                        name
                    );
                }
            }
        }
    }
}
