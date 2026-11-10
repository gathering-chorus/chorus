// @test-type: unit — which runs the sweep ends as a crash, against a fake door and a real ps
// @domain: identity
//! #4948 AC2 — Jeff 2026-11-10 14:34: "i want a session that is effectively bounded by
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

fn now_iso() -> String {
    let out = std::process::Command::new("date").args(["-u", "+%Y-%m-%dT%H:%M:%SZ"]).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn ac2_a_run_whose_process_is_gone_ends_as_a_crash_and_its_session_stays_open() {
    let home = tempdir("4948-crash");
    door_with_runs(&home, "wren", "200", &runs("wren", OLD));
    register(&home, "wren", DEAD); // a stale registry file: its pid is dead (Silas: silas-79380.json)
    let (code, out) = sweep(&home);
    let calls = read(home.join("calls.log"));
    assert_eq!(code, 0, "{out}\n{calls}");
    let line = calls.lines().find(|l| l.contains("identity/sessionruns/wren-run-1")).unwrap_or_else(|| panic!("run not ended: {out}\n{calls}"));
    assert!(line.starts_with("PATCH "), "{line}");
    let body: serde_json::Value = serde_json::from_str(line.splitn(3, ' ').nth(2).unwrap()).unwrap();
    assert_eq!(body["endReason"], "crash", "{body}");
    assert!(body["runEndedAt"].as_str().unwrap_or("").ends_with('Z'), "{body}");
    assert_eq!(body.as_object().unwrap().len(), 2, "only the two fields: {body}");
    assert!(!calls.contains("identity/sessions/"), "a crash never closes the session: {calls}");
}

#[test]
fn ac2_negative_a_live_run_is_never_ended() {
    let home = tempdir("4948-alive");
    door_with_runs(&home, "wren", "200", &runs("wren", OLD));
    register(&home, "wren", DEAD);
    register(&home, "wren", std::process::id()); // one live file beside a stale one
    let (code, out) = sweep(&home);
    assert_eq!(code, 0, "{out}");
    assert!(!read(home.join("calls.log")).contains("sessionruns"), "{}", read(home.join("calls.log")));
}

#[test]
fn ac2_a_run_younger_than_the_grace_is_not_ended() {
    let home = tempdir("4948-young");
    door_with_runs(&home, "wren", "200", &runs("wren", &now_iso()));
    register(&home, "wren", DEAD);
    let (code, out) = sweep(&home);
    assert_eq!(code, 0, "{out}");
    assert!(!read(home.join("calls.log")).contains("sessionruns"), "{}", read(home.join("calls.log")));
}
