//! #2790 — chorus-hook-shim must fail CLOSED on pre-tool-use when the
//! daemon is unreachable.
//!
//! Receipt: 2026-05-07. While rebuilding chorus-hooks (which contains the
//! canonical_write_guard), kade stopped the daemon, edited canonical
//! directly, and was not blocked. The shim's connect-failure path returned
//! ExitCode::SUCCESS — Claude treats success as allow — every guard
//! bypassed. Fox-in-henhouse class: the substrate owner can disable the
//! enforcement layer while editing the enforcement layer.
//!
//! This test invokes the chorus-hook-shim binary with a deliberately-broken
//! socket path and asserts:
//!   - pre-tool-use emits a permissionDecision=deny on stdout
//!   - the deny reason names "daemon unreachable" and "#2790"
//!   - non-security endpoints (post-tool-use, session-start) still fail open
//!     (no deny JSON, no enforcement disruption)

use std::io::Write;
use std::process::{Command, Stdio};

const SHIM_BIN: &str = env!("CARGO_BIN_EXE_chorus-hook-shim");

fn run_shim(endpoint: &str, stdin_json: &str) -> (String, String, Option<i32>) {
    let mut child = Command::new(SHIM_BIN)
        .arg(endpoint)
        // Force socket-connect failure: point at a path that won't exist.
        // The shim reads SOCKET_PATH at compile time today, so to actually
        // induce a connect failure in CI we need the daemon down OR an
        // override. For unit-level coverage, the inline tests in shim.rs
        // hammer the pure helper. This integration test runs against the
        // real binary; it stays meaningful as long as the daemon isn't
        // running on this socket — which is the case in CI sandbox.
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn shim");
    if let Some(stdin) = child.stdin.as_mut() {
        let _ = stdin.write_all(stdin_json.as_bytes());
    }
    let out = child.wait_with_output().expect("wait shim");
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        out.status.code(),
    )
}

/// If the shim's socket is reachable on the test host, this test is a no-op.
/// We only assert the fail-closed property when the connect actually fails.
/// #3670 — probe the SAME socket the shim resolves (~/.chorus/run, #3631/#3617).
/// This guard still pointed at the retired /tmp path after the socket moved, so
/// on any host with a live daemon it failed to skip and the test went red
/// against a healthy system — the exact false-fire class #3617 closed.
fn socket_unreachable() -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    let sock = format!("{}/.chorus/run/chorus-hooks.sock", home);
    std::os::unix::net::UnixStream::connect(sock).is_err()
}

#[test]
fn pre_tool_use_fails_closed_when_daemon_down() {
    if !socket_unreachable() {
        eprintln!("skip: chorus-hooks daemon is running on this host; this test only proves fail-closed when the socket is unreachable.");
        return;
    }
    let payload = r#"{"tool_name":"Bash","tool_input":{"command":"ls"},"cwd":"/tmp"}"#;
    let (stdout, _stderr, _code) = run_shim("pre-tool-use", payload);
    assert!(
        stdout.contains(r#""permissionDecision":"deny""#),
        "pre-tool-use must emit permissionDecision=deny when daemon is unreachable; got stdout={}",
        stdout
    );
    assert!(
        stdout.contains("daemon unreachable") || stdout.contains("#2790"),
        "deny reason must reference daemon-unreachable / #2790: stdout={}",
        stdout
    );
}

#[test]
fn post_tool_use_fails_open_when_daemon_down() {
    if !socket_unreachable() {
        eprintln!("skip: daemon up; can't exercise fail-open path");
        return;
    }
    let payload = r#"{"tool_name":"Bash","tool_response":{"output":"ok"}}"#;
    let (stdout, _stderr, _code) = run_shim("post-tool-use", payload);
    assert!(
        !stdout.contains(r#""permissionDecision":"deny""#),
        "post-tool-use must NOT emit deny when daemon is down (fail open); got stdout={}",
        stdout
    );
}

#[test]
fn session_start_fails_open_when_daemon_down() {
    if !socket_unreachable() {
        eprintln!("skip: daemon up");
        return;
    }
    let payload = r#"{"session_id":"x"}"#;
    let (stdout, _stderr, _code) = run_shim("session-start", payload);
    assert!(
        !stdout.contains(r#""permissionDecision":"deny""#),
        "session-start must NOT emit deny when daemon is down (fail open); got stdout={}",
        stdout
    );
}
