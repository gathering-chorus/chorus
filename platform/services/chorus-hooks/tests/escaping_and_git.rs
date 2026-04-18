//! #2078 — git detection heuristic.
//!
//! Bug 2: git detection assumes team repo when no cd command present.
//!
//! (Bug 1 — #2078 AppleScript single-quote escaping — moved to a unit test
//! on `escape_for_applescript` in chorus-inject/src/main.rs per #2166.)

use std::process::Command;
use serde_json::json;

const SHIM: &str = env!("CARGO_BIN_EXE_chorus-hook-shim");

// === Bug 2: git detection ===

#[test]
fn git_commit_allowed_outside_team_repo() {
    // git commit from /tmp should NOT be blocked — it's outside the team repo
    let hook_input = json!({
        "tool_name": "Bash",
        "tool_input": {"command": "git commit -m 'test'"},
        "session_id": "test-session",
        "cwd": "/tmp/some-other-repo"
    });

    let output = Command::new(SHIM)
        .arg("pre-tool-use")
        .env("DEPLOY_ROLE", "kade")
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
        })
        .expect("shim should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("git-queue.sh"),
        "git commit outside team repo should NOT be blocked, got: {}",
        stdout
    );
}

#[test]
fn git_commit_blocked_inside_team_repo() {
    // git commit from CascadeProjects SHOULD be blocked
    let hook_input = json!({
        "tool_name": "Bash",
        "tool_input": {"command": "git commit -m 'test'"},
        "session_id": "test-session",
        "cwd": "/Users/jeffbridwell/CascadeProjects/chorus/roles/kade"
    });

    let output = Command::new(SHIM)
        .arg("pre-tool-use")
        .env("DEPLOY_ROLE", "kade")
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
        })
        .expect("shim should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("git-queue.sh"),
        "git commit inside team repo should be blocked, got: {}",
        stdout
    );
}
