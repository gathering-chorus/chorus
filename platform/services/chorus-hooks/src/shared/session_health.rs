//! #3700 (kade half) — self-healing session registration + turn-state.
//!
//! Lives lib-side (shared/) because BOTH daemon hook dispatch (busy marking,
//! self-heal on any tool call) and the shim's session commands consume it.
//!
//! Two mechanisms, one decision table:
//!
//! 1. SELF-HEALING REGISTRATION — `ensure_registered_in` heals a missing or
//!    dead-pid registry entry on any dispatch. One idempotent mechanism
//!    covers every way an entry goes dark (boot, compaction, sweep, hand-rm)
//!    instead of chasing each lifecycle event. Registry entries keep the
//!    #3125 shape (`<role>-<pid>.json`, {role,pid,tty,host,registered_at}).
//!
//! 2. TURN STATE — the daemon marks a role busy at prompt-submit and clears
//!    at Stop (`<role>.turn.json` beside the registry entries). `busy_state`
//!    is the delivery decision table pulse consumes:
//!    Busy → queue (drain at the target's own turn boundary)
//!    Idle → inject now (nudge-as-wake preserved — a queued nudge for an
//!    idle session must never sleep forever)
//!    Dead → typed undelivered; the null→name-match fallback entry point
//!    closes (the #3700 spray class)
//!    Guards (Silas navigation 2026-07-26): busy older than BUSY_STALE_SECS
//!    with no activity decays to Idle (a crash mid-turn cannot queue
//!    forever); a dead pid beats the busy flag always.

use std::path::Path;

/// Crash-guard: a busy marker older than this with no subsequent activity is
/// treated as not-busy. 30 min = the longest observed real turns (~10 min
/// herds) with generous margin.
pub const BUSY_STALE_SECS: u64 = 30 * 60;

/// The pair-agreed cross-seam taxonomy (kade Rust ↔ silas TS, 2026-07-26):
/// busy | idle | dead | unregistered. dead ≠ unregistered is truth-telling —
/// "the role WAS here (entry, pid gone)" vs "never was" — even though both
/// route to typed-undelivered, never name-match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusyState {
    /// Registered, alive, mid-turn → queue, drain at its turn boundary.
    Busy,
    /// Registered, alive, not mid-turn → inject now (wake).
    Idle,
    /// Entry exists but its pid is gone → typed undelivered (was here).
    Dead,
    /// No entry at all → typed undelivered (never registered).
    Unregistered,
}

fn pid_alive(pid: u32) -> bool {
    // signal 0 = existence probe, no signal delivered
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

/// Public probe for the shim wiring: does the role have a live entry?
pub fn has_live_entry(dir: &Path, role: &str) -> bool {
    live_entry(dir, role).is_some()
}

/// The role's live registry entry `<role>-<pid>.json`, if its pid is alive.
fn live_entry(dir: &Path, role: &str) -> Option<(std::path::PathBuf, u32)> {
    let prefix = format!("{}-", role);
    let entries = std::fs::read_dir(dir).ok()?;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !name.starts_with(&prefix) || !name.ends_with(".json") {
            continue;
        }
        if let Ok(pid) = name[prefix.len()..name.len() - 5].parse::<u32>() {
            if pid_alive(pid) {
                return Some((e.path(), pid));
            }
        }
    }
    None
}

