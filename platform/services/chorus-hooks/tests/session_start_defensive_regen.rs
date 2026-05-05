//! #2731 — CLAUDE.md becomes a derived artifact. SessionStart defensively
//! regenerates `roles/*/CLAUDE.md` from fragments before running
//! `protocol_contract::check`, so the staleness deadlock that paged Jeff
//! 12+ times in a week (2026-04-28 → 2026-05-04) becomes structurally
//! impossible: regen always produces a CLAUDE.md whose fragment-hash stamp
//! matches the live fragment set.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use chorus_hooks::shared::state_paths::chorus_root;

const INIT_DIR: &str = "/tmp/claude-session-init";
const TEST_ROLE: &str = "wren";

fn skip_unless_integration(reason: &str) -> bool {
    if std::env::var("RUN_INTEGRATION").is_err() {
        eprintln!("SKIP: axis-4 — {reason} (set RUN_INTEGRATION=1 to run)");
        return true;
    }
    false
}

struct GateGuard {
    pending: PathBuf,
    done: PathBuf,
    had_pending: bool,
    had_done: bool,
}

impl GateGuard {
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
    fn clear(&self) {
        let _ = fs::create_dir_all(INIT_DIR);
        let _ = fs::remove_file(&self.pending);
        let _ = fs::remove_file(&self.done);
    }
}

impl Drop for GateGuard {
    fn drop(&mut self) {
        if self.had_pending { let _ = fs::write(&self.pending, ""); }
        else { let _ = fs::remove_file(&self.pending); }
        if self.had_done { let _ = fs::write(&self.done, ""); }
        else { let _ = fs::remove_file(&self.done); }
    }
}

fn read_stamp(role: &str) -> Option<String> {
    let path = format!("{}/roles/{}/CLAUDE.md", chorus_root(), role);
    let body = fs::read_to_string(&path).ok()?;
    for line in body.lines().take(20) {
        if let Some(rest) = line.trim().strip_prefix("<!-- role-fragments: sha256=") {
            return Some(rest.trim_end_matches("-->").trim().to_string());
        }
    }
    None
}

fn poison_stamp(role: &str) -> String {
    let path = format!("{}/roles/{}/CLAUDE.md", chorus_root(), role);
    let body = fs::read_to_string(&path).expect("CLAUDE.md exists for test role");
    let original = read_stamp(role).expect("CLAUDE.md has role-fragments stamp");
    let poisoned = body.replace(
        &format!("role-fragments: sha256={}", original),
        "role-fragments: sha256=deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
    );
    fs::write(&path, poisoned).unwrap();
    original
}

/// AC4 — SessionStart regenerates CLAUDE.md before the protocol check, so
/// a stale CLAUDE.md no longer wedges the gate.
#[test]
fn session_start_regen_heals_stale_claudemd() {
    if skip_unless_integration("invokes chorus-hook-shim binary + mutates roles/<role>/CLAUDE.md") { return; }

    let g = GateGuard::new(TEST_ROLE);
    g.clear();

    let live_stamp = poison_stamp(TEST_ROLE);
    let stamp_after_poison = read_stamp(TEST_ROLE).unwrap();
    assert!(
        stamp_after_poison.starts_with("deadbeef"),
        "precondition: stamp must be poisoned before session-start runs"
    );

    let shim = format!(
        "{}/platform/services/chorus-hooks/target/release/chorus-hook-shim",
        chorus_root()
    );
    let out = Command::new(&shim)
        .args(["session-start", TEST_ROLE])
        .output()
        .expect("chorus-hook-shim must be built; run platform/scripts/build-signed.sh chorus-hooks");
    assert!(out.status.success(), "session-start failed: {:?}", out);

    assert!(
        g.done.exists(),
        "AC4: SessionStart must write .done after defensive regen — gate must not deadlock on staleness"
    );
    let stamp_after_boot = read_stamp(TEST_ROLE).unwrap();
    assert_eq!(
        stamp_after_boot, live_stamp,
        "AC4: regen must restore the role-fragments stamp to the live hash"
    );
}

/// AC2 — `claudemd-gen` rejects per-role generate. The canonical write
/// operation is "regen all three roles atomically." Read-only modes
/// keep per-role narrowing.
#[test]
fn claudemd_gen_rejects_per_role_write() {
    if skip_unless_integration("invokes claudemd-gen binary") { return; }

    let script = format!("{}/platform/scripts/claudemd-gen", chorus_root());
    let out = Command::new(&script)
        .arg("wren")
        .output()
        .expect("claudemd-gen must be present");
    assert!(!out.status.success(), "AC2: per-role generate must NOT succeed");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("per-role generate is rejected"),
        "AC2: rejection error must explain why; got stderr: {}",
        stderr
    );
}

/// AC2 negative — read-only modes still accept per-role narrowing.
#[test]
fn claudemd_gen_allows_per_role_check() {
    if skip_unless_integration("invokes claudemd-gen --check binary") { return; }

    let script = format!("{}/platform/scripts/claudemd-gen", chorus_root());
    let out = Command::new(&script)
        .args(["--check", "wren"])
        .output()
        .expect("claudemd-gen must be present");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("per-role generate is rejected"),
        "AC2: --check is read-only; per-role narrowing must remain allowed. stderr: {}",
        stderr
    );
}
