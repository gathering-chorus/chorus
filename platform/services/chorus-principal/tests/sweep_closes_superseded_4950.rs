// @test-type: unit — which pane sessions the sweep closes as superseded, against a fake door
// @domain: identity
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

/// The 11-10 shape: an older session (fresh lastSeenAt from the stale seen-write) and the
/// login's newer one. Only the older is closed, as superseded, by PATCH.
#[test]
fn ac2_the_older_pane_session_is_closed_as_superseded_newest_by_issued_at() {
    let home = tempdir("4950-sup");
    let sessions = format!(r#"{{"data":[{},{}]}}"#,
        sess("wren-old", "wren", "pane", "2026-11-08T16:00:00Z", "2026-11-11T10:50:00Z"),
        sess("wren-new", "wren", "pane", "2026-11-09T16:15:00Z", "2026-11-11T10:40:00Z"));
    door_with(&home, "wren", "200", r#"{"data":[]}"#, &sessions);
    live(&home, "wren");
    let (code, out) = sweep(&home);
    let c = closes(&read(home.join("calls.log")));
    assert_eq!(code, 0, "{out}");
    assert_eq!(c.len(), 1, "exactly one close: {c:?}\n{out}");
    assert!(c[0].starts_with("PATCH ") && c[0].contains("/identity/sessions/wren-old "), "{c:?}");
    let body: serde_json::Value = serde_json::from_str(c[0].splitn(3, ' ').nth(2).unwrap()).unwrap();
    assert_eq!(body["sessionState"], "closed", "{body}");
    assert!(body.get("endedBy").is_none(), "a superseded close names no ender: {body}");
    assert!(read(home.join("spine.log")).contains("session.superseded wren closed=wren-old by=sweep"), "{out}");
    assert!(body["endedAt"].as_str().unwrap_or("").ends_with('Z'), "{body}");
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

/// Wren's guard (pair 4950): a seen-write from a stale cache (an open copy of a row the
/// sweep has since closed) PATCHes only what it changed, lastSeenAt, and never sends
/// sessionState or endedAt, so it can never PATCH a closed row back open.
#[test]
fn ac2_a_seen_write_never_reopens_a_superseded_session() {
    let cached: serde_json::Value = serde_json::from_str(&sess("wren-old", "wren", "pane", "2026-11-08T16:00:00Z", "2026-11-11T10:50:00Z")).unwrap();
    let after = chorus_principal::rows::seen_session(cached.clone(), "2026-11-11T11:00:00Z").unwrap();
    let (_, body) = chorus_principal::rows::changed_fields(&cached, &after).unwrap();
    let keys: Vec<&String> = body.as_object().unwrap().keys().collect();
    assert!(keys.contains(&&"lastSeenAt".to_string()), "{body}");
    for k in ["sessionState", "endedAt", "endedBy"] { assert!(!keys.iter().any(|x| x.as_str() == k), "{k} sent: {body}"); }
}

/// NEGATIVE PROOF (carried from #4838 AC5, whose expiry cases #4950 deleted): a door that
/// refuses the close makes the sweep fail loudly, a non-zero exit with failed > 0, never a
/// quiet 0.
#[test]
fn ac2_negative_a_refused_close_makes_the_sweep_fail_loudly() {
    let home = tempdir("4950-refused");
    let sessions = format!(r#"{{"data":[{},{}]}}"#,
        sess("wren-old", "wren", "pane", "2026-11-08T16:00:00Z", "2026-11-11T10:50:00Z"),
        sess("wren-new", "wren", "pane", "2026-11-09T16:15:00Z", "2026-11-11T10:40:00Z"));
    door_with(&home, "wren", "422", r#"{"data":[]}"#, &sessions);
    live(&home, "wren");
    let (code, out) = sweep(&home);
    assert_eq!(closes(&read(home.join("calls.log"))).len(), 1, "the close was tried: {out}");
    assert_ne!(code, 0, "{out}");
    assert!(out.contains("0 superseded session(s) closed, 1 failed"), "{out}");
}
