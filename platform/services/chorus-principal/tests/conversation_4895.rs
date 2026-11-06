// @domain: spine — placed by chorus-crawl's domain rules (#4538)
//! #4895 AC3 — an agy run fills its conversationId. Abby's and BQ's runs read "pending"
//! from 11-05 16:19 on: `seen` read only `session_id`, which an Antigravity turn does not
//! carry as an id (it carries none, or the word "transcript"); the id is the brain folder
//! in `transcript_path`, the rule chorus-hooks' shim already reads (#4785).
use chorus_principal::rows::{conversation_of, turn_facts, with_conversation};
use serde_json::json;

const AGY: &str = "/Users/chorus-abby-normal/.gemini/antigravity-cli/brain/c20fed59-2575-431e-92d5-33b4939ba2e4/.system_generated/logs/transcript.jsonl";

#[test]
fn an_agy_turn_names_its_conversation_from_the_brain_folder() {
    let input = json!({"prompt": "hi", "transcript_path": AGY}).to_string();
    assert_eq!(turn_facts(&input).0, "c20fed59-2575-431e-92d5-33b4939ba2e4");
    let input = json!({"prompt": "hi", "session_id": "transcript", "transcript_path": AGY}).to_string();
    assert_eq!(turn_facts(&input).0, "c20fed59-2575-431e-92d5-33b4939ba2e4", "the word 'transcript' is not an id");
}

#[test]
fn a_claude_turn_keeps_its_session_id() {
    let v = json!({"session_id": "a65fbe91-a0b1-4c2d-9e3f-0123456789ab", "transcript_path": "/x/.claude/projects/p/a65fbe91-a0b1-4c2d-9e3f-0123456789ab.jsonl"});
    assert_eq!(conversation_of(&v), "a65fbe91-a0b1-4c2d-9e3f-0123456789ab");
}

#[test]
fn the_pending_run_takes_the_agy_conversation() {
    let run = json!({"name": "abby-normal-run-1a1ac823a4b", "conversationId": "pending"});
    let conv = turn_facts(&json!({"prompt": "hi", "transcript_path": AGY}).to_string()).0;
    assert_eq!(with_conversation(run, &conv).unwrap()["conversationId"], "c20fed59-2575-431e-92d5-33b4939ba2e4");
}

// NEGATIVE PROOF (#3734): a turn with no id anywhere writes nothing (the run stays
// pending, never a made-up id), and a path without a brain/<uuid> folder is no id.
#[test]
fn negative_proof_no_id_is_never_invented() {
    assert_eq!(conversation_of(&json!({"session_id": "transcript"})), "");
    assert_eq!(conversation_of(&json!({"transcript_path": "/tmp/transcript.jsonl"})), "");
    assert_eq!(conversation_of(&json!({"transcript_path": "/x/brain/not-a-uuid/logs/transcript.jsonl"})), "");
    let run = json!({"name": "abby-normal-run-x", "conversationId": "pending"});
    assert!(with_conversation(run, "").is_none());
}
