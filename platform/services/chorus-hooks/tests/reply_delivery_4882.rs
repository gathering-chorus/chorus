//! @test-type unit
//! #4882 — a turn's reply is the turn's own text: the Stop payload's final text first, the
//! transcript only after the turn's prompt; never the previous reply or an interim line.
use chorus_hooks::reply_delivery::reply_text;
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
fn gemini_path_is_unchanged_4882() {
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
