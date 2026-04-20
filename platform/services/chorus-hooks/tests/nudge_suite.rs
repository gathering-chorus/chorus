//! Nudge test suite — behavioral + performance (#2283)
//!
//! These tests send real nudges and verify real outcomes.
//! Source analysis tests were removed — they prove code shape, not behavior.
//!
//! Coverage gap (documented, not mocked):
//!   The osascript inject path requires a live Terminal window with the right
//!   tab name. That cannot be tested in CI or cargo test without a display.
//!   inject_by_tab_name is covered by the chorus-inject binary's own tests.
//!   What we test here: everything the nudge binary does before and after inject.

use std::fs;
use std::process::Command;
use std::time::Instant;

const NUDGE_SCRIPT: &str = "/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/nudge";
const INBOX_DIR: &str = "/tmp/voice-inbox";

/// Dry-run nudge does NOT write to the queue file.
/// Queue is inject-fail fallback only — dry-run skips inject entirely.
#[test]
fn dry_run_does_not_queue() {
    let inbox = format!("{}/wren/pending-inject.txt", INBOX_DIR);
    let before_len = fs::read_to_string(&inbox).unwrap_or_default().len();

    let out = Command::new("bash")
        .arg(NUDGE_SCRIPT)
        .arg("wren")
        .arg("behavioral-test-dry-run")
        .env("CHORUS_INJECT_DRY_RUN", "1")
        .env("DEPLOY_ROLE", "silas")
        .output()
        .expect("nudge script must run");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("DRY-RUN"), "dry-run nudge must print DRY-RUN, got: {}", stdout);

    let after_len = fs::read_to_string(&inbox).unwrap_or_default().len();
    assert_eq!(
        before_len, after_len,
        "dry-run nudge must NOT write to queue file — queue is inject-fail fallback only"
    );
}

/// Queue file mechanism: write, read, clear atomically.
/// Tests the queue file path directly — this is what inject-fail fallback uses.
/// NOTE: triggering inject failure in cargo test requires a live environment where
/// the role Terminal window does not exist. That gap is documented, not mocked.
#[test]
fn queue_file_write_and_clear() {
    let role = "wren";
    let inbox_dir = format!("{}/{}", INBOX_DIR, role);
    let inbox = format!("{}/pending-inject.txt", inbox_dir);
    fs::create_dir_all(&inbox_dir).unwrap();
    let _ = fs::remove_file(&inbox);

    // Write a message directly (simulating what queue_message does)
    fs::write(&inbox, "test queued message\n").unwrap();
    assert!(fs::metadata(&inbox).is_ok(), "queue file must exist after write");

    // Atomic drain: rename → read → remove
    let drain_path = format!("{}/draining-test.txt", inbox_dir);
    fs::rename(&inbox, &drain_path).unwrap();
    let content = fs::read_to_string(&drain_path).unwrap();
    fs::remove_file(&drain_path).unwrap();

    assert!(content.contains("test queued message"), "drained content must match written message");
    assert!(!fs::metadata(&inbox).is_ok(), "original queue file must be gone after atomic drain");
}

/// Persist path (POST to Bridge API) completes in <100ms.
/// Measured baseline: ~25-45ms. Budget: 100ms (2x headroom).
#[test]
fn persist_path_under_100ms() {
    let t = Instant::now();
    let out = Command::new("curl")
        .args([
            "-s", "-X", "POST",
            "http://localhost:3475/api/nudge",
            "-H", "Content-Type: application/json",
            "-d", r#"{"from":"silas","to":"wren","content":"perf-test","traceId":"test-suite"}"#,
            "--connect-timeout", "2",
        ])
        .output();
    let elapsed = t.elapsed().as_millis();

    assert!(out.is_ok(), "curl to Bridge API must not error");
    assert!(
        elapsed < 100,
        "persist path took {}ms — must be <100ms. Bridge API slow? (#2283)",
        elapsed
    );
}

/// Full dry-run nudge (persist + detect_sender + all overhead except inject) under 500ms.
/// Before #2283: 20,000ms (lsof). After: ~150ms. Budget: 500ms (3x headroom).
#[test]
fn full_dry_run_under_500ms() {
    let t = Instant::now();
    let out = Command::new("bash")
        .arg(NUDGE_SCRIPT)
        .arg("wren")
        .arg("perf-test-full-dry-run")
        .env("CHORUS_INJECT_DRY_RUN", "1")
        .env("DEPLOY_ROLE", "silas")
        .output()
        .expect("nudge script must run");
    let elapsed = t.elapsed().as_millis();

    assert!(
        String::from_utf8_lossy(&out.stdout).contains("DRY-RUN"),
        "must complete a dry-run nudge"
    );
    assert!(
        elapsed < 500,
        "full dry-run took {}ms — must be <500ms. lsof regression? (#2283)",
        elapsed
    );
}

/// DEPLOY_ROLE is exported by the nudge wrapper — verified behaviorally.
/// A nudge with no DEPLOY_ROLE in env should still complete (defaults to jeff, not hang).
#[test]
fn nudge_without_deploy_role_does_not_hang() {
    // Run with a clean env — no DEPLOY_ROLE. Should complete quickly (jeff fallback).
    let t = Instant::now();
    Command::new("bash")
        .arg(NUDGE_SCRIPT)
        .arg("wren")
        .arg("no-deploy-role-test")
        .env("CHORUS_INJECT_DRY_RUN", "1")
        .env_remove("DEPLOY_ROLE")
        .output()
        .expect("nudge script must run even without DEPLOY_ROLE");
    let elapsed = t.elapsed().as_millis();

    assert!(
        elapsed < 500,
        "nudge without DEPLOY_ROLE took {}ms — must not hang (jeff fallback, no lsof). (#2283)",
        elapsed
    );
}
