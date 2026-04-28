//! #1915 — TDD gate should skip when role is not actively building
//!
//! Bug: Role not in building state (idle, waiting, observing) still gets
//! blocked by TDD gate when demoing or editing code. Acceptance and
//! retroactive closure should be exempt.

use serde_json::json;
use std::io::Write;
use std::process::Command;
use chorus_hooks::shared::state_paths::chorus_root;

const SHIM: &str = env!("CARGO_BIN_EXE_chorus-hook-shim");

/// Helper: simulate a PreToolUse hook check via the shim
fn hook_check_with_role(tool: &str, input: serde_json::Value, role: &str) -> String {
    let hook_input = json!({
        "tool_name": tool,
        "tool_input": input,
        "session_id": "test-session-1915",
        "cwd": format!("{}/roles/{}", chorus_root(), role)
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

/// Write a role state file to simulate building/idle states
fn set_role_state(role: &str, state: &str, card: Option<u64>, card_type: Option<&str>) {
    let path = format!("/tmp/claude-team-scan/{}-declared.json", role);
    let mut val = json!({
        "role": role,
        "state": state,
        "ts": 1776191641,
        "last_emit": "2026-04-14 14:00:00",
        "session_alive": true,
        "wall_clock": "2026-04-14 14:00:00",
        "pid": 99999
    });
    if let Some(c) = card {
        val["card"] = json!(c);
    }
    if let Some(ct) = card_type {
        val["card_type"] = json!(ct);
    }
    std::fs::write(&path, serde_json::to_string(&val).unwrap()).unwrap();
}

/// Use a test role name to avoid stomping real state.
/// Falls back to "wren" since the test harness checks real paths.
const TEST_ROLE: &str = "wren";

// === AC item 2: Role not building → demo allowed without tests ===

#[test]
fn idle_role_demo_allowed_without_tests() {
    // Set role to idle (not building)
    set_role_state(TEST_ROLE, "idle", None, None);

    let result = hook_check_with_role(
        "Skill",
        json!({"skill": "demo"}),
        TEST_ROLE,
    );

    // Restore state
    set_role_state(TEST_ROLE, "idle", None, None);

    // Should NOT contain TDD block
    assert!(
        !result.contains("TDD gate"),
        "#1915: idle role should not be blocked by TDD gate on demo. Got: {}",
        result
    );
}

// === AC item 2: Role not building → code edit allowed ===

#[test]
fn idle_role_code_edit_allowed_without_tests() {
    set_role_state(TEST_ROLE, "idle", None, None);

    let result = hook_check_with_role(
        "Edit",
        json!({"file_path": &format!("{}/platform/api/src/server.ts", chorus_root()), "old_string": "foo", "new_string": "bar"}),
        TEST_ROLE,
    );

    set_role_state(TEST_ROLE, "idle", None, None);

    assert!(
        !result.contains("TDD gate"),
        "#1915: idle role should not be blocked by TDD gate on code edit. Got: {}",
        result
    );
}

// === AC item 3: Role building → gate still enforces ===
// Use type:new to avoid log-first gate intercepting before TDD gate

#[test]
fn building_role_code_edit_still_blocked_without_tests() {
    // Temporarily clear ALL role states to avoid is_fix_card() cross-contamination
    for role in &["kade", "silas", "wren"] {
        set_role_state(role, "idle", None, None);
    }

    // Set test role to building a type:new card (avoids log-first gate)
    set_role_state(TEST_ROLE, "building", Some(9999), Some("new"));

    let result = hook_check_with_role(
        "Edit",
        json!({"file_path": &format!("{}/platform/api/src/server.ts", chorus_root()), "old_string": "foo", "new_string": "bar"}),
        TEST_ROLE,
    );

    // Restore
    set_role_state(TEST_ROLE, "idle", None, None);

    assert!(
        result.contains("TDD gate") || result.contains("haven't written a test"),
        "#1915: building role should still be blocked by TDD gate. Got: {}",
        result
    );
}
