// @domain: identity
// @test-type: unit — rows over temp transcripts, and the built `seen` binary against a temp identity dir; no door
//! #4965 (4895's nightly red) — Abby's and Borg Queen's Presence read "unknown" for good:
//! `seen` judged a delivery from the hook input's `prompt`, and an agy hook input carries
//! none (the row's measured notes, 10-14). The prompt is now read through the role's
//! runtime row, the one reader chorus-hooks uses (shared/runtime_prompt.rs).

use chorus_principal::rows::*;
use serde_json::json;

fn scratch() -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("agy-4965-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn agy_turn(dir: &std::path::Path, user_input: &str) -> String {
    let logs = dir.join("brain/c20fed59-2575-431e-92d5-33b4939ba2e4/.system_generated/logs");
    std::fs::create_dir_all(&logs).unwrap();
    let t = logs.join("transcript.jsonl");
    let line = json!({"type": "USER_INPUT", "content": format!("<USER_REQUEST>{user_input}</USER_REQUEST>")});
    std::fs::write(&t, format!("{}\n", line)).unwrap();
    // an agy PreInvocation input: no prompt field at all
    json!({"conversationId": "transcript", "transcriptPath": t.to_str().unwrap(), "invocationNum": 0}).to_string()
}

#[test]
fn an_agy_turn_whose_transcript_holds_a_delivery_reads_as_delivered() {
    let dir = scratch();
    let input = agy_turn(&dir, WAKE_LINE);
    let (_, agy) = runtime_for(&runtime_table(), "borg-queen").expect("borg-queen runs on a runtime row");
    assert!(turn_facts_on(&input, Some(&agy)).1, "read through the agy row, the delivery is seen");
    // NEGATIVE PROOF: the old read (the input's own prompt field) never sees it
    assert!(!turn_facts(&input).1, "without the row the agy delivery is invisible");
}

#[test]
fn an_agy_turn_that_is_not_a_delivery_stays_not_delivered() {
    let dir = scratch();
    let input = agy_turn(&dir, "just Jeff typing, not a pulse delivery");
    let (_, agy) = runtime_for(&runtime_table(), "abby-normal").expect("abby-normal runs on a runtime row");
    assert!(!turn_facts_on(&input, Some(&agy)).1);
}

// #4965 review fix — agy's before-prompt hook (PreInvocation) fires once per model call;
// only the first (invocationNum 0) is the turn. `seen` acted on every call: each later
// call re-read the turn's prompt, force-stamped a delivery past the rate limit and ran
// the revocation check again. And the speaker and the received log read the input's
// `prompt` field, which agy never sends, so Jeff's turns to an agy role never read as his.

fn agy_call(dir: &std::path::Path, user_input: &str, invocation: i64) -> String {
    let mut v: serde_json::Value = serde_json::from_str(&agy_turn(dir, user_input)).unwrap();
    v["invocationNum"] = json!(invocation);
    v.to_string()
}

/// `seen borg-queen` on the built binary with this input; (exit code, identity dir).
fn run_seen(input: &str, revoked: bool) -> (i32, std::path::PathBuf) {
    use std::io::Write;
    let home = scratch();
    let id = home.join("identity");
    let dir = id.join("borg-queen");
    std::fs::create_dir_all(&dir).unwrap();
    if revoked { std::fs::write(dir.join("revoked"), "revoked for the test").unwrap(); }
    let curl = home.join("curl-down");
    std::fs::write(&curl, "#!/bin/sh\nexit 7\n").unwrap();
    std::process::Command::new("chmod").arg("+x").arg(&curl).status().unwrap();
    let bin = std::env::current_exe().unwrap().parent().and_then(|d| d.parent()).unwrap().join("chorus-principal");
    let mut child = std::process::Command::new(bin)
        .args(["seen", "borg-queen"])
        .env("HOME", &home).env("AWAKE_CURL", &curl).env("CHORUS_IDENTITY_DIR", &id)
        .env("AWAKE_SEEN_EVERY", "3600").env("AWAKE_SEEN_SYNC", "0")
        .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
        .spawn().unwrap();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    (child.wait().unwrap().code().unwrap_or(-1), dir)
}

#[test]
fn a_later_agy_model_call_is_not_a_turn() {
    let dir = scratch();
    let (_, agy) = runtime_for(&runtime_table(), "borg-queen").unwrap();
    assert!(!later_model_call(&agy_call(&dir, WAKE_LINE, 0), Some(&agy)), "invocation 0 is the turn");
    assert!(later_model_call(&agy_call(&dir, WAKE_LINE, 3), Some(&agy)), "invocation 3 is a later model call");
    // a runtime with no turn-start field (Claude) has no later calls
    assert!(!later_model_call(r#"{"prompt":"hi"}"#, None));
    // on the binary: a revoked principal's turn is stopped once, at the turn's first call...
    assert_eq!(run_seen(&agy_call(&dir, WAKE_LINE, 0), true).0, 2);
    // NEGATIVE PROOF: ...and a later call of the same turn does nothing at all
    assert_eq!(run_seen(&agy_call(&dir, WAKE_LINE, 3), true).0, 0, "seen acted on a later model call");
}

#[test]
fn jeffs_words_to_an_agy_role_read_as_his_and_are_logged() {
    let dir = scratch();
    let (_, agy) = runtime_for(&runtime_table(), "borg-queen").unwrap();
    let input = agy_call(&dir, "Jeff typing to Borg Queen", 0);
    assert_eq!(speaker_of(&prompt_on(&input, Some(&agy))), Speaker::Jeff);
    // NEGATIVE PROOF: read from the input's own prompt field, agy's turn is the harness's
    assert_eq!(speaker(&input), Speaker::Harness);
    let (code, id) = run_seen(&input, false);
    assert_eq!(code, 0);
    let log = std::fs::read_to_string(id.join("received.jsonl")).unwrap_or_default();
    assert!(log.contains("Jeff typing to Borg Queen"), "the received log missed agy's prompt: {log:?}");
}
