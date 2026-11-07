// @test-type: unit — pure: reply source selection over a temp transcript; no daemon, no network.
// @domain: messages
// @card: #4882
//! #4882 — a turn's reply is the turn's own text: the Stop payload's final text first, the
//! transcript only after the turn's prompt; never the previous reply or an interim line.
use chorus_hooks::reply_delivery::{publishes_reply, reply_text};
use serde_json::json;
/// A transcript as it stands at Stop: the previous turn's reply is there,
/// this turn's final text is not yet written (it lags), only its interim line.
fn lagging_transcript(dir: &std::path::Path) -> String {
    let p = dir.join("t.jsonl");
    let lines = [
        json!({"type":"user","message":{"role":"user","content":"earlier prompt"}}),
        json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"previous turn's reply"}]}}),
        json!({"type":"user","message":{"role":"user","content":"whats next"}}),
        json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"I'm checking what's still open."}]}}),
    ];
    std::fs::write(&p, lines.iter().map(|l| l.to_string() + "\n").collect::<String>()).unwrap();
    p.to_string_lossy().to_string()
}

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("rd-4882-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn the_turn_s_final_text_is_published_not_the_lagging_transcript_4882() {
    let d = tmp("final");
    let tp = lagging_transcript(&d);
    let raw = json!({"transcript_path": tp, "last_assistant_message": "#4882 is next."});
    assert_eq!(reply_text(&raw).as_deref(), Some("#4882 is next."));
    // negative proof (#3734): the old lookup, transcript only, publishes the interim line
    assert_eq!(chorus_hooks::last_assistant_text(&tp).as_deref(), Some("I'm checking what's still open."));
}

#[test]
fn a_turn_with_no_text_publishes_nothing_never_the_previous_reply_4882() {
    let d = tmp("empty");
    let p = d.join("t.jsonl");
    std::fs::write(&p, json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"previous turn's reply"}]}}).to_string() + "\n").unwrap();
    let tp = p.to_string_lossy().to_string();
    for empty in [json!(""), json!("   "), json!(null)] {
        let raw = json!({"transcript_path": tp, "last_assistant_message": empty});
        assert_eq!(reply_text(&raw), None, "empty final text must not fall back to the transcript");
    }
    // negative proof: the old whole-transcript lookup re-sends the previous reply
    assert_eq!(chorus_hooks::last_assistant_text(&tp).as_deref(), Some("previous turn's reply"));
}

#[test]
fn a_rewrite_after_a_stop_gate_publishes_the_rewrite_only_4882() {
    let d = tmp("rewrite");
    let tp = lagging_transcript(&d);
    // the continuation's Stop carries the rewrite, while the transcript still ends on the rejected draft
    let raw = json!({"transcript_path": tp, "stop_hook_active": true, "last_assistant_message": "short rewrite"});
    assert_eq!(reply_text(&raw).as_deref(), Some("short rewrite"));
}

#[test]
fn without_the_field_a_lagging_transcript_yields_nothing_past_the_prompt_4882() {
    let d = tmp("fallback");
    let p = d.join("t.jsonl");
    let lines = [
        json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"previous turn's reply"}]}}),
        json!({"type":"user","message":{"role":"user","content":"whats next"}}),
        json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","name":"Bash"}]}}),
        json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"ok"}]}}),
    ];
    std::fs::write(&p, lines.iter().map(|l| l.to_string() + "\n").collect::<String>()).unwrap();
    let tp = p.to_string_lossy().to_string();
    assert_eq!(reply_text(&json!({"transcript_path": tp})), None, "no text past the turn's prompt: publish nothing");
    assert_eq!(chorus_hooks::last_assistant_text(&tp).as_deref(), Some("previous turn's reply"), "negative proof: old lookup");
    // and text past the prompt (after a tool result) is the turn's own
    let mut more = std::fs::read_to_string(&p).unwrap();
    more.push_str(&(json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"answer"}]}}).to_string() + "\n"));
    std::fs::write(&p, more).unwrap();
    assert_eq!(reply_text(&json!({"transcript_path": tp})).as_deref(), Some("answer"));
}

