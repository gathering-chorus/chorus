//! #2731 — CLAUDE.md becomes a derived artifact. SessionStart defensively
//! regenerates `roles/*/CLAUDE.md` from fragments, so the staleness deadlock
//! that paged Jeff 12+ times in a week (2026-04-28 → 2026-05-04) becomes
//! structurally impossible: regen always rewrites CLAUDE.md from the live
//! fragment set. (#3288: the stamp-compare that used to run after regen is
//! retired — regen itself is the coherence mechanism, so these tests poison
//! the file body and assert regen heals it.)

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

const POISON_LINE: &str = "<!-- POISON-3288: hand-edit that regen must erase -->";

fn poison_body(role: &str) {
    let path = format!("{}/roles/{}/CLAUDE.md", chorus_root(), role);
    let body = fs::read_to_string(&path).expect("CLAUDE.md exists for test role");
    fs::write(&path, format!("{}\n{}", POISON_LINE, body)).unwrap();
}

fn body_is_poisoned(role: &str) -> bool {
    let path = format!("{}/roles/{}/CLAUDE.md", chorus_root(), role);
    fs::read_to_string(&path)
        .map(|b| b.contains(POISON_LINE))
        .unwrap_or(false)
}

/// AC4 — SessionStart regenerates CLAUDE.md from fragments, so a hand-edited
/// (stale) CLAUDE.md is healed at boot and the gate cannot wedge.
#[test]
fn session_start_regen_heals_stale_claudemd() {
    if skip_unless_integration("invokes chorus-hook-shim binary + mutates roles/<role>/CLAUDE.md") { return; }

    let g = GateGuard::new(TEST_ROLE);
    g.clear();

    poison_body(TEST_ROLE);
    assert!(
        body_is_poisoned(TEST_ROLE),
        "precondition: CLAUDE.md must be poisoned before session-start runs"
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
    assert!(
        !body_is_poisoned(TEST_ROLE),
        "AC4: regen must rewrite CLAUDE.md from fragments, erasing the hand-edit"
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
