//! #1981 — ops_awareness hook should not fire "unreachable" on a healthy API
//!
//! The bug: the first HTTP request after idle hits a cold DNS/TCP path that
//! races the 1s timeout. The hook declares the API unreachable and blocks
//! tool execution (exit code 2) even though the API responds in ~140ms
//! on subsequent requests.
//!
//! What Jeff sees: "⚠ Chorus API unreachable at localhost:3340" injected into
//! the terminal when the API is perfectly healthy.

use serde_json::json;
use std::process::Command;
use chorus_hooks::shared::state_paths::chorus_root;

const SHIM: &str = env!("CARGO_BIN_EXE_chorus-hook-shim");

/// Simulate a PostToolUse hook check via the shim
fn post_tool_hook(tool: &str, input: serde_json::Value) -> (String, String, i32) {
    let hook_input = json!({
        "tool_name": tool,
        "tool_input": input,
        "session_id": "test-ops-awareness-1981",
        "cwd": &format!("{}/roles/kade", chorus_root())
    });

    let output = Command::new(SHIM)
        .arg("post-tool-use")
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
        .expect("failed to run shim");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);
    (stdout, stderr, code)
}

/// #1981 AC2+AC3: When the API is healthy, the hook must not block tool execution
/// or report "unreachable". Run it twice rapidly — the first call is the one that
/// races the cold-start timeout. Both must pass.
#[test]
fn healthy_api_never_reports_unreachable() {
    // Verify API is actually up before testing the hook
    let api_check = Command::new("curl")
        .args(["-sf", "--max-time", "3", "http://localhost:3340/api/chorus/health"])
        .output()
        .expect("curl failed");
    assert!(api_check.status.success(), "API must be running for this test");

    // Run the hook twice — first call may hit cold path, second is warm
    for attempt in 1..=2 {
        let (stdout, stderr, code) = post_tool_hook("Bash", json!({"command": "echo hello"}));

        assert!(
            !stderr.contains("unreachable"),
            "Attempt {}: hook reported API unreachable on a healthy API.\nstderr: {}\nstdout: {}\nexit: {}",
            attempt, stderr, stdout, code
        );

        assert_ne!(
            code, 2,
            "Attempt {}: hook blocked tool execution (exit 2) on a healthy API.\nstderr: {}\nstdout: {}",
            attempt, stderr, stdout
        );
    }
}
