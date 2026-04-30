//! #2078 — git detection heuristic.
//!
//! Bug 2: git detection assumes team repo when no cd command present.
//!
//! (Bug 1 — #2078 AppleScript single-quote escaping — moved to a unit test
//! on `escape_for_applescript` in chorus-inject/src/main.rs per #2166.)
//!
//! #2311 rescope: session_init_gate is now binary — when .pending exists
//! and .done does not, ALL Bash is denied. Tests that exercise downstream
//! hooks (like infra_guardrails / git-queue) must first pass the init
//! gate by ensuring .done is present for the test's role. `MarkerGuard`
//! snapshots and restores the role's real marker state.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use serde_json::json;

/// #2614: returns true (and prints a skip line) when RUN_INTEGRATION is unset.
fn skip_unless_integration(reason: &str) -> bool {
    if std::env::var("RUN_INTEGRATION").is_err() {
        eprintln!("SKIP: axis-4 — {reason} (set RUN_INTEGRATION=1 to run)");
        return true;
    }
    false
}

const SHIM: &str = env!("CARGO_BIN_EXE_chorus-hook-shim");
const INIT_DIR: &str = "/tmp/claude-session-init";

struct MarkerGuard {
    pending: PathBuf,
    done: PathBuf,
    had_pending: bool,
    had_done: bool,
}

impl MarkerGuard {
    fn ensure_done(role: &str) -> Self {
        let pending = PathBuf::from(format!("{}/{}.pending", INIT_DIR, role));
        let done = PathBuf::from(format!("{}/{}.done", INIT_DIR, role));
        let had_pending = pending.exists();
        let had_done = done.exists();
        let _ = fs::create_dir_all(INIT_DIR);
        let _ = fs::write(&done, "");
        Self { pending, done, had_pending, had_done }
    }
}

impl Drop for MarkerGuard {
    fn drop(&mut self) {
        if self.had_pending {
            let _ = fs::write(&self.pending, "");
        }
        if self.had_done {
            let _ = fs::write(&self.done, "");
        } else {
            let _ = fs::remove_file(&self.done);
        }
    }
}

// === Bug 2: git detection ===

#[test]
fn git_commit_allowed_outside_team_repo() {
    if skip_unless_integration("exercises real git/nudge with role names") { return; }
    let _guard = MarkerGuard::ensure_done("kade");
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
    if skip_unless_integration("exercises real git/nudge with role names") { return; }
    let _guard = MarkerGuard::ensure_done("kade");
    // git commit inside the team repo SHOULD be blocked. The server's
    // infra_guardrails compares cwd to chorus_root() (env-aware via
    // shared::state_paths). Compute cwd the same way so it matches on both
    // local Mac and Linux CI: CHORUS_ROOT env var with hardcoded Mac fallback.
    let chorus_root = std::env::var("CHORUS_ROOT")
        .ok()
        .filter(|s| !s.is_empty())
        .expect("CHORUS_ROOT must be set and non-empty");
    let cwd = format!("{}/roles/kade", chorus_root);
    let hook_input = json!({
        "tool_name": "Bash",
        "tool_input": {"command": "git commit -m 'test'"},
        "session_id": "test-session",
        "cwd": cwd
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
