// @test-type: unit
// @domain: services
//! #4513 — a tool is a Skill row: deterministic, with an inputShape whose
//! schema athena-gen generated. It runs as its owner, only the owner and Jeff
//! may call it, its arguments are checked against the schema, and its argv is
//! an array filled one element per placeholder, never a shell string.

use chorus_runner::skills::{argv, check_args, child_env, input_table, may_invoke, skill_tools};
use serde_json::{json, Map, Value};

fn row(v: Value) -> Value {
    v
}

fn herald_row() -> Value {
    row(json!({
        "name": "herald-run", "executor": "deterministic", "inputShape": "HeraldRunInput",
        "implementedBy": "chorus-crawl",
        "arguments": "sensor {sensor} [--target {target}] [--dry-run {dry_run}] --requested-by {caller}",
        "ownedBy": "principal-silas", "comment": "Run one sensor pass.", "timeoutSeconds": 600, "runsAs": "owner"
    }))
}

fn args(v: Value) -> Map<String, Value> {
    v.as_object().unwrap().clone()
}

#[test]
fn a_deterministic_skill_with_an_input_shape_is_a_tool() {
    let rows = vec![
        herald_row(),
        json!({"name": "pull", "executor": "agent", "inputShape": "HeraldRunInput", "implementedBy": "x"}),
        json!({"name": "werk-build", "executor": "deterministic", "implementedBy": "werk-build"}),
    ];
    let (tools, unmeasured) = skill_tools(&rows, &input_table());
    assert_eq!(tools.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), vec!["herald-run"]);
    assert!(unmeasured.is_empty(), "{unmeasured:?}");
    let t = &tools[0];
    assert_eq!((t.owner.as_str(), t.binary.as_str(), t.timeout_s), ("silas", "chorus-crawl", 600));
    assert_eq!(t.schema["required"], json!(["sensor"]), "the schema is the generated one");
}

/// NEGATIVE PROOF: a row that asks to be a tool but cannot be offered is
/// reported as unmeasured, never dropped silently.
#[test]
fn a_tool_row_that_cannot_be_offered_is_unmeasured() {
    let rows = vec![
        json!({"name": "ghost", "executor": "deterministic", "inputShape": "NoSuchInput", "implementedBy": "x"}),
        json!({"name": "nobin", "executor": "deterministic", "inputShape": "HeraldRunInput"}),
    ];
    let (tools, unmeasured) = skill_tools(&rows, &input_table());
    assert!(tools.is_empty());
    let why: Vec<(&str, &str)> = unmeasured.iter().map(|u| (u.skill.as_str(), u.reason.as_str())).collect();
    assert_eq!(why[0].0, "ghost");
    assert!(why[0].1.contains("NoSuchInput"), "{why:?}");
    assert_eq!(why[1].0, "nobin");
}

#[test]
fn only_the_owner_and_jeff_may_call_a_tool() {
    assert!(may_invoke("silas", "silas"));
    assert!(may_invoke("principal-silas", "silas"));
    assert!(may_invoke("jeff", "silas"));
    assert!(!may_invoke("wren", "silas"), "a tool lends its owner's write rights");
    assert!(!may_invoke("kade", "silas"));
}

#[test]
fn arguments_are_checked_against_the_generated_schema() {
    let (tools, _) = skill_tools(&[herald_row()], &input_table());
    let schema = &tools[0].schema;
    assert!(check_args(&args(json!({"sensor": "code", "dry_run": true})), schema).is_ok());
    assert!(check_args(&args(json!({})), schema).unwrap_err().contains("sensor is required"));
    assert!(check_args(&args(json!({"sensor": "bogus"})), schema).unwrap_err().contains("must be one of"));
    assert!(check_args(&args(json!({"sensor": "code", "rm": "-rf"})), schema).unwrap_err().contains("not an argument"));
    assert!(check_args(&args(json!({"sensor": "code", "dry_run": "yes"})), schema).unwrap_err().contains("wrong type"));
}

#[test]
fn the_argv_follows_the_rows_template() {
    let t = "sensor {sensor} [--target {target}] [--dry-run {dry_run}] --requested-by {caller}";
    assert_eq!(argv(t, &args(json!({"sensor": "files"})), "wren"), vec!["sensor", "files", "--requested-by", "wren"]);
    assert_eq!(
        argv(t, &args(json!({"sensor": "code", "target": "/repo", "dry_run": true})), "jeff"),
        vec!["sensor", "code", "--target", "/repo", "--dry-run", "--requested-by", "jeff"]
    );
    assert_eq!(argv(t, &args(json!({"sensor": "code", "dry_run": false})), "jeff"),
        vec!["sensor", "code", "--requested-by", "jeff"], "a false flag is absent");
}

/// NEGATIVE PROOF (Silas): a value that would be a shell injection lands as
/// ONE literal argument; the argv is never a shell string.
#[test]
fn a_hostile_value_is_one_literal_argument() {
    let t = "sensor {sensor} [--target {target}]";
    let v = argv(t, &args(json!({"sensor": "code", "target": "/repo; rm -rf ~"})), "silas");
    assert_eq!(v, vec!["sensor", "code", "--target", "/repo; rm -rf ~"]);
}

// #4513 reopen 1 — Silas measured herald-run exiting 2 on live :3342:
// CHORUS_ROLE was unset in the child. A real child must see the owner.
#[test]
fn the_child_runs_as_the_skill_owner() {
    let out = std::process::Command::new("/usr/bin/env")
        .env_clear()
        .envs(child_env("silas", "t", "00-trace"))
        .output()
        .expect("env runs");
    let seen = String::from_utf8_lossy(&out.stdout);
    assert!(seen.lines().any(|l| l == "CHORUS_ROLE=silas"), "child env: {seen}");
    assert!(seen.lines().any(|l| l == "DEPLOY_ROLE=silas"), "child env: {seen}");
    assert!(seen.lines().any(|l| l == "CHORUS_TRACE=00-trace"), "child env: {seen}");
}