#[test]
fn prompt_response_is_used_when_there_is_no_field_or_transcript_4882() {
    let raw = json!({"prompt_response": "gemini reply"});
    assert_eq!(reply_text(&raw).as_deref(), Some("gemini reply"));
}

#[test]
fn the_room_relay_gets_the_turn_s_final_text_not_the_lagging_transcript_4882() {
    use chorus_hooks::reply_delivery::enrich_stop_prompt_response;
    let d = tmp("relay");
    let tp = d.join("t.jsonl");
    let lines = [
        json!({"type":"user","message":{"role":"user","content":"earlier"}}),
        json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"previous turn's reply"}]}}),
        json!({"type":"user","message":{"role":"user","content":"whats next"}}),
    ];
    std::fs::write(&tp, lines.iter().map(|l| l.to_string() + "\n").collect::<String>()).unwrap();
    let tp = tp.to_string_lossy().to_string();
    let with_field = json!({"transcript_path": tp, "last_assistant_message": "final answer"}).to_string();
    let out: serde_json::Value = serde_json::from_str(&enrich_stop_prompt_response(with_field)).unwrap();
    assert_eq!(out["prompt_response"], "final answer");
    // a lagging transcript with no text past the prompt relays nothing
    let without = json!({"transcript_path": tp}).to_string();
    let out: serde_json::Value = serde_json::from_str(&enrich_stop_prompt_response(without)).unwrap();
    assert!(out.get("prompt_response").is_none(), "must not relay the previous reply: {out}");
    // negative proof (#3734): the old lookup this relay used returns the previous reply
    assert_eq!(chorus_hooks::last_assistant_text(&tp).as_deref(), Some("previous turn's reply"));
}

#[test]
fn a_session_with_no_role_never_publishes_a_reply_4884() {
    // negative proof: before #4884 an unknown-role Stop reached delivery and failed with
    // "no key at identity/unknown" (15 times on 11-03 from 18:40)
    for r in ["unknown", ""] {
        assert!(!publishes_reply(r), "{r:?} must not publish");
    }
    for r in ["wren", "silas", "kade", "abby-normal", "borg-queen"] {
        assert!(publishes_reply(r), "{r} publishes");
    }
}

#[test]
fn only_a_typed_prompt_starts_a_turn_4884() {
    let d = tmp("prompt");
    let p = d.join("t.jsonl");
    let lines = [
        json!({"type":"user","message":{"role":"user","content":"whats next"}}),
        json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"the answer"}]}}),
        // a hook-injected meta entry and a tool result are not the turn's prompt
        json!({"type":"user","isMeta":true,"message":{"role":"user","content":"<system-reminder>x</system-reminder>"}}),
        json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"ok"}]}}),
    ];
    std::fs::write(&p, lines.iter().map(|l| l.to_string() + "\n").collect::<String>()).unwrap();
    let tp = p.to_string_lossy().to_string();
    // negative proof: counting the meta entry as the prompt would leave no text after it → None
    assert_eq!(reply_text(&json!({"transcript_path": tp})).as_deref(), Some("the answer"));
}

#[test]
fn the_4882_test_files_carry_their_domain_4884() {
    // the crawler files a test with no `@domain:` header as out of sync (Kade, 11-03 19:00)
    let root = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR")).join("..");
    // werk-demo/tests/product_proof_4882.rs left with the product gate (#4889)
    let f = "chorus-hooks/tests/reply_delivery_4882.rs";
    let head: String = std::fs::read_to_string(root.join(f)).unwrap().lines().take(5).collect::<Vec<_>>().join("\n");
    assert!(head.contains("// @domain: "), "{f} has no @domain header in its first lines");
}
