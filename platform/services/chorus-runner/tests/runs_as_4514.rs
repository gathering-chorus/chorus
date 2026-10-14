// @test-type: unit
// @domain: services
//! #4514 — a verb a role runs on its own card runs as the verified caller; a
//! tool with one principal's rights runs as its owner; a row that says neither
//! is not offered. Silas + Wren, 2026-10-13: the werk rows are kade's, so under
//! owner-only rules wren and silas could not commit their own cards.
use chorus_runner::server::task_status_text;
use chorus_runner::skills::{run_identity, skill_tools, RunsAs};
use serde_json::{json, Map, Value};

fn inputs() -> Map<String, Value> {
    json!({ "WerkCardInput": { "type": "object", "properties": { "card": { "type": "integer" } }, "required": ["card"] } })
        .as_object().unwrap().clone()
}

fn werk_commit(runs_as: Option<&str>) -> Value {
    let mut r = json!({
        "name": "werk-commit", "executor": "deterministic", "inputShape": "WerkCardInput",
        "implementedBy": "werk-commit", "arguments": "{card} {caller} --atomic", "ownedBy": "principal-kade",
        "longRunning": true
    });
    if let Some(v) = runs_as { r["runsAs"] = json!(v); }
    r
}

#[test]
fn a_caller_run_verb_runs_as_whoever_calls_it() {
    let (tools, _) = skill_tools(&[werk_commit(Some("caller"))], &inputs());
    let t = &tools[0];
    assert_eq!(t.runs_as, RunsAs::Caller);
    assert!(t.long_running);
    assert_eq!(run_identity(t, "principal-wren").unwrap(), "wren", "wren commits her own card as wren, not kade");
    assert_eq!(run_identity(t, "silas").unwrap(), "silas");
}

#[test]
fn negative_proof_a_row_without_runs_as_is_not_offered() {
    let (tools, unmeasured) = skill_tools(&[werk_commit(None)], &inputs());
    assert!(tools.is_empty(), "a row with no runsAs became a tool");
    assert!(unmeasured[0].reason.contains("runsAs"), "{:?}", unmeasured);
}

#[test]
fn negative_proof_an_owner_run_tool_refuses_another_caller_and_an_unverified_one() {
    let (tools, _) = skill_tools(&[werk_commit(Some("owner"))], &inputs());
    let t = &tools[0];
    assert_eq!(run_identity(t, "kade").unwrap(), "kade");
    assert_eq!(run_identity(t, "jeff").unwrap(), "kade", "jeff may run it, as its owner");
    assert!(run_identity(t, "wren").is_err());
    let (caller_tools, _) = skill_tools(&[werk_commit(Some("caller"))], &inputs());
    assert!(run_identity(&caller_tools[0], "unverified").is_err(), "an unverified caller ran a verb");
}

#[test]
fn a_task_is_read_only_by_its_caller_or_jeff() {
    let dir = std::env::temp_dir().join(format!("runner-status-4514-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("CHORUS_RUNNER_TASKS", &dir);
    std::fs::write(dir.join("werk-commit-x1.json"), r#"{"id":"werk-commit-x1","caller":"wren","pid":0}"#).unwrap();
    std::fs::write(dir.join("werk-commit-x1.exit"), "0").unwrap();
    assert!(task_status_text("werk-commit-x1", "wren").unwrap().contains("done (exit 0)"));
    assert!(task_status_text("werk-commit-x1", "jeff").is_ok());
    assert!(task_status_text("werk-commit-x1", "kade").is_err(), "another role read wren's task");
    assert!(task_status_text("../etc", "wren").is_err(), "an id escaped the tasks folder");
}
