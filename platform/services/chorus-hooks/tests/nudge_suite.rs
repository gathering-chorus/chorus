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
use chorus_hooks::shared::state_paths::chorus_root;

fn nudge_script() -> String { format!("{}/platform/scripts/nudge", chorus_root()) }
const INBOX_DIR: &str = "/tmp/voice-inbox";

/// #2614: tests in this file invoke real `nudge` script with real role names
/// (`wren`, `silas`) and read/write `/tmp/voice-inbox/<role>/` — same paths
/// the live daemon drains. Gated behind `RUN_INTEGRATION=1`; default
/// `cargo test` skips them with reason.
fn skip_unless_integration(reason: &str) -> bool {
    if std::env::var("RUN_INTEGRATION").is_err() {
        eprintln!("SKIP: axis-4 — {reason} (set RUN_INTEGRATION=1 to run)");
        return true;
    }
    false
}

/// Dry-run nudge does NOT write to the queue file.
/// Queue is inject-fail fallback only — dry-run skips inject entirely.
#[test]
fn dry_run_does_not_queue() {
    if skip_unless_integration("reads/writes /tmp/voice-inbox/, races live daemon") { return; }
    let inbox = format!("{}/wren/pending-inject.txt", INBOX_DIR);
    let before_len = fs::read_to_string(&inbox).unwrap_or_default().len();

    let out = Command::new("bash")
        .arg(nudge_script())
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
    if skip_unless_integration("reads/writes /tmp/voice-inbox/, races live daemon") { return; }
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
    assert!(fs::metadata(&inbox).is_err(), "original queue file must be gone after atomic drain");
}

/// Persist path (POST to Bridge API) completes in <100ms.
/// Measured baseline: ~25-45ms. Budget: 100ms (2x headroom).
#[test]
fn persist_path_under_100ms() {
    if skip_unless_integration("reads/writes /tmp/voice-inbox/, races live daemon") { return; }
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
    if skip_unless_integration("reads/writes /tmp/voice-inbox/, races live daemon") { return; }
    let t = Instant::now();
    let out = Command::new("bash")
        .arg(nudge_script())
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

/// #2287: DEPLOY_ROLE unset is a contract violation, not a supported state.
/// The nudge binary fails loud instead of defaulting to "jeff".
#[test]
fn nudge_without_deploy_role_fails_loud() {
    if skip_unless_integration("reads/writes /tmp/voice-inbox/, races live daemon") { return; }
    let out = Command::new("bash")
        .arg(nudge_script())
        .arg("wren")
        .arg("contract-test")
        .env("CHORUS_INJECT_DRY_RUN", "1")
        .env_remove("DEPLOY_ROLE")
        .output()
        .expect("nudge script must run");

    assert!(!out.status.success(), "nudge without DEPLOY_ROLE must exit non-zero (contract violation)");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("CONTRACT VIOLATION") && stderr.contains("DEPLOY_ROLE"),
        "stderr must name the contract violation: {}", stderr
    );
}

/// #2435 — nudge CLI emits a canonical `nudge.emitted` spine event per invocation.
/// Producer side of the one-canonical-path design: consumers fold nudge.emitted
/// against nudge.surfaced to compute unread sets. Parallel-run with legacy
/// role.nudge.sent; post-flag-flip, role.nudge.sent retires.
///
/// Event name chosen to avoid collision with role_state drain's nudge.acknowledged
/// (count-payload, different semantics — Kade's 0.3 audit).
#[test]
fn nudge_cli_emits_canonical_emitted_event() {
    if skip_unless_integration("reads/writes /tmp/voice-inbox/, races live daemon") { return; }
    let log_path = &format!("{}/platform/logs/chorus.log", chorus_root());
    let marker = format!("emit-test-{}-canonical", std::process::id());

    let out = Command::new("bash")
        .arg(nudge_script())
        .arg("wren")
        .arg(&marker)
        .env("CHORUS_INJECT_DRY_RUN", "1")
        .env("DEPLOY_ROLE", "silas")
        .output()
        .expect("nudge script must run");
    assert!(out.status.success(), "dry-run nudge must succeed");

    let log = fs::read_to_string(log_path).unwrap_or_default();
    let recent: Vec<&str> = log.lines().rev().take(400).collect();
    let our_lines: Vec<&&str> = recent.iter().filter(|l| l.contains(&marker)).collect();
    assert!(
        !our_lines.is_empty(),
        "chorus.log must contain lines with marker `{}` after nudge", marker
    );

    let emitted_line = our_lines
        .iter()
        .find(|l| l.contains("\"event\":\"nudge.emitted\""))
        .unwrap_or_else(|| panic!(
            "chorus.log must contain a nudge.emitted event for marker `{}`. Got: {:?}",
            marker, our_lines
        ));

    // chorus_log flattens the first key=value as a proper JSON field and crams the
    // rest into its value. What we guarantee: emitted event carries sender as role,
    // target + trace id + the marker are all somewhere in the line.
    assert!(
        emitted_line.contains("\"role\":\"silas\""),
        "nudge.emitted must carry sender as role: {}", emitted_line
    );
    assert!(
        emitted_line.contains("to=wren"),
        "nudge.emitted must name target role: {}", emitted_line
    );
    assert!(
        emitted_line.contains("trace=ntr-"),
        "nudge.emitted must include trace id: {}", emitted_line
    );
    assert!(
        emitted_line.contains(&marker),
        "nudge.emitted must include marker in content preview: {}", emitted_line
    );
}
