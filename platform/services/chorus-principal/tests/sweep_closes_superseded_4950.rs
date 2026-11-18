// @test-type: unit — which pane sessions the sweep closes as superseded, against a fake door
// @domain: identity
//! #4517 AC6 — superseding moved to the door (see logins_4517.rs in athena-make); this file now
//! proves the sweep leaves Session rows alone. Its history:
//! #4950 AC2 — a role has one session, login to logout. On 2026-11-10 every role had two
//! open pane sessions: the 11-08 one stayed open after the 11-09 login (and a seen-write
//! run as Jeff kept bumping its lastSeenAt). The sweep closes a role's older open pane
//! sessions when a newer one exists, newest by issuedAt (required, one per row; never
//! lastSeenAt, which seen-write keeps moving), by PATCH (#4944), with no endedBy: nobody
//! ended it, and endedBy must name a Principal (prod 11-11 08:27: 5 x 422 on "superseded").
//! The spine's session.superseded by=sweep records why.
//! It never closes the newest one, one an open run points at, another role's, or a browser
//! session.
mod common_4944;
use common_4944::*;

fn sess(name: &str, role: &str, channel: &str, issued: &str, seen: &str) -> String {
    format!(r#"{{"name":"{name}","ownedBy":"principal-{role}","actsAs":"role-{role}","channel":"{channel}","sessionState":"open","issuedAt":"{issued}","lastSeenAt":"{seen}","tokenId":"t"}}"#)
}

/// A live registry file for `role` (this test's own pid), so the sweep can read liveness.
fn live(home: &std::path::Path, role: &str) {
    let d = home.join(".chorus/sessions");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join(format!("{role}-{}.json", std::process::id())),
        format!(r#"{{"role":"{role}","pid":{},"tty":"/dev/ttys001","host":"tmux"}}"#, std::process::id())).unwrap();
}

fn closes(calls: &str) -> Vec<String> {
    calls.lines().filter(|l| l.contains("/identity/sessions/")).map(|l| l.to_string()).collect()
}

/// #4517 AC6 — the sweep no longer closes superseded sessions: the door ends a role's older
/// open pane rows when its next login is written (one writer; the sweep presented each
/// role's own login from Jeff's account to do it). NEGATIVE PROOF that the path does not
/// return: the 11-10 shape (an older and a newer open pane session) makes no Session write.
#[test]
fn ac6_the_sweep_writes_no_session_row_even_with_a_superseded_one() {
    let home = tempdir("4950-sup");
    let sessions = format!(r#"{{"data":[{},{}]}}"#,
        sess("wren-old", "wren", "pane", "2026-11-08T16:00:00Z", "2026-11-11T10:50:00Z"),
        sess("wren-new", "wren", "pane", "2026-11-09T16:15:00Z", "2026-11-11T10:40:00Z"));
    door_with(&home, "wren", "200", r#"{"data":[]}"#, &sessions);
    live(&home, "wren");
    let (code, out) = sweep(&home);
    let c = closes(&read(home.join("calls.log")));
    assert_eq!(code, 0, "{out}");
    assert!(c.iter().all(|l| !l.starts_with("PATCH ") && !l.starts_with("PUT ")), "the sweep wrote a Session row: {c:?}\n{out}");
}

/// NEGATIVE PROOF: one open pane session per role, another role's older session, and a
/// browser session are never touched.
#[test]
fn ac2_negative_one_per_role_and_browser_sessions_are_left_alone() {
    let home = tempdir("4950-alone");
    let sessions = format!(r#"{{"data":[{},{},{}]}}"#,
        sess("wren-only", "wren", "pane", "2026-11-09T16:15:00Z", "2026-11-11T10:40:00Z"),
        sess("kade-only", "kade", "pane", "2026-11-08T16:00:00Z", "2026-11-11T10:40:00Z"),
        sess("wren-browser", "wren", "browser", "2026-11-01T10:00:00Z", "2026-11-01T10:00:00Z"));
    door_with(&home, "wren", "200", r#"{"data":[]}"#, &sessions);
    live(&home, "wren");
    let (code, out) = sweep(&home);
    assert_eq!(code, 0, "{out}");
    assert!(closes(&read(home.join("calls.log"))).is_empty(), "{}", read(home.join("calls.log")));
}

/// NEGATIVE PROOF: an older session that an open run still points at is kept.
#[test]
fn ac2_negative_an_older_session_with_an_open_run_is_kept() {
    let home = tempdir("4950-run");
    let sessions = format!(r#"{{"data":[{},{}]}}"#,
        sess("wren-old", "wren", "pane", "2026-11-08T16:00:00Z", "2026-11-11T10:50:00Z"),
        sess("wren-new", "wren", "pane", "2026-11-09T16:15:00Z", "2026-11-11T10:40:00Z"));
    let runs = r#"{"data":[{"name":"wren-run-1","ownedBy":"principal-wren","runOf":"session-wren-old","startedAt":"2026-11-08T16:01:00Z","runEndedAt":""}]}"#;
    door_with(&home, "wren", "200", runs, &sessions);
    live(&home, "wren");
    let (_, out) = sweep(&home);
    assert!(closes(&read(home.join("calls.log"))).is_empty(), "{out}\n{}", read(home.join("calls.log")));
}
