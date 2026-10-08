// @test-type: unit — temp-dir chat-file fixtures; no daemon, no relay, no live Gemini.
// @domain: messages
//! #4445 — Abby's 20:41 reply reached the demo Clearing doubled (10-07):
//! " \n \n--- Abby Normal | ... #4445. 026-10-07 20:41 Boston | # 4358 | ... #444 5. "
//! Gemini's AfterAgent `prompt_response` is every model turn of the prompt
//! joined with spaces (gemini-cli: cumulativeResponse += ` ${responseText}`),
//! not the reply. Her chat file held the reply once. The published text is the
//! last Gemini message in that file; prompt_response is only the fallback.
use chorus_hooks::reply_delivery::reply_text;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const DOUBLED: &str = " \n \n--- Abby Normal | 2026-10-07 20:41 Boston | #4358 | Werk v1.8.0 ---\n\nAbby here, testing Clearing render for Silas #4445. 026-10-07 20:41 Boston | # 4358 | Werk v1.8.0 ---\n\nAbby here, testing Clearing render for Silas #444 5. ";
const REPLY: &str = "--- Abby Normal | 2026-10-07 20:41 Boston | #4358 | Werk v1.8.0 ---\n\nAbby here, testing Clearing render for Silas #4445.";

/// A chat file shaped like hers: an older reply, a tool round with empty model
/// text, `$set` bookkeeping lines, a record written twice, then the reply.
fn chat(last: &str) -> PathBuf {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let p = std::env::temp_dir().join(format!("gemini-chat-4445-{n}.jsonl"));
    let lines = [
        serde_json::json!({"id":"a","type":"gemini","content":"an older reply"}).to_string(),
        serde_json::json!({"id":"b","type":"user","content":[{"text":"Silas here, testing #4445"}]}).to_string(),
        serde_json::json!({"id":"c","type":"gemini","content":"","toolCalls":[{"name":"run_shell_command"}]}).to_string(),
        r#"{"$set":{"lastUpdated":"2026-10-08T00:41:26.088Z"}}"#.to_string(),
        serde_json::json!({"id":"c","type":"gemini","content":"","toolCalls":[{"name":"run_shell_command"}]}).to_string(),
        serde_json::json!({"id":"d","type":"user","content":[{"functionResponse":{"name":"run_shell_command"}}]}).to_string(),
        serde_json::json!({"id":"e","type":"gemini","content":last}).to_string(),
    ];
    std::fs::write(&p, lines.join("\n") + "\n").unwrap();
    p
}

fn at_turn_end(path: &Path) -> serde_json::Value {
    serde_json::json!({"hook_event_name":"AfterAgent","prompt_response":DOUBLED,
        "gemini_transcript_path": path.to_string_lossy()})
}

#[test]
fn the_reply_published_is_her_last_message_once() {
    let p = chat(REPLY);
    assert_eq!(reply_text(&at_turn_end(&p)).as_deref(), Some(REPLY));
}

#[test]
fn negative_proof_without_her_chat_file_the_doubled_text_is_what_goes_out() {
    // the state the fix exists to separate: same turn end, no chat file named
    let raw = serde_json::json!({"hook_event_name":"AfterAgent","prompt_response":DOUBLED});
    assert_eq!(reply_text(&raw).as_deref(), Some(DOUBLED));
}

#[test]
fn a_turn_that_ended_on_tools_never_republishes_an_older_reply() {
    // last Gemini record is empty: the older "an older reply" must not be sent
    let p = chat("");
    let got = reply_text(&at_turn_end(&p));
    assert_ne!(got.as_deref(), Some("an older reply"));
    assert_eq!(got.as_deref(), Some(DOUBLED), "falls back to what Gemini handed over");
}

#[test]
fn an_unreadable_chat_file_falls_back_to_what_gemini_handed_over() {
    let raw = serde_json::json!({"prompt_response":"Pipeline is green.",
        "gemini_transcript_path":"/nonexistent/chat-4445.jsonl"});
    assert_eq!(reply_text(&raw).as_deref(), Some("Pipeline is green."));
}

// 2026-10-08 06:39: a turn with no text reached the Clearing as "[no response text]"
#[test]
fn gemini_s_no_text_placeholder_is_never_published() {
    let raw = serde_json::json!({"hook_event_name":"AfterAgent","prompt_response":"[no response text]"});
    assert_eq!(reply_text(&raw), None);
    let p = chat("");
    let raw = serde_json::json!({"prompt_response":"[no response text]","gemini_transcript_path":p.to_string_lossy()});
    assert_eq!(reply_text(&raw), None, "an empty last message plus the placeholder publishes nothing");
}
