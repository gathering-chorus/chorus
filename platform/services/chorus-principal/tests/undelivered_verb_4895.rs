// @test-type: unit
// @domain: identity
//! #4895 round 2 — the `undelivered` verb reaches its code through the built binary. Round 1
//! put the arm in lib.rs and proved the row logic (presence_truth_4895.rs), but main.rs's
//! verb list never routed it: in prod on 11-06 13:12, `chorus-principal undelivered` was
//! refused as a login by all five roles' verify. This test runs the binary, as pulse does.
use std::process::{Command, Stdio};

fn bin() -> std::path::PathBuf {
    let me = std::env::current_exe().unwrap();
    me.parent().and_then(|d| d.parent()).unwrap().join("chorus-principal")
}

fn run(args: &[&str]) -> (i32, String) {
    let home = std::env::temp_dir().join(format!("undelivered-4895-{}-{}", std::process::id(), args.len()));
    std::fs::create_dir_all(home.join("identity")).unwrap();
    let o = Command::new(bin()).args(args)
        .env("HOME", &home).env("CHORUS_IDENTITY_DIR", home.join("identity"))
        .env_remove("CHORUS_ROLE").stdin(Stdio::null()).output().unwrap();
    (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stderr).to_string())
}

#[test]
fn the_binary_routes_undelivered_to_its_own_arm() {
    // without a pane id the arm answers its own usage line (exit 2): proof the verb got there
    let (code, err) = run(&["undelivered", "abby-normal"]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("undelivered <role> <pane %N>"), "{err}");
}

#[test]
fn a_role_with_no_cached_presence_writes_nothing_and_is_not_a_login() {
    let (code, err) = run(&["undelivered", "abby-normal", "%16"]);
    assert_eq!(code, 1, "{err}");
    assert!(!err.contains("cannot start"), "the 11-06 prod refusal: {err}");
}

// NEGATIVE PROOF (#3734): a verb the list does not name never reaches the undelivered arm,
// so the usage line above is this verb's own, not a catch-all.
#[test]
fn negative_proof_an_unlisted_verb_does_not_reach_the_arm() {
    let (_, err) = run(&["undelivered-not-a-verb", "abby-normal"]);
    assert!(!err.contains("undelivered <role> <pane %N>"), "{err}");
}
