// @domain: identity
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
