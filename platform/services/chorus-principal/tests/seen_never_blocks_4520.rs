// @test-type: unit
// @domain: identity
//! #4520 reopen 1 — a slow or unreachable door never stops a prompt through
//! the per-turn `seen` stamp. On 2026-10-13 Abby's every prompt was refused:
//! "chorus-principal: REFUSED — could not read …/v1/roles/roles?limit=500".

use std::process::{Command, Stdio};
use std::io::Write;

fn run_seen(home: &std::path::Path, revoked: bool) -> i32 {
    let dir = home.join("identity").join("abby-normal");
    std::fs::create_dir_all(&dir).unwrap();
    if revoked { std::fs::write(dir.join("revoked"), "revoked for the test").unwrap(); }
    // a curl that always fails: the door is unreachable
    let curl = home.join("curl-down");
    std::fs::write(&curl, "#!/bin/sh\nexit 7\n").unwrap();
    Command::new("chmod").arg("+x").arg(&curl).status().unwrap();
    let mut child = Command::new(bin())
        .args(["seen", "abby-normal"])
        .env("HOME", home)
        .env("AWAKE_CURL", &curl)
        .env("CHORUS_IDENTITY_DIR", home.join("identity"))
        .env("AWAKE_SEEN_SYNC", "0")
        .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null())
        .spawn().unwrap();
    child.stdin.take().unwrap().write_all(br#"{"prompt":"hello"}"#).unwrap();
    child.wait().unwrap().code().unwrap_or(-1)
}

#[test]
fn an_unreachable_door_does_not_stop_the_prompt() {
    let home = tempfile_dir("seen-down");
    assert_eq!(run_seen(&home, false), 0, "seen must not refuse a turn because the door is down");
}

#[test]
fn negative_proof_a_revoked_principal_still_stops_the_turn() {
    let home = tempfile_dir("seen-revoked");
    assert_eq!(run_seen(&home, true), 2, "revocation still bites on the next turn");
}

fn tempfile_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The built binary, found at run time beside this test (target/<profile>/).
fn bin() -> std::path::PathBuf {
    let me = std::env::current_exe().unwrap();
    me.parent().and_then(|d| d.parent()).unwrap().join("chorus-principal")
}

/// #4520 reopen 2 — Abby's last seen sat at 20:42Z for three hours while her
/// hook fired on every prompt: the background row write died with the hook.
/// This runs `seen` the way a hook runner can: in its own process group,
/// which is killed the moment `seen` returns. The write must still happen.
#[test]
fn the_row_write_outlives_the_hook_runner_killing_its_group() {
    use std::os::unix::process::CommandExt;
    let home = tempfile_dir("seen-group-kill");
    let dir = home.join("identity").join("abby-normal");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("session.row.json"), r#"{"name":"s1","actsAs":"abby-normal"}"#).unwrap();
    // the first thing the write does is ask for its credential: the fake marks that it ran
    let marker = home.join("write-ran");
    let token = home.join("token-bin");
    std::fs::write(&token, format!("#!/bin/sh\nsleep 0.3\ntouch '{}'\nexit 1\n", marker.display())).unwrap();
    Command::new("chmod").arg("+x").arg(&token).status().unwrap();
    let curl = home.join("curl-down");
    std::fs::write(&curl, "#!/bin/sh\nexit 7\n").unwrap();
    Command::new("chmod").arg("+x").arg(&curl).status().unwrap();
    let mut child = Command::new(bin())
        .args(["seen", "abby-normal"])
        .env("HOME", &home)
        .env("AWAKE_CURL", &curl)
        .env("CHORUS_TOKEN_BIN", &token)
        .env("CHORUS_IDENTITY_DIR", home.join("identity"))
        .env("AWAKE_SEEN_EVERY", "0")
        .env("AWAKE_SEEN_SYNC", "0")
        .process_group(0)
        .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null())
        .spawn().unwrap();
    child.stdin.take().unwrap().write_all(br#"{"prompt":"hello"}"#).unwrap();
    let pgid = child.id();
    assert_eq!(child.wait().unwrap().code(), Some(0));
    // the hook runner is done with this hook: it ends the whole group
    let _ = Command::new("kill").args(["-KILL", &format!("-{pgid}")]).stderr(Stdio::null()).status();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !marker.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(marker.exists(), "the session row write died with the hook's process group");
}
