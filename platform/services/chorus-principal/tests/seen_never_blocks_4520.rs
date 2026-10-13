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
