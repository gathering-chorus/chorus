// @test-type: unit
// @domain: identity
//! #4444 reopen 2 — Abby on agy. Her hook events are read through agy's
//! AgentRuntime row (the generated table), not typed in code. The fixtures are
//! real agy inputs recorded by the probe on 2026-10-14 08:32–08:42.
use chorus_hooks::runtime_rows::{canonical, is_turn_start, moment, prompt, reply, row_in, table, Moment};
use serde_json::{json, Value};
use std::path::PathBuf;

fn fixture(name: &str) -> Value {
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("tests/fixtures/agy-4444");
    serde_json::from_str(&std::fs::read_to_string(dir.join(name)).unwrap()).unwrap()
}

fn agy() -> Value { row_in(&table(), "agy").expect("the model has an agy row").clone() }

#[test]
fn agys_command_call_reaches_the_guard_as_bash() {
    let row = agy();
    assert_eq!(moment(&row, "PreToolUse"), Moment::BeforeTool);
    let c = canonical(&row, Moment::BeforeTool, &fixture("pre-tool-use.json")).unwrap();
    assert_eq!(c["tool_name"], "Bash");
    assert!(c["tool_input"]["command"].as_str().unwrap().contains("role-state idle"), "{c}");
    assert_eq!(c["cwd"], "/Users/jeffbridwell/CascadeProjects/chorus/roles/abby-normal");
    assert_eq!(c["session_id"], "110cd369-87dc-436f-a90b-f0acfc38c68b");
}

#[test]
fn agys_file_view_reaches_the_guard_as_read() {
    let c = canonical(&agy(), Moment::BeforeTool, &fixture("pre-tool-use-view.json")).unwrap();
    assert_eq!(c["tool_name"], "Read");
    assert!(c["tool_input"]["file_path"].as_str().unwrap().starts_with('/'), "{c}");
}

/// NEGATIVE PROOF: a tool the row does not map is never allowed silently:
/// the reply is agy's ask, so Jeff approves it.
#[test]
fn a_tool_the_row_does_not_map_asks_the_person() {
    let row = agy();
    assert!(canonical(&row, Moment::BeforeTool, &fixture("pre-tool-use-unmapped.json")).is_err());
    assert_eq!(reply(&row, Moment::BeforeTool, false, "", "", true), json!({"decision":"ask"}));
}

#[test]
fn only_the_first_model_call_of_a_turn_is_the_turn_start() {
    let row = agy();
    assert_eq!(moment(&row, "PreInvocation"), Moment::BeforePrompt);
    assert!(is_turn_start(&row, &fixture("pre-invocation-first.json")));
    assert!(!is_turn_start(&row, &fixture("pre-invocation-later.json")), "invocationNum 1 is not a new prompt");
}

#[test]
fn the_prompt_is_read_from_the_transcript() {
    let dir = std::env::temp_dir().join(format!("agy-transcript-4444-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let t = dir.join("transcript_full.jsonl");
    std::fs::write(&t, concat!(
        r#"{"step_index":0,"type":"USER_INPUT","content":"<USER_REQUEST>\nhi\n</USER_REQUEST>\n<ADDITIONAL_METADATA>x</ADDITIONAL_METADATA>"}"#, "\n",
        r#"{"step_index":1,"type":"PLANNER_RESPONSE","content":"hello"}"#, "\n")).unwrap();
    let mut raw = fixture("pre-invocation-first.json");
    raw["transcriptPath"] = json!(t.to_string_lossy());
    assert_eq!(prompt(&agy(), &raw).as_deref(), Some("hi"));
}

#[test]
fn replies_are_the_rows_templates_with_the_text_escaped() {
    let row = agy();
    assert_eq!(reply(&row, Moment::BeforeTool, true, "no \"rm\" here", "", false), json!({"decision":"deny","reason":"no \"rm\" here"}));
    // Jeff 09:06 "Guard decides": a tool the guard allows runs without asking him
    assert_eq!(reply(&row, Moment::BeforeTool, false, "", "", false), json!({"decision":"auto_approve"}));
    assert_eq!(reply(&row, Moment::BeforePrompt, false, "", "board: 2 WIP", false), json!({"injectSteps":[{"ephemeralMessage":"board: 2 WIP"}]}));
    assert_eq!(reply(&row, Moment::Other, false, "", "", false), json!({}));
}

/// NEGATIVE PROOF: the event names are the row's. A row that calls its
/// before-tool moment something else no longer treats PreToolUse as one.
#[test]
fn the_event_names_come_from_the_row() {
    let mut row = agy();
    row["eventBeforeTool"] = json!("SomethingElse");
    assert_eq!(moment(&row, "PreToolUse"), Moment::Other);
    assert_eq!(moment(&row, "SomethingElse"), Moment::BeforeTool);
}

/// The installed command end to end: a tool agy's row does not map gets the
/// ask reply on stdout, with no daemon, door or session involved.
#[test]
fn the_hook_command_asks_for_an_unmapped_agy_tool() {
    use std::io::Write;
    let bin = std::env::current_exe().unwrap().parent().and_then(|d| d.parent()).unwrap().join("chorus-hook-shim");
    let mut child = std::process::Command::new(bin).args(["runtime-hook", "agy", "PreToolUse"])
        .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null())
        .spawn().unwrap();
    child.stdin.take().unwrap().write_all(fixture("pre-tool-use-unmapped.json").to_string().as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v, json!({"decision":"ask"}));
}

/// R8 — the row-driven code names no runtime, native event, input field or
/// tag: every such value in the generated table is searched for in the source.
/// NEGATIVE PROOF: the same search over a source that types one finds it.
fn typed_values(source: &str) -> Vec<String> {
    let t = table();
    let mut values: Vec<String> = t["runtimes"].as_object().unwrap().keys().cloned().collect();
    for row in t["runtimes"].as_object().unwrap().values() {
        for (k, v) in row.as_object().unwrap() {
            if k.starts_with("event") || k.starts_with("field") || k.starts_with("transcript") {
                if let Some(s) = v.as_str() { values.push(s.to_string()); }
            }
        }
    }
    values.sort(); values.dedup();
    // the guard's own vocabulary is ours to type (Claude Code happens to share it)
    let ours = ["session_id", "prompt", "tool_name", "tool_input", "transcript_path", "cwd"];
    values.into_iter().filter(|v| !ours.contains(&v.as_str()) && v.len() > 3 && source.contains(&format!("\"{v}\""))).collect()
}

#[test]
fn the_row_driven_adapter_types_no_runtime_or_event_name() {
    let src = std::fs::read_to_string(PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("src/runtime_rows.rs")).unwrap();
    assert_eq!(typed_values(&src), Vec::<String>::new());
    assert_eq!(typed_values(&format!("{src}\nif name == \"PreInvocation\" {{}}")), vec!["PreInvocation".to_string()]);
}
