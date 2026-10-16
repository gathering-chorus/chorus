// @test-type: unit
// @domain: messages
//! #4548 — Abby's replies never reached the Clearing: 0 reply.emitted for
//! abby-normal from 2026-10-14 10:06 (her move to agy) to 2026-10-16. The agy
//! row path handed the stop leg no reply, so the daemon had nothing to publish.
//! The reply is read from her transcript by the shim, which runs as her.
use chorus_hooks::runtime_rows::{canonical, row_in, table, Moment};
use serde_json::{json, Value};

fn agy() -> Value { row_in(&table(), "agy").expect("the model has an agy row").clone() }

fn stop_input(lines: &[&str]) -> Value {
    let dir = std::env::temp_dir().join(format!("agy-reply-4548-{}-{}", std::process::id(), lines.len()));
    std::fs::create_dir_all(&dir).unwrap();
    let t = dir.join("transcript_full.jsonl");
    std::fs::write(&t, lines.join("\n") + "\n").unwrap();
    // agy's hook input carries the same base fields on every event (recorded 10-14)
    json!({"conversationId": "16d27d81", "transcriptPath": t.to_string_lossy()})
}

#[test]
fn the_stop_leg_carries_abbys_reply_from_the_transcript() {
    let raw = stop_input(&[
        r#"{"step_index":0,"type":"USER_INPUT","content":"<USER_REQUEST>\nu there?\n</USER_REQUEST>"}"#,
        r#"{"step_index":1,"type":"PLANNER_RESPONSE","content":"thinking"}"#,
        r#"{"step_index":2,"type":"PLANNER_RESPONSE","content":"Yes, here."}"#,
    ]);
    let c = canonical(&agy(), Moment::OnStop, &raw).unwrap();
    assert_eq!(c["prompt_response"], "Yes, here.");
}

/// NEGATIVE PROOF: a turn that ended with no reply after the latest prompt
/// publishes nothing, never the previous turn's reply again.
#[test]
fn a_turn_with_no_reply_does_not_resend_the_last_one() {
    let raw = stop_input(&[
        r#"{"step_index":0,"type":"USER_INPUT","content":"<USER_REQUEST>\nfirst\n</USER_REQUEST>"}"#,
        r#"{"step_index":1,"type":"PLANNER_RESPONSE","content":"old reply"}"#,
        r#"{"step_index":2,"type":"USER_INPUT","content":"<USER_REQUEST>\nsecond\n</USER_REQUEST>"}"#,
    ]);
    let c = canonical(&agy(), Moment::OnStop, &raw).unwrap();
    assert!(c.get("prompt_response").is_none_or(Value::is_null), "{c}");
}
