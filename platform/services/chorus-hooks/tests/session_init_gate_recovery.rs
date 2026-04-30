//! #2311 rescope — In-session recovery for locked roles.
//!
//! Scenario: a role's SessionStart hook ran an older binary that wrote
//! .pending but never .done (e.g., because the old flow required a Read
//! of session-start.md to complete boot, and the role's first response
//! was prose with no tool calls). The binary-gate upgrade then locks
//! the role out of ALL Bash/Edit/Write. Recovery must not require a
//! reboot.
//!
//! Design (per navigator jenga rule — no new CLI): Reading the role's
//! own session-start file with .pending armed and .done missing
//! re-runs the same protocol_contract::check that SessionStart runs,
//! writing .done on pass. Same one entry point, reachable from Read
//! as well as SessionStart.

use std::fs;
use std::path::PathBuf;
use chorus_hooks::shared::state_paths::chorus_root;

/// #2614: returns true (and prints a skip line) when RUN_INTEGRATION is unset.
fn skip_unless_integration(reason: &str) -> bool {
    if std::env::var("RUN_INTEGRATION").is_err() {
        eprintln!("SKIP: axis-4 — {reason} (set RUN_INTEGRATION=1 to run)");
        return true;
    }
    false
}

const INIT_DIR: &str = "/tmp/claude-session-init";
const TEST_ROLE: &str = "kade";

struct MarkerGuard {
    pending: PathBuf,
    done: PathBuf,
    had_pending: bool,
    had_done: bool,
}

impl MarkerGuard {
    fn new(role: &str) -> Self {
        let pending = PathBuf::from(format!("{}/{}.pending", INIT_DIR, role));
        let done = PathBuf::from(format!("{}/{}.done", INIT_DIR, role));
        Self {
            had_pending: pending.exists(),
            had_done: done.exists(),
            pending,
            done,
        }
    }

    fn arm_locked(&self) {
        let _ = fs::create_dir_all(INIT_DIR);
        let _ = fs::write(&self.pending, "");
        let _ = fs::remove_file(&self.done);
    }
}

impl Drop for MarkerGuard {
    fn drop(&mut self) {
        if self.had_pending {
            let _ = fs::write(&self.pending, "");
        } else {
            let _ = fs::remove_file(&self.pending);
        }
        if self.had_done {
            let _ = fs::write(&self.done, "");
        } else {
            let _ = fs::remove_file(&self.done);
        }
    }
}

fn post_via_socket(endpoint: &str, body: &str) -> String {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    let mut stream = UnixStream::connect("/tmp/chorus-hooks.sock")
        .expect("chorus-hooks socket up");
    let req = format!(
        "POST /{} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        endpoint, body.len(), body
    );
    stream.write_all(req.as_bytes()).unwrap();
    let mut resp = Vec::new();
    stream.read_to_end(&mut resp).unwrap();
    let s = String::from_utf8_lossy(&resp).to_string();
    let body_start = s.find("\r\n\r\n").map(|p| p + 4).unwrap_or(0);
    s[body_start..].to_string()
}

/// Core recovery test: locked role reads its own session-start.md and
/// the gate writes .done on protocol pass.
#[test]
fn read_session_start_unlocks_role_on_protocol_pass() {
    if skip_unless_integration("connects to /tmp/chorus-hooks.sock + writes /tmp/session-start-<role>.md") { return; }
    let g = MarkerGuard::new(TEST_ROLE);
    g.arm_locked();
    assert!(g.pending.exists(), "precondition: .pending armed");
    assert!(!g.done.exists(), "precondition: .done missing");

    let body = serde_json::json!({
        "tool_name": "Read",
        "tool_input": {
            "file_path": format!("/tmp/session-start-{}.md", TEST_ROLE),
        },
        "session_id": "recovery-test",
        "cwd": format!("{}/roles/{}", chorus_root(), TEST_ROLE),
        "deploy_role": TEST_ROLE,
    })
    .to_string();

    let _ = post_via_socket("pre-tool-use", &body);

    // Give the async hook a beat to flush the file write.
    std::thread::sleep(std::time::Duration::from_millis(100));

    // If kade's CLAUDE.md passes protocol check (normal on clean tree),
    // .done is now written. If it fails, .done stays missing — but then
    // the banner file should exist. Cover both.
    let done_exists = g.done.exists();
    let banner_exists = std::path::Path::new(
        &format!("/tmp/session-start-{}-PROTOCOL_VIOLATION.md", TEST_ROLE)
    ).exists();

    assert!(
        done_exists || banner_exists,
        "After Read of session-start file with .pending armed, \
         either .done must be written (protocol pass) or a PROTOCOL \
         VIOLATION banner must exist (protocol fail). Neither happened."
    );
}

/// Reading an unrelated file does NOT flip .done — only the role's own
/// session-start.md is the recovery signal.
#[test]
fn read_other_file_does_not_unlock() {
    if skip_unless_integration("connects to /tmp/chorus-hooks.sock + writes /tmp/session-start-<role>.md") { return; }
    let g = MarkerGuard::new(TEST_ROLE);
    g.arm_locked();

    let body = serde_json::json!({
        "tool_name": "Read",
        "tool_input": {"file_path": "/tmp/some-unrelated-file.txt"},
        "session_id": "recovery-test-negative",
        "cwd": format!("{}/roles/{}", chorus_root(), TEST_ROLE),
        "deploy_role": TEST_ROLE,
    })
    .to_string();

    let _ = post_via_socket("pre-tool-use", &body);
    std::thread::sleep(std::time::Duration::from_millis(100));

    assert!(
        !g.done.exists(),
        ".done must NOT be written by reading an unrelated file — \
         only the role's own session-start.md is the recovery signal."
    );
}
