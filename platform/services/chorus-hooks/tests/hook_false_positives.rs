//! #2047 + #2062 — Hook false positive tests
//!
//! #2047: infra guardrail blocks kill -0 (signal-0 liveness check) — should allow
//! #2062: pair gate blocks CSS/HTML/static assets — should allow without pair

use serde_json::json;

// We can't call the async check functions directly from integration tests
// without the full hook server. Test via the shim binary instead.
use std::process::Command;

const SHIM: &str = "/Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-hooks/target/release/chorus-hook-shim";

/// Helper: simulate a PreToolUse hook check via the shim
fn hook_check(tool: &str, input: serde_json::Value) -> String {
    let hook_input = json!({
        "tool_name": tool,
        "tool_input": input,
        "session_id": "test-session",
        "cwd": "/Users/jeffbridwell/CascadeProjects/chorus/platform/roles/silas"
    });

    let output = Command::new(SHIM)
        .arg("pre-tool-use")
        .env("DEPLOY_ROLE", "silas")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
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

// === #2047: kill -0 should be allowed ===

#[test]
fn kill_signal_0_allowed() {
    let result = hook_check("Bash", json!({"command": "kill -0 12345"}));
    assert!(!result.contains("BLOCKED"), "kill -0 should be allowed (signal-0 is a liveness check), got: {}", result);
}

#[test]
fn kill_9_still_blocked() {
    let result = hook_check("Bash", json!({"command": "kill -9 12345"}));
    assert!(result.contains("deny") || result.contains("BLOCKED"), "kill -9 should still be blocked, got: {}", result);
}
