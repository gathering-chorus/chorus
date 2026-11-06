// @domain: spine — placed by chorus-crawl's domain rules (#4538)
//! #4895 AC4 — lastSeenAt moves with a role's activity within one turn. Measured 11-06
//! 09:58: Kade's lastSeenAt 10 minutes old and Wren's 16 while both worked. `seen` ran
//! only before a prompt, and a long turn of tool calls sends none. The turn's end now
//! stamps the session too, on every runtime the hooks file is written for.
use chorus_principal::rows::runtime_hooks_file;
use serde_json::json;

fn claude_row() -> serde_json::Value {
    json!({"runtimeHooksLayout": "claude-settings", "eventSessionStart": "SessionStart", "eventBeforePrompt": "UserPromptSubmit",
           "eventBeforeTool": "PreToolUse", "eventAfterTool": "PostToolUse", "eventOnStop": "Stop"})
}

fn agy_row() -> serde_json::Value {
    json!({"runtimeHooksLayout": "named-hooks", "eventBeforePrompt": "BeforeAgent", "eventBeforeTool": "BeforeTool", "eventOnStop": "AfterAgent"})
}

#[test]
fn a_claude_turn_end_stamps_the_session_seen() {
    let h = runtime_hooks_file(&claude_row(), "claude-code", "kade", "/b/shim", "/b/principal").unwrap();
    assert_eq!(h["hooks"]["Stop"][0]["hooks"][0]["command"], "/b/principal seen kade");
    assert_eq!(h["hooks"]["Stop"][0]["hooks"][1]["command"], "/b/shim runtime-hook claude-code Stop");
    assert_eq!(h["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"], "/b/principal seen kade", "the prompt still stamps first");
}

#[test]
fn an_agy_turn_end_stamps_the_session_seen() {
    let h = runtime_hooks_file(&agy_row(), "gemini", "abby-normal", "/b/shim", "/b/principal").unwrap();
    assert_eq!(h["chorus"]["AfterAgent"][0]["command"], "/b/principal seen abby-normal");
}

// NEGATIVE PROOF (#3734): a tool call does not stamp (seen is once per turn edge, not
// per tool), so the before-prompt and end-of-turn moments are the only two that do.
#[test]
fn negative_proof_tool_moments_do_not_stamp() {
    let h = runtime_hooks_file(&claude_row(), "claude-code", "kade", "/b/shim", "/b/principal").unwrap();
    for ev in ["PreToolUse", "PostToolUse", "SessionStart"] {
        let cmds: Vec<String> = h["hooks"][ev][0]["hooks"].as_array().unwrap().iter().map(|x| x["command"].as_str().unwrap().to_string()).collect();
        assert!(!cmds.iter().any(|c| c.contains(" seen ")), "{ev} must not stamp: {cmds:?}");
    }
}

// The Claude roles' hooks are the tracked roles/<role>/.claude/settings.json (and Abby's
// .gemini/settings.json), not rendered: each must stamp at the turn's end too.
#[test]
fn every_tracked_role_settings_stamps_at_the_turn_end() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for (role, file, ev) in [("wren", ".claude/settings.json", "Stop"), ("kade", ".claude/settings.json", "Stop"),
                             ("silas", ".claude/settings.json", "Stop"), ("abby-normal", ".gemini/settings.json", "AfterAgent")] {
        let s = std::fs::read_to_string(root.join("roles").join(role).join(file)).unwrap();
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        let cmds: Vec<String> = v["hooks"][ev].as_array().unwrap().iter()
            .flat_map(|m| m["hooks"].as_array().cloned().unwrap_or_default())
            .filter_map(|h| h["command"].as_str().map(String::from)).collect();
        assert!(cmds.iter().any(|c| c.ends_with(&format!("chorus-principal seen {role}"))), "{role} {ev}: {cmds:?}");
    }
}

// Code review (run 2): the turn's end only stamps the time. A Stop hook that exits 2 tells
// Claude to keep going, so the turn-end path never takes the revocation branch.
#[test]
fn a_turn_end_is_recognised_and_a_prompt_is_not() {
    use chorus_principal::rows::is_turn_end;
    assert!(is_turn_end(r#"{"hook_event_name":"Stop","session_id":"s"}"#));
    assert!(is_turn_end(r#"{"hook_event_name":"AfterAgent","prompt":"hi"}"#));
    assert!(!is_turn_end(r#"{"hook_event_name":"UserPromptSubmit","prompt":"hi"}"#));
    assert!(!is_turn_end(r#"{"prompt":"hi"}"#));
}
