//! #3700 (kade half) — self-healing session registry + turn-state (busy)
//! tracking. Red-first against the pair-agreed decision table (Silas
//! navigation, 2026-07-26):
//!
//!   - re-register is SELF-HEALING: any hook dispatch heals a missing or
//!     dead-pid entry (covers boot, compaction, sweep, hand-rm — one
//!     idempotent mechanism, no event-chasing)
//!   - busy = daemon-tracked turn state: marked at prompt-submit, cleared at
//!     Stop; STALENESS BOUND (30 min, no subsequent activity) so a crash
//!     mid-turn cannot queue nudges forever; pid-dead beats busy always
//!   - decision table for delivery (consumed by pulse via the state file):
//!       BUSY  → queue (drain at the target's own turn boundary)
//!       IDLE  → inject now (nudge-as-wake is the point — pull-only would
//!               let a queued nudge for an idle session sleep forever)
//!       DEAD  → typed undelivered, never name-match
//!
//! Hermetic: every test gets its own tmp dir for registry + turn-state; no
//! live ~/.chorus, no live role identifiers (test-role names only).

use chorus_hooks::shared::session_health::{
    busy_state, clear_busy, ensure_registered_in, mark_busy, BusyState,
};
use std::fs;

fn tmpdir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("s3700-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

const ROLE: &str = "testrole";

// ── self-healing registration ───────────────────────────────────────────────

#[test]
fn ensure_registered_writes_entry_when_missing() {
    let dir = tmpdir("missing");
    let healed = ensure_registered_in(&dir, ROLE, std::process::id(), "/dev/ttys099", "terminal");
    assert!(healed, "missing entry must be written");
    let entries: Vec<_> = fs::read_dir(&dir).unwrap().collect();
    assert_eq!(entries.len(), 1, "one registration file");
    let body = fs::read_to_string(entries[0].as_ref().unwrap().path()).unwrap();
    assert!(body.contains(ROLE), "entry carries the role: {}", body);
}

#[test]
fn ensure_registered_replaces_dead_pid_entry() {
    let dir = tmpdir("stale");
    // a registry entry whose pid is certainly dead — an impossible pid
    fs::write(
        dir.join(format!("{}-99999999.json", ROLE)),
        format!(r#"{{"role":"{}","pid":99999999,"tty":"/dev/ttys001","host":"terminal"}}"#, ROLE),
    )
    .unwrap();
    let healed = ensure_registered_in(&dir, ROLE, std::process::id(), "/dev/ttys099", "terminal");
    assert!(healed, "dead-pid entry must be replaced");
    let names: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert!(
        names.iter().any(|n| n.contains(&std::process::id().to_string())),
        "live-pid entry present: {:?}",
        names
    );
    assert!(
        !names.iter().any(|n| n.contains("99999999")),
        "dead entry pruned: {:?}",
        names
    );
}

#[test]
fn ensure_registered_is_a_noop_when_entry_live() {
    let dir = tmpdir("live");
    ensure_registered_in(&dir, ROLE, std::process::id(), "/dev/ttys099", "terminal");
    let before = fs::read_dir(&dir).unwrap().count();
    let healed = ensure_registered_in(&dir, ROLE, std::process::id(), "/dev/ttys099", "terminal");
    assert!(!healed, "live entry must not be rewritten (idempotent no-op)");
    assert_eq!(fs::read_dir(&dir).unwrap().count(), before);
}

// ── busy lifecycle + decision table ─────────────────────────────────────────

#[test]
fn busy_lifecycle_marks_and_clears() {
    let dir = tmpdir("busy");
    ensure_registered_in(&dir, ROLE, std::process::id(), "/dev/ttys099", "terminal");
    let now = 1_000_000u64;
    mark_busy(&dir, ROLE, std::process::id(), now);
    assert_eq!(
        busy_state(&dir, ROLE, now + 60),
        BusyState::Busy,
        "mid-turn = busy"
    );
    clear_busy(&dir, ROLE);
    assert_eq!(
        busy_state(&dir, ROLE, now + 120),
        BusyState::Idle,
        "after Stop = idle (inject-now path — nudge-as-wake preserved)"
    );
}

#[test]
fn busy_goes_stale_after_bound_so_a_crash_cannot_queue_forever() {
    let dir = tmpdir("stalebusy");
    ensure_registered_in(&dir, ROLE, std::process::id(), "/dev/ttys099", "terminal");
    let now = 1_000_000u64;
    mark_busy(&dir, ROLE, std::process::id(), now);
    // 29 min: still busy (long herds are real — a 10-min turn + margin)
    assert_eq!(busy_state(&dir, ROLE, now + 29 * 60), BusyState::Busy);
    // past the 30-min bound with no activity: treated as NOT busy
    assert_eq!(
        busy_state(&dir, ROLE, now + 31 * 60),
        BusyState::Idle,
        "stale busy must decay to idle"
    );
}

#[test]
fn dead_pid_beats_busy_flag_always() {
    let dir = tmpdir("deadbusy");
    // entry + busy marker both point at a dead pid
    fs::write(
        dir.join(format!("{}-99999999.json", ROLE)),
        format!(r#"{{"role":"{}","pid":99999999,"tty":"/dev/ttys001","host":"terminal"}}"#, ROLE),
    )
    .unwrap();
    let now = 1_000_000u64;
    mark_busy(&dir, ROLE, 99999999, now);
    assert_eq!(
        busy_state(&dir, ROLE, now + 10),
        BusyState::Dead,
        "dead pid wins over busy — typed undelivered, never name-match"
    );
}

// ── taxonomy: dead ≠ unregistered (truth-telling — WAS here vs never was) ──

#[test]
fn no_entry_at_all_is_unregistered() {
    let dir = tmpdir("unreg");
    assert_eq!(
        busy_state(&dir, ROLE, 1_000_000),
        BusyState::Unregistered,
        "never registered = Unregistered (typed) — the null→name-match entry point closes"
    );
}

#[test]
fn entry_with_dead_pid_is_dead_not_unregistered() {
    let dir = tmpdir("deadentry");
    fs::write(
        dir.join(format!("{}-99999999.json", ROLE)),
        format!(r#"{{"role":"{}","pid":99999999,"tty":"/dev/ttys001","host":"terminal"}}"#, ROLE),
    )
    .unwrap();
    assert_eq!(
        busy_state(&dir, ROLE, 1_000_000),
        BusyState::Dead,
        "entry exists but pid gone = Dead (the role WAS here) — distinct report string"
    );
}

// ── role derivation for the shim wiring (pure, unit-pinned) ────────────────

#[test]
fn derive_role_reads_deploy_role_then_cwd_then_werk_slot() {
    use chorus_hooks::shared::session_health::derive_role;
    assert_eq!(
        derive_role(r#"{"deploy_role":"testrole","cwd":"/x"}"#),
        Some("testrole".to_string()),
        "deploy_role wins"
    );
    assert_eq!(
        derive_role(r#"{"cwd":"/Users/x/CascadeProjects/chorus/roles/wren"}"#),
        Some("wren".to_string()),
        "role dir cwd"
    );
    assert_eq!(
        derive_role(r#"{"cwd":"/Users/x/CascadeProjects/chorus-werk/silas-3700/platform"}"#),
        Some("silas".to_string()),
        "werk slot cwd"
    );
    assert_eq!(derive_role(r#"{"cwd":"/tmp/elsewhere"}"#), None, "unknown = None, never guessed");
}
