//! #2311 rescope AC#2 — PreToolUse session_init_gate is binary:
//! .pending exists AND .done missing → deny ALL Write/Edit/Bash.
//! Zero exemptions for TZ=, wall-clock, session-start.sh, chorus-prompt.sh,
//! werk-init.sh, role-state. If boot isn't done, nothing runs.
//!
//! The Read-handler side-effect (protocol check on reading session-start)
//! is also retired — protocol check now happens inline in SessionStart hook
//! (session.rs). Read is plain-allow.
//!
//! Test hermeticity (#2558): each test uses its own tmpdir for the
//! session-init markers and calls `session_init_gate::check_with_dir()`
//! directly — no socket round-trip, no shared filesystem with the live
//! chorus-hook-shim daemon. The earlier integration shape (post to
//! /tmp/chorus-hooks.sock with markers in /tmp/claude-session-init) raced
//! against the daemon's own state writes whenever a same-role Claude
//! session was live on the test machine. Per-test tmpdir eliminates the
//! race by construction; tmpdir is cleaned up automatically on drop.

use chorus_hooks::hooks::session_init_gate;
use chorus_hooks::state::AppState;
use chorus_hooks::types::HookInput;
use chorus_hooks::shared::state_paths::chorus_root;
use std::fs;
use tempfile::TempDir;

/// Build a HookInput for the given role/tool/input. Cwd is set to the
/// role's directory so HookInput::role() resolves via deploy_role.
fn make_input(role: &str, tool: &str, tool_input: serde_json::Value) -> HookInput {
    let cwd = format!("{}/roles/{}", chorus_root(), role);
    serde_json::from_value(serde_json::json!({
        "tool_name": tool,
        "tool_input": tool_input,
        "session_id": format!("session-init-gate-test-{}", role),
        "cwd": cwd,
        "deploy_role": role,
    }))
    .expect("HookInput should deserialize from test fixture")
}

/// Arm the gate: write `<role>.pending`, ensure `<role>.done` is absent.
fn arm_pending_no_done(dir: &std::path::Path, role: &str) {
    let pending = dir.join(format!("{}.pending", role));
    let done = dir.join(format!("{}.done", role));
    fs::write(&pending, "").expect("write pending");
    let _ = fs::remove_file(&done);
}

/// Arm the boot-complete state: both `<role>.pending` and `<role>.done` present.
fn arm_done(dir: &std::path::Path, role: &str) {
    let pending = dir.join(format!("{}.pending", role));
    let done = dir.join(format!("{}.done", role));
    fs::write(&pending, "").expect("write pending");
    fs::write(&done, "").expect("write done");
}

/// Use "kade" throughout for role identity. The gate's role parser only
/// recognizes silas/wren/kade; "unknown" returns allow before reaching the
/// marker check, so synthetic roles can't drive the deny path. Tmpdir
/// isolation makes the live-daemon race irrelevant — the daemon never
/// looks at our tmpdir, only at /tmp/claude-session-init.
const TEST_ROLE: &str = "kade";

async fn expect_deny(role: &str, tool: &str, tool_input: serde_json::Value) {
    let dir = TempDir::new().expect("tmpdir");
    arm_pending_no_done(dir.path(), role);
    let dir_str = dir.path().to_str().expect("tmpdir utf-8");
    let input = make_input(role, tool, tool_input);
    let state = AppState::new();
    let resp = session_init_gate::check_with_dir(&input, &state, dir_str).await;
    let stdout = resp.stdout.as_deref().unwrap_or("");
    assert!(
        stdout.contains("\"deny\"") && stdout.contains("Session init gate"),
        "{} {} should be DENIED by binary session init gate. Got stdout: {}, exit_code: {}",
        role, tool, stdout, resp.exit_code,
    );
}

#[tokio::test]
async fn bash_tz_prefix_denied_when_pending() {
    expect_deny(
        TEST_ROLE,
        "Bash",
        serde_json::json!({"command": "TZ=America/New_York date '+%Y-%m-%d %H:%M'"}),
    ).await;
}

#[tokio::test]
async fn bash_wall_clock_denied_when_pending() {
    expect_deny(
        TEST_ROLE,
        "Bash",
        serde_json::json!({"command": "wall-clock"}),
    ).await;
}

#[tokio::test]
async fn bash_role_state_denied_when_pending() {
    expect_deny(
        TEST_ROLE,
        "Bash",
        serde_json::json!({"command": format!("role-state {} waiting", TEST_ROLE)}),
    ).await;
}

#[tokio::test]
async fn bash_session_start_sh_denied_when_pending() {
    expect_deny(
        TEST_ROLE,
        "Bash",
        serde_json::json!({"command": format!("session-start.sh {}", TEST_ROLE)}),
    ).await;
}

#[tokio::test]
async fn bash_chorus_prompt_sh_denied_when_pending() {
    expect_deny(
        TEST_ROLE,
        "Bash",
        serde_json::json!({"command": format!("chorus-prompt.sh {}", TEST_ROLE)}),
    ).await;
}

#[tokio::test]
async fn bash_werk_init_sh_denied_when_pending() {
    expect_deny(
        TEST_ROLE,
        "Bash",
        serde_json::json!({"command": format!("werk-init.sh {}", TEST_ROLE)}),
    ).await;
}

#[tokio::test]
async fn bash_allowed_when_done_exists() {
    let dir = TempDir::new().expect("tmpdir");
    arm_done(dir.path(), TEST_ROLE);
    let dir_str = dir.path().to_str().expect("tmpdir utf-8");
    let input = make_input(TEST_ROLE, "Bash", serde_json::json!({"command": "ls"}));
    let state = AppState::new();
    let resp = session_init_gate::check_with_dir(&input, &state, dir_str).await;
    let stdout = resp.stdout.as_deref().unwrap_or("");
    assert!(
        !stdout.contains("\"deny\""),
        "Bash should be ALLOWED when .done exists. Got stdout: {}",
        stdout,
    );
}
