// @test-type: unit — what the sweep keeps, ends and never closes, against a fake door and a real ps
// @domain: identity
//! #4948 AC1 (and the sweep half of AC3) — Jeff 2026-11-10 14:34: "i want a session that is effectively bounded by
//! chorus-principal login and logout". Measured 14:32: every sweep said "0 live
//! session(s) kept open" because it read each role's login.json from Jeff's home,
//! while the real one is in the role's own 700 home (Silas, pair 4948). The sweep now
//! reads no role files: liveness comes from the session registry's pids, runs from
//! the door. A crash ends the RUN (endReason=crash); a session ends only at logout.
mod common_4944;
use common_4944::*;
use std::path::Path;

const OLD: &str = "2026-11-09T16:18:51Z";

fn runs(role: &str, started: &str) -> String {
    format!(r#"{{"data":[{{"name":"{role}-run-1","ownedBy":"principal-{role}","runOf":"session-{role}-s-1","startedAt":"{started}","runEndedAt":""}}]}}"#)
}

fn register(home: &Path, role: &str, pid: u32) {
    let d = home.join(".chorus/sessions");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join(format!("{role}-{pid}.json")), format!(r#"{{"role":"{role}","pid":{pid},"tty":"/dev/ttys001","host":"tmux"}}"#)).unwrap();
}

/// A pid no process has: ps -p answers non-zero.
const DEAD: u32 = 99_999_999;


#[test]
fn ac1_a_running_role_is_kept_without_reading_any_role_file() {
    let home = tempdir("4948-kept");
    door_with_runs(&home, "wren", "200", &runs("wren", OLD));
    register(&home, "wren", std::process::id());
    // no identity dir at all: the sweep must not need one
    let (code, out) = sweep(&home);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("1 live session(s) kept open"), "{out}");
}

/// NEGATIVE PROOF: no registry dir is a named failure, and nothing is counted kept or ended.
#[test]
fn ac1_negative_an_unreadable_registry_keeps_nothing_and_ends_nothing() {
    let home = tempdir("4948-noreg");
    door_with_runs(&home, "wren", "200", &runs("wren", OLD));
    let (code, out) = sweep(&home);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("0 live session(s) kept open"), "{out}");
    assert!(out.contains("registry"), "the failure names the registry: {out}");
    assert!(!read(home.join("calls.log")).contains("sessionruns"), "no run ended blind: {}", read(home.join("calls.log")));
}

/// NEGATIVE PROOF: a run whose process is alive is never ended.
/// Silas (pair 4948): a run younger than the grace, before its registry file exists, is not ended.
/// AC3 — the sweep never closes an agent's session for its expiry: only logout ends it.
/// (Before #4948 the expired_row fixture was closed by PATCH here.)
#[test]
fn ac3_an_expired_agent_session_is_never_closed_by_the_sweep() {
    let home = tempdir("4948-noclose");
    door_with_runs(&home, "wren", "200", r#"{"data":[]}"#);
    register(&home, "wren", DEAD);
    let (code, out) = sweep(&home);
    assert_eq!(code, 0, "{out}");
    assert!(!read(home.join("calls.log")).contains("identity/sessions/"), "{}", read(home.join("calls.log")));
}
