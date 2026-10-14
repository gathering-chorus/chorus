// @test-type: unit
// @domain: services
//! #4515 — `chorus-runner stdio`, for a client that cannot renew an HTTP
//! credential (Abby's Gemini). What a role sees: its tools over stdio run as
//! the role the process proved it is, and a process that cannot prove one
//! never starts.
use chorus_runner::server::pick_caller;
use std::process::{Command, Stdio};

fn bin() -> std::path::PathBuf {
    // run-time, not env!(): the werk-test 4030 guard
    let me = std::env::current_exe().unwrap();
    me.parent().and_then(|d| d.parent()).unwrap().join("chorus-runner")
}

#[test]
fn the_gate_wins_then_the_stdio_caller_then_nobody() {
    assert_eq!(pick_caller(Some("wren".into()), Some("abby-normal")), "wren");
    assert_eq!(pick_caller(None, Some("abby-normal")), "abby-normal");
    assert_eq!(pick_caller(None, None), "unverified");
    assert_eq!(pick_caller(None, Some("")), "unverified", "an empty name is nobody");
}

fn stdio_with(env: &[(&str, &str)]) -> (i32, String) {
    let mut c = Command::new(bin());
    c.arg("stdio").env_remove("ROLE").env_remove("CHORUS_ROLE").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    for (k, v) in env { c.env(k, v); }
    let out = c.output().unwrap();
    (out.status.code().unwrap_or(-1), String::from_utf8_lossy(&out.stderr).to_string())
}

#[test]
fn negative_proof_no_role_never_starts() {
    let (code, err) = stdio_with(&[]);
    assert_eq!(code, 1);
    assert!(err.contains("no ROLE"), "{err}");
}

#[test]
fn negative_proof_a_role_this_account_cannot_mint_never_starts() {
    let empty = std::env::temp_dir().join(format!("stdio-4515-{}", std::process::id()));
    std::fs::create_dir_all(&empty).unwrap();
    let (code, err) = stdio_with(&[("ROLE", "abby-normal"), ("CHORUS_HOME", empty.to_str().unwrap())]);
    assert_eq!(code, 1);
    assert!(err.contains("could not mint abby-normal's credential"), "{err}");
}
