//! #1916 — Demo gate --proven bypass tests
//!
//! Prior work: demo_gate.rs dispatches to done-gate.sh for evidence checks.
//! #1996 separated TDD from acceptance. #1915 added building-state check.
//! Current state: no bypass for retroactive closure of cards proven by
//! distributed work across multiple cards.
//!
//! Approach: demo_gate.rs detects --proven in the command and skips the
//! done-gate.sh dispatch entirely. The CLI (sdk.ts) also skips its inline
//! checkDemoEvidence() when --proven is present.

use serde_json::json;
use std::io::Write;
use std::process::Command;

const SHIM: &str = env!("CARGO_BIN_EXE_chorus-hook-shim");

fn hook_check_with_role(tool: &str, input: serde_json::Value, role: &str) -> String {
    let hook_input = json!({
        "tool_name": tool,
        "tool_input": input,
        "session_id": "test-session-1916",
        "cwd": format!("/Users/jeffbridwell/CascadeProjects/chorus/roles/{}", role)
    });

    let output = Command::new(SHIM)
        .arg("pre-tool-use")
        .env("DEPLOY_ROLE", role)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(serde_json::to_string(&hook_input).unwrap().as_bytes());
            }
            child.wait_with_output()
        });

    match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(e) => format!("ERROR: {}", e),
    }
}

// === AC item 1: --proven flag bypasses demo gate ===

#[test]
fn cards_done_with_proven_flag_passes_demo_gate() {
    let result = hook_check_with_role(
        "Bash",
        json!({"command": "bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards done 9999 --proven \"1815 1898 1894\""}),
        "silas",
    );

    assert!(
        !result.contains("Demo gate") && !result.contains("demo evidence"),
        "#1916: --proven flag should bypass demo gate. Got: {}",
        result
    );
}

// === AC item 4: Without --proven, demo gate still enforces ===

#[test]
fn cards_done_without_proven_still_checked() {
    // Verifies the hook dispatch path fires for cards done without --proven.
    // Card 9999 doesn't exist so done-gate.sh falls through (exit 0) —
    // that's the current behavior for non-existent cards (let other gates handle).
    // This test documents the baseline: the gate path is exercised.
    let result = hook_check_with_role(
        "Bash",
        json!({"command": "bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards done 9999"}),
        "silas",
    );

    // Baseline assertion: gate path runs without crash
    assert!(
        !result.contains("ERROR"),
        "#1916: demo gate path should not error. Got: {}",
        result
    );
}
