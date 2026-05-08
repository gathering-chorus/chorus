//! #2475 thread 3 — nudge.requested carries origin=cli|mcp|http so audit can
//! distinguish how the nudge was sent. Default cli when invoked from the bash
//! CLI; callers (MCP server, future Rust MCP client) override via the
//! CHORUS_NUDGE_ORIGIN env var.
//!
//! Hermetic — uses CHORUS_LOG_FILE override + CHORUS_INJECT_DRY_RUN to keep
//! the test from touching the live spine or actually delivering anything.

use std::fs;

const SHIM: &str = env!("CARGO_BIN_EXE_chorus-hook-shim");

fn read_log(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

fn run_nudge(env: &[(&str, &str)]) -> std::process::Output {
    // CHORUS_INJECT_DRY_RUN env skips osascript delivery but lets the
    // chorus_log emit fire (which is what we're testing). The CLI --dry-run
    // flag, by contrast, short-circuits before the emit — wrong for this test.
    let mut cmd = std::process::Command::new(SHIM);
    cmd.args(["nudge", "wren", "test message", "--force"]);
    cmd.env("CHORUS_INJECT_DRY_RUN", "1");
    cmd.env("DEPLOY_ROLE", "silas");
    for (k, v) in env { cmd.env(k, v); }
    cmd.output().expect("shim should run")
}

/// Default invocation (no override) tags origin=cli.
#[test]
fn nudge_default_origin_is_cli() {
    let tmp = tempfile::tempdir().unwrap();
    let log = tmp.path().join("chorus.log");
    let log_str = log.to_str().unwrap().to_string();

    let out = run_nudge(&[("CHORUS_LOG_FILE", &log_str)]);
    assert!(out.status.success(), "shim should succeed: {}", String::from_utf8_lossy(&out.stderr));

    let content = read_log(&log_str);
    assert!(
        content.contains("nudge.requested"),
        "nudge.requested event must be logged. Got: {}", content
    );
    // Migration-window dual-emit (Silas ops review 2026-05-07): both events
    // fire while readers still reference nudge.emitted. Drop nudge.emitted
    // assertion when the cleanup card retires the dual-emit.
    assert!(
        content.contains("nudge.emitted"),
        "nudge.emitted event must also be logged during migration window. Got: {}", content
    );
    assert!(
        content.contains("origin=cli"),
        "default origin must be 'cli' (no env override). Got: {}", content
    );
}

/// CHORUS_NUDGE_ORIGIN=mcp tags the spine event accordingly.
#[test]
fn nudge_origin_mcp_when_env_set() {
    let tmp = tempfile::tempdir().unwrap();
    let log = tmp.path().join("chorus.log");
    let log_str = log.to_str().unwrap().to_string();

    let out = run_nudge(&[
        ("CHORUS_LOG_FILE", &log_str),
        ("CHORUS_NUDGE_ORIGIN", "mcp"),
    ]);
    assert!(out.status.success());

    let content = read_log(&log_str);
    assert!(content.contains("nudge.requested"));
    assert!(
        content.contains("origin=mcp"),
        "CHORUS_NUDGE_ORIGIN=mcp must be reflected in spine event. Got: {}", content
    );
    assert!(
        !content.contains("origin=cli"),
        "must not also tag origin=cli when MCP override is set"
    );
}

/// Unknown origin values still pass through (validation happens at consumer).
#[test]
fn nudge_origin_passes_through_unknown_value() {
    let tmp = tempfile::tempdir().unwrap();
    let log = tmp.path().join("chorus.log");
    let log_str = log.to_str().unwrap().to_string();

    let out = run_nudge(&[
        ("CHORUS_LOG_FILE", &log_str),
        ("CHORUS_NUDGE_ORIGIN", "futureclient"),
    ]);
    assert!(out.status.success());

    let content = read_log(&log_str);
    assert!(content.contains("origin=futureclient"));
}
