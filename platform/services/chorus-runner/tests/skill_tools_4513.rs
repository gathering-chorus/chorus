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

// #4514 (Silas 19:07) — an athena verb's args is a list: each item is its own
// word, in order, never one JSON string; a non-string item is refused.
#[test]
fn a_list_argument_is_spliced_one_word_per_item_and_checked_item_by_item() {
    use chorus_runner::skills::coerce_args;
    let schema = json!({ "type": "object", "properties": { "args": { "type": "array", "items": { "type": "string" } } }, "required": ["args"] })
        .as_object().unwrap().clone();
    let a = args(json!({ "args": ["scope", "/r", "a..b"] }));
    check_args(&a, &schema).unwrap();
    assert_eq!(argv("{args}", &a, "wren"), vec!["scope", "/r", "a..b"]);
    // NEGATIVE PROOF: a number inside the list is refused, not stringified
    assert!(check_args(&args(json!({ "args": ["scope", 7] })), &schema).is_err());
    assert!(check_args(&args(json!({ "args": [] })), &schema).is_err(), "an empty list passed");
    // a lone value from a command line becomes a one-item list
    assert_eq!(coerce_args(&args(json!({ "args": "scope" })), &schema)["args"], json!(["scope"]));
}

// #4514 part 2 (Silas) — werk-test reads only `--domain=value`: a placeholder
// inside a word is filled, and the word is dropped when the value is absent.
#[test]
fn a_placeholder_inside_a_word_is_filled_or_the_word_dropped() {
    let a = args(json!({ "card": "4514", "domain": "services" }));
    assert_eq!(argv("{card} {caller} [--domain={domain}] [--type={type}]", &a, "wren"), vec!["4514", "wren", "--domain=services"]);
}

// #4515 (Silas) — chorus-cards-tag runs `untag {id} {category}:{value}`: every
// placeholder in a word is filled, and the word drops if any one is absent.
#[test]
fn every_placeholder_inside_a_word_is_filled() {
    let a = args(json!({ "id": 4515, "category": "chunk", "value": "ops" }));
    assert_eq!(argv("untag {id} {category}:{value}", &a, "silas"), vec!["untag", "4515", "chunk:ops"]);
    // NEGATIVE PROOF: one missing value drops the word, never leaves a literal {value}
    let half = args(json!({ "id": 4515, "category": "chunk" }));
    assert_eq!(argv("untag {id} [{category}:{value}]", &half, "silas"), vec!["untag", "4515"]);
}

/// #4515 — a value that does not match the shape's sh:pattern is refused by
/// name. NEGATIVE PROOF: the same table with no pattern lets the value through,
/// and a pattern that is not a regex refuses rather than passes.
#[test]
fn a_value_that_breaks_the_pattern_is_refused() {
    let schema = |pat: Option<&str>| {
        let mut to = json!({"type": "string"});
        if let Some(p) = pat { to["pattern"] = json!(p); }
        json!({"type": "object", "properties": {"to": to}, "required": ["to"]}).as_object().unwrap().clone()
    };
    let pat = schema(Some("^[a-z][a-z0-9-]*$"));
    assert!(check_args(&args(json!({"to": "wren"})), &pat).is_ok());
    assert_eq!(check_args(&args(json!({"to": "Wren; rm"})), &pat).unwrap_err(), "to must match ^[a-z][a-z0-9-]*$");
    assert!(check_args(&args(json!({"to": "Wren; rm"})), &schema(None)).is_ok(), "no pattern, no check");
    assert!(check_args(&args(json!({"to": "wren"})), &schema(Some("(["))).unwrap_err().contains("not a regex"));
}
