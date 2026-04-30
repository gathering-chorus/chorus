//! #1846 + #1847 — Spine event tests for context cache failures and nudge acknowledgment
//!
//! #1846: session.context.error emitted when cache is empty/failed at boot
//! #1847: nudge.acknowledged emitted when role processes a nudge
//!
//! #2614: tests in this file mutate real role-state paths
//! (`/tmp/session-context-kade.md`, `/tmp/voice-inbox/kade/...`) — same paths the
//! live daemon reads. Running them on a developer machine while kade is in a
//! session can wipe kade's context. Gated behind `RUN_INTEGRATION=1`; pre-commit
//! and default `cargo test` runs skip them.

use std::fs;
use std::process::Command;
use chorus_hooks::shared::state_paths::chorus_root;

const SHIM: &str = env!("CARGO_BIN_EXE_chorus-hook-shim");
fn chorus_log() -> String { format!("{}/platform/logs/chorus.log", chorus_root()) }
fn log_tail(n: usize) -> String {
    let content = fs::read_to_string(chorus_log()).unwrap_or_default();
    content.lines().rev().take(n).collect::<Vec<_>>().join("\n")
}

/// #2614: returns true (and prints a skip line) when RUN_INTEGRATION is unset.
/// Use at the top of any test that mutates real role-state paths or APIs so
/// default `cargo test` runs don't race the live daemon.
fn skip_unless_integration(reason: &str) -> bool {
    if std::env::var("RUN_INTEGRATION").is_err() {
        eprintln!("SKIP: axis-4 — {reason} (set RUN_INTEGRATION=1 to run)");
        return true;
    }
    false
}

// === #1846: context cache failure events ===

#[test]
fn session_start_emits_context_error_on_empty_cache() {
    if skip_unless_integration("writes /tmp/session-context-kade.md, races live daemon") { return; }
    // Remove the cache file so session-start gets empty content
    let cache_file = "/tmp/session-context-kade.md";
    let cache_backup = fs::read_to_string(cache_file).ok();
    let _ = fs::remove_file(cache_file);

    // Also ensure the context-cache won't regenerate successfully in time
    // by creating an empty cache file (simulates failed generation)
    let _ = fs::write(cache_file, "");

    let output = Command::new(SHIM)
        .args(["session-start", "kade"])
        .output()
        .expect("session-start should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let log = log_tail(5);

    // Should emit session.context.error
    assert!(
        log.contains("session.context.error") || stdout.contains("context.error"),
        "should emit session.context.error when cache is empty, log tail: {}, stdout: {}",
        log, stdout
    );

    // Restore cache
    match cache_backup {
        Some(content) => { let _ = fs::write(cache_file, content); }
        None => { let _ = fs::remove_file(cache_file); }
    }
}

// === #1847: nudge acknowledgment ===

// macOS-only: role-state drain logic depends on the per-role LaunchAgent
// inbox lifecycle (/tmp/voice-inbox/<role>/pending-inject.txt + tick-poller).
// On Linux the shim writes role.state.changed but the drain code path that
// emits nudge.acknowledged doesn't fire — same Mac-stack pattern as
// ops_awareness_timeout / healthy_api_never_reports_unreachable.
#[cfg(target_os = "macos")]
#[test]
fn role_state_drain_emits_nudge_acknowledged() {
    if skip_unless_integration("writes /tmp/voice-inbox/kade/, races live daemon's drain") { return; }
    let test_role = "kade";
    let inbox_dir = format!("/tmp/voice-inbox/{}", test_role);
    let inbox_file = format!("{}/pending-inject.txt", inbox_dir);

    let _ = fs::create_dir_all(&inbox_dir);
    fs::write(&inbox_file, "test nudge for ack tracking\n").expect("write test nudge");

    // Transition to waiting — triggers drain
    let output = Command::new(SHIM)
        .args(["role-state", test_role, "waiting"])
        .output()
        .expect("role-state should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let log = log_tail(5);

    // Should emit nudge.acknowledged
    assert!(
        log.contains("nudge.acknowledged") || stdout.contains("nudge.acknowledged"),
        "should emit nudge.acknowledged when draining nudges, log tail: {}, stdout: {}",
        log, stdout
    );
}
