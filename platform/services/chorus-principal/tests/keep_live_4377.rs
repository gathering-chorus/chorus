// @test-type: unit
// @domain: identity
//! #4377 reopen 1 — Jeff, 2026-10-14 05:23: "see no sessions for wren and abby".
//! Both were running; both Session rows had expired 10 minutes after their
//! last turn, so the Clearing dropped them. What he sees after this: a session
//! whose process is running stays open, idle or not; one whose process has
//! exited is not kept alive by the sweep.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const EXPIRED: &str = "2026-01-01T00:00:00Z";

/// A world with wren logged in and a saved session row that expired long ago.
/// `pid` is the process the registry says wren runs in.
fn world(tag: &str, pid: u32) -> PathBuf {
    let home = std::env::temp_dir().join(format!("keep-live-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let id = home.join("identity").join("wren");
    std::fs::create_dir_all(&id).unwrap();
    std::fs::write(id.join("login.json"), r#"{"at":"2026-10-12T12:45:00Z","pid":null,"session":"wren-s1","state":"recorded"}"#).unwrap();
    std::fs::write(id.join("session.row.json"), format!(r#"{{"name":"wren-s1","sessionState":"open","expiresAt":"{EXPIRED}","lastSeenAt":"2026-01-01T00:00:00Z"}}"#)).unwrap();
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    std::fs::write(sessions.join(format!("wren-{pid}.json")), format!(r#"{{"role":"wren","pid":{pid},"tty":"/dev/ttys003","host":"tmux","tmux":"%1"}}"#)).unwrap();

    // a login token for wren that is good for ten more minutes
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let claims = format!(r#"{{"webid":"http://localhost:3000/wren/profile/card#me","exp":{},"iat":{now},"jti":"j1"}}"#, now + 600);
    let token = format!("h.{}.s", b64url(claims.as_bytes()));
    script(&home.join("token-bin"), &format!("printf '%s' '{token}'"));
    // the door: roles list, an empty sessions listing, and every write accepted
    // (each write's body is kept so the test can read what was sent)
    script(&home.join("curl"), &format!(r#"case "$*" in
  *" -X PUT "*) for a in "$@"; do case "$a" in @*body) cat "${{a#@}}" >> '{w}';; esac; done; printf '{{}}\n200';;
  */v1/roles/roles*) printf '{{"data":[{{"name":"wren","roleKind":"agent"}},{{"name":"jeff","roleKind":"human"}}]}}';;
  *) printf '{{"data":[]}}';;
esac"#, w = home.join("writes").display()));
    home
}

fn sweep(home: &Path) -> (i32, String) {
    let out = Command::new(bin())
        .arg("sweep")
        .env("HOME", home)
        .env("CHORUS_ROOT", home)
        .env("CHORUS_IDENTITY_DIR", home.join("identity"))
        .env("CHORUS_SESSIONS_DIR", home.join("sessions"))
        .env("CHORUS_TOKEN_BIN", home.join("token-bin"))
        .env("AWAKE_CURL", home.join("curl"))
        .stdin(Stdio::null())
        .output().unwrap();
    (out.status.code().unwrap_or(-1), String::from_utf8_lossy(&out.stdout).to_string())
}

fn writes(home: &Path) -> String { std::fs::read_to_string(home.join("writes")).unwrap_or_default() }

#[test]
fn an_idle_session_whose_process_runs_is_kept_open_past_its_expiry() {
    // this test's own process stands in for wren's running Claude
    let home = world("alive", std::process::id());
    let (code, out) = sweep(&home);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("1 live session(s) kept open"), "{out}");
    let sent = writes(&home);
    assert!(sent.contains(r#""name":"wren-s1""#), "the session row was written: {sent}");
    assert!(!sent.contains(&format!(r#""expiresAt":"{EXPIRED}""#)), "its expiry moved forward to the renewed login: {sent}");
}

#[test]
fn negative_proof_a_session_whose_process_exited_is_not_kept() {
    // a pid that is not running: spawn a process and wait for it to end
    let mut gone = Command::new("true").spawn().unwrap();
    let pid = gone.id();
    gone.wait().unwrap();
    let home = world("exited", pid);
    let (code, out) = sweep(&home);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("0 live session(s) kept open"), "{out}");
    assert!(!writes(&home).contains("wren-s1"), "nothing renews a session with no process: {}", writes(&home));
}

fn script(p: &Path, body: &str) {
    std::fs::write(p, format!("#!/bin/sh\n{body}\n")).unwrap();
    Command::new("chmod").arg("+x").arg(p).status().unwrap();
}

fn b64url(b: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut s = String::new();
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..=c.len() { s.push(A[(n >> (18 - 6 * i) & 63) as usize] as char); }
    }
    s
}

/// The built binary, found at run time beside this test (target/<profile>/).
fn bin() -> PathBuf {
    let me = std::env::current_exe().unwrap();
    me.parent().and_then(|d| d.parent()).unwrap().join("chorus-principal")
}
