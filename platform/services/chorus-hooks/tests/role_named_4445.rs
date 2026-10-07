// @domain: identity
//! #4445 — an agent role other than the three Claude roles keeps its name in
//! every hook decision and activity event (Abby logged as role=unknown, 10-06 19:50).
use chorus_hooks::HookInput;

fn input(deploy_role: Option<&str>, cwd: &str) -> HookInput {
    serde_json::from_value(serde_json::json!({
        "tool_name": "Bash", "tool_input": {"command": "date"},
        "cwd": cwd, "deploy_role": deploy_role,
    })).unwrap()
}

#[test]
fn abby_keeps_her_name() {
    assert_eq!(input(Some("abby-normal"), "/tmp").role().as_str(), "abby-normal");
}

#[test]
fn the_three_claude_roles_are_unchanged() {
    for r in ["silas", "wren", "kade"] {
        assert_eq!(input(Some(r), "/tmp").role().as_str(), r);
    }
}

#[test]
fn negative_proof_a_malformed_or_empty_name_is_not_a_role() {
    // a bad name is ignored: the role is whatever the fallback (ancestry, cwd)
    // gives with no name at all — never the bad string itself
    let fallback = input(None, "/tmp").role();
    for bad in ["", "unknown", "Abby", "abby;rm", "-x"] {
        let got = input(Some(bad), "/tmp").role();
        assert_eq!(got, fallback, "{bad:?}");
        assert_ne!(got.as_str(), bad, "{bad:?}");
    }
}

// #4445 — Gemini's turn end hands the reply itself; it is what gets published.
#[test]
fn a_gemini_turn_end_reply_is_the_text_published() {
    let raw = serde_json::json!({"hook_event_name": "AfterAgent", "prompt": "status?", "prompt_response": "Pipeline is green."});
    assert_eq!(chorus_hooks::reply_delivery::reply_text(&raw).as_deref(), Some("Pipeline is green."));
}

#[test]
fn negative_proof_an_empty_or_missing_reply_publishes_nothing() {
    assert_eq!(chorus_hooks::reply_delivery::reply_text(&serde_json::json!({"prompt_response": "  "})), None);
    assert_eq!(chorus_hooks::reply_delivery::reply_text(&serde_json::json!({"prompt": "x"})), None);
}