/// Heal the role's registration in `dir`: write `<role>-<pid>.json` when no
/// live entry exists, pruning any dead-pid entries for the role. Returns true
/// when something was healed, false on the idempotent no-op path.
pub fn ensure_registered_in(dir: &Path, role: &str, pid: u32, tty: &str, host: &str) -> bool {
    if live_entry(dir, role).is_some() {
        return false;
    }
    // prune the role's dead entries (never another role's files)
    let prefix = format!("{}-", role);
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with(&prefix) && name.ends_with(".json") && !name.ends_with(".turn.json")
            {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let body = format!(
        r#"{{"role":"{}","pid":{},"tty":"{}","host":"{}","registered_at":{},"healed":true}}"#,
        role, pid, tty, host, now
    );
    let _ = std::fs::create_dir_all(dir);
    std::fs::write(dir.join(format!("{}-{}.json", role, pid)), body).is_ok()
}

fn turn_path(dir: &Path, role: &str) -> std::path::PathBuf {
    dir.join(format!("{}.turn.json", role))
}

/// Derive the acting role from a hook-input JSON line: `deploy_role` field
/// wins, then a `roles/<role>` cwd, then a `chorus-werk/<role>-<card>` werk
/// slot. None when undeterminable — the caller must NEVER guess (a wrong
/// role here would heal the registry with a lie).
pub fn derive_role(input_json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(input_json).ok()?;
    if let Some(r) = v.get("deploy_role").and_then(|x| x.as_str()) {
        if !r.is_empty() {
            return Some(r.to_string());
        }
    }
    let cwd = v.get("cwd").and_then(|x| x.as_str()).unwrap_or("");
    for role in ["wren", "silas", "kade"] {
        if cwd.contains(&format!("/roles/{}", role)) {
            return Some(role.to_string());
        }
        if cwd.contains(&format!("chorus-werk/{}-", role)) {
            return Some(role.to_string());
        }
    }
    // generic fallback for test/fixture roles in role-dir form only
    if let Some(idx) = cwd.find("/roles/") {
        let rest = &cwd[idx + 7..];
        let role = rest.split('/').next().unwrap_or("");
        if !role.is_empty() {
            return Some(role.to_string());
        }
    }
    None
}

/// Mark the role mid-turn (prompt-submit). `now_secs` injected for testability.
pub fn mark_busy(dir: &Path, role: &str, pid: u32, now_secs: u64) {
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(
        turn_path(dir, role),
        format!(r#"{{"role":"{}","pid":{},"busy_since":{}}}"#, role, pid, now_secs),
    );
}

/// Clear the role's turn marker (Stop).
pub fn clear_busy(dir: &Path, role: &str) {
    let _ = std::fs::remove_file(turn_path(dir, role));
}

/// Does the role have ANY registry entry (alive or not)?
fn any_entry(dir: &Path, role: &str) -> bool {
    let prefix = format!("{}-", role);
    std::fs::read_dir(dir)
        .map(|entries| {
            entries.flatten().any(|e| {
                let n = e.file_name().to_string_lossy().to_string();
                n.starts_with(&prefix) && n.ends_with(".json") && !n.ends_with(".turn.json")
            })
        })
        .unwrap_or(false)
}

/// The delivery decision table. Dead-pid beats busy; stale busy decays to Idle.
pub fn busy_state(dir: &Path, role: &str, now_secs: u64) -> BusyState {
    if live_entry(dir, role).is_none() {
        return if any_entry(dir, role) {
            BusyState::Dead
        } else {
            BusyState::Unregistered
        };
    }
    let Ok(body) = std::fs::read_to_string(turn_path(dir, role)) else {
        return BusyState::Idle;
    };
    // minimal field reads — the file is daemon-written, shape-stable
    let busy_since = body
        .split("\"busy_since\":")
        .nth(1)
        .and_then(|r| r.trim_end_matches('}').trim().parse::<u64>().ok());
    let marker_pid = body
        .split("\"pid\":")
        .nth(1)
        .and_then(|r| r.split(',').next())
        .and_then(|s| s.trim().parse::<u32>().ok());
    if let Some(p) = marker_pid {
        if !pid_alive(p) {
            // the turn that set busy is gone — but Dead only if no OTHER live
            // session holds the registration (checked above → here the entry
            // is live, so the stale marker just doesn't count as busy)
            return BusyState::Idle;
        }
    }
    match busy_since {
        Some(t) if now_secs.saturating_sub(t) <= BUSY_STALE_SECS => BusyState::Busy,
        _ => BusyState::Idle,
    }
}
