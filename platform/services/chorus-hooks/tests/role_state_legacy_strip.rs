//! #2629 follow-up — query path strips legacy card / card_type fields.
//!
//! Found by wren in #2629 gate:product probe (2026-04-30): legacy state
//! files written before #2467 wave 1 may persist card / card_type fields
//! in /tmp/claude-team-scan/<role>-declared.json. Wave 1 stopped writing
//! them, but the query path was just dumping the parsed JSON — leaking
//! the legacy fields to consumers.
//!
//! Refusal at every surface includes the read path. This integration test
//! confirms: legacy file with card -> query output without card.

use std::fs;
use std::process::Command;

const SHIM: &str = env!("CARGO_BIN_EXE_chorus-hook-shim");
const SCAN_DIR: &str = "/tmp/claude-team-scan";

/// #2614: returns true (and prints a skip line) when RUN_INTEGRATION is unset.
fn skip_unless_integration(reason: &str) -> bool {
    if std::env::var("RUN_INTEGRATION").is_err() {
        eprintln!("SKIP: axis-4 — {reason} (set RUN_INTEGRATION=1 to run)");
        return true;
    }
    false
}

#[test]
fn query_strips_legacy_card_field_from_read_path() {
    if skip_unless_integration("mutates /tmp/claude-team-scan/<role>-declared.json — races team-scan") { return; }
    let role = "kade";
    let path = format!("{}/{}-declared.json", SCAN_DIR, role);

    // Write a legacy-shape file directly (bypasses the writer)
    fs::write(&path,
        r##"{"role":"kade","state":"building","card":"#9999","card_type":"fix","ts":1777580000,"session_alive":true,"source":"declared"}"##
    ).expect("write legacy state file");

    // Sanity: file actually contains legacy fields
    let content = fs::read_to_string(&path).expect("read legacy file");
    assert!(content.contains("\"card\""), "test setup: legacy file should have card");

    // Run query through the shim — same path live consumers hit
    let output = Command::new(SHIM)
        .arg("role-state")
        .arg("query")
        .arg(role)
        .output()
        .expect("query");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(!stdout.contains("\"card\""),
        "query output must NOT contain 'card' field even when state file has it (#2629): {}", stdout);
    assert!(!stdout.contains("\"card_type\""),
        "query output must NOT contain 'card_type' field (#2629): {}", stdout);
    assert!(stdout.contains("\"state\": \"building\"") || stdout.contains("\"state\":\"building\""),
        "query output should still surface state: {}", stdout);
}

#[test]
fn query_unaffected_when_file_has_no_card() {
    if skip_unless_integration("mutates /tmp/claude-team-scan/<role>-declared.json") { return; }
    let role = "kade";
    let path = format!("{}/{}-declared.json", SCAN_DIR, role);

    fs::write(&path,
        r#"{"role":"kade","state":"building","ts":1777580000,"session_alive":true,"source":"declared"}"#
    ).expect("write modern state file");

    let output = Command::new(SHIM)
        .arg("role-state")
        .arg("query")
        .arg(role)
        .output()
        .expect("query");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("\"state\": \"building\"") || stdout.contains("\"state\":\"building\""), "{}", stdout);
    assert!(!stdout.contains("\"card\""), "{}", stdout);
}
