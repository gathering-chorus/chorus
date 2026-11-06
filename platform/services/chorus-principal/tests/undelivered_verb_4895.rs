// @test-type: integration — runs the built binary as pulse does; no network (no cached row, no login path)
// @domain: identity
//! #4895 round 2 — the `undelivered` verb reaches its code through the built binary. Round 1
//! put the arm in lib.rs and proved the row logic (presence_truth_4895.rs), but main.rs's
//! verb list never routed it: in prod on 11-06 13:12, `chorus-principal undelivered` fell
//! through to a login and was refused for all five roles' verify. This test runs the binary.
use std::process::{Command, Stdio};

fn run(tag: &str, args: &[&str]) -> (i32, String) {
    let home = std::env::temp_dir().join(format!("undelivered-4895-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join("identity")).unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_chorus-principal")).args(args)
        .env("HOME", &home).env("CHORUS_IDENTITY_DIR", home.join("identity"))
        .env_remove("CHORUS_ROLE").stdin(Stdio::null()).output().unwrap();
    let _ = std::fs::remove_dir_all(&home);
    (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stderr).to_string())
}

#[test]
fn the_binary_routes_undelivered_to_its_own_arm() {
    // without a pane id the arm answers its own usage line (exit 2): proof the verb got there
    let (code, err) = run("usage", &["undelivered", "abby-normal"]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("undelivered <role> <pane %N>"), "{err}");
}

#[test]
fn a_role_with_no_cached_presence_writes_nothing_and_is_never_a_login() {
    // no cached Presence: the arm returns 1 before any door call; the 11-06 fall-through
    // to login printed REFUSED instead
    let (code, err) = run("nocache", &["undelivered", "abby-normal", "%16"]);
    assert_eq!(code, 1, "{err}");
    assert!(!err.contains("REFUSED"), "routed to login again: {err}");
}

// NEGATIVE PROOF (#3734): a verb main.rs does not list (and that is no principal name, so it
// can never become a login) gets the binary's general usage, exit 2, not the undelivered arm.
#[test]
fn negative_proof_an_unlisted_verb_never_reaches_the_arm() {
    let (code, err) = run("unlisted", &["Undelivered_x", "abby-normal"]);
    assert_eq!(code, 2, "{err}");
    assert!(!err.contains("undelivered <role> <pane %N>"), "{err}");
}
