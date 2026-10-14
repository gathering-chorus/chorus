// @test-type: unit
// @domain: services
//! #4513 — a tool's arguments come from a ToolInput shape in the model, and
//! athena-gen writes them as the JSON Schema the tool runner offers. The
//! runner's table (platform/services/chorus-runner/src/tool_inputs.json) is
//! never hand-written: it must be exactly what the generator writes today.
use std::path::PathBuf;

fn repo() -> PathBuf {
    // run-time, not env!(): the werk-test 4030 guard
    PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR")).join("../../..")
}

const FIXTURE: &str = r#"
@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
chorus:PingInput a sh:NodeShape , chorus:ToolInput ;
    sh:property [ sh:path chorus:host ; sh:minCount 1 ; sh:maxCount 1 ; sh:datatype xsd:string ; sh:order 1 ; sh:name "host" ;
                  sh:in ( "a" "b" ) ; sh:description "Which host." ] ;
    sh:property [ sh:path chorus:times ; sh:maxCount 1 ; sh:datatype xsd:integer ; sh:order 2 ; sh:name "times" ] ;
    sh:property [ sh:path chorus:quiet ; sh:maxCount 1 ; sh:datatype xsd:boolean ; sh:order 3 ; sh:name "quiet" ] ;
    sh:property [ sh:path chorus:extra ; sh:datatype xsd:string ; sh:order 4 ; sh:name "extra" ] ;
    sh:property [ sh:path chorus:owner ; sh:maxCount 1 ; sh:datatype xsd:string ; sh:order 5 ; sh:name "owner" ; chorus:namesRole "peer" ] ;
    sh:property [ sh:path chorus:slug ; sh:maxCount 1 ; sh:datatype xsd:string ; sh:order 6 ; sh:name "slug" ; sh:pattern "^[a-z][a-z0-9-]*$" ] .
# A class shape: a door collection, never a tool input.
chorus:WidgetShape a sh:NodeShape ; sh:targetClass chorus:Widget ;
    sh:property [ sh:path chorus:size ; sh:datatype xsd:integer ] .
"#;

fn fixture_inputs() -> Vec<(String, Vec<athena_gen::model::ToolArg>)> {
    let dir = std::env::temp_dir().join(format!("tool-inputs-4513-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("fixture.ttl");
    std::fs::write(&f, FIXTURE).unwrap();
    let (store, skipped) = athena_gen::model::load(&[f]).unwrap();
    assert!(skipped.is_empty(), "{skipped:?}");
    athena_gen::model::tool_inputs(&store).unwrap()
}

#[test]
fn a_tool_input_shape_becomes_a_json_schema() {
    let inputs = fixture_inputs();
    let v: serde_json::Value = serde_json::from_str(&athena_gen::emit::tool_inputs_json(&inputs)).unwrap();
    let ping = &v["PingInput"];
    assert_eq!(ping["type"], "object");
    assert_eq!(ping["required"], serde_json::json!(["host"]));
    assert_eq!(ping["properties"]["host"]["enum"], serde_json::json!(["a", "b"]));
    assert_eq!(ping["properties"]["times"]["type"], "integer");
    assert_eq!(ping["properties"]["quiet"]["type"], "boolean");
    assert_eq!(ping["additionalProperties"], false);
}

/// #4514 — no sh:maxCount is a list (an athena verb's args, in order).
/// NEGATIVE PROOF: a field WITH sh:maxCount 1 never becomes a list.
#[test]
fn a_field_with_no_max_count_is_a_list_and_one_with_is_not() {
    let v: serde_json::Value = serde_json::from_str(&athena_gen::emit::tool_inputs_json(&fixture_inputs())).unwrap();
    let p = &v["PingInput"]["properties"];
    assert_eq!(p["extra"]["type"], "array");
    assert_eq!(p["extra"]["items"]["type"], "string");
    assert_eq!(p["host"]["type"], "string", "a single field became a list");
}

/// NEGATIVE PROOF: a class shape is a door collection, never a tool input, so
/// it never reaches the runner's table.
#[test]
fn a_class_shape_is_not_a_tool_input() {
    let names: Vec<String> = fixture_inputs().into_iter().map(|(n, _)| n).collect();
    assert_eq!(names, vec!["PingInput"]);
}

#[test]
fn the_runners_table_is_what_the_generator_writes_today() {
    let root = repo();
    let (store, skipped) = athena_gen::model::load(&athena_gen::model::model_files(&root)).unwrap();
    assert!(skipped.is_empty(), "{skipped:?}");
    let want = athena_gen::emit::tool_inputs_json(&athena_gen::model::tool_inputs(&store).unwrap());
    let have = std::fs::read_to_string(root.join("platform/services/chorus-runner/src/tool_inputs.json")).unwrap();
    assert_eq!(have, want, "regenerate: athena-gen tool-inputs --root . --out platform/services/chorus-runner/src/tool_inputs.json");
    assert!(want.contains("HeraldRunInput"), "borg-tend-herald's input is in the model");
}

/// #4515 — `chorus:namesRole` reaches the runner's table, so the runner can
/// refuse a role the door does not list. NEGATIVE PROOF: an unmarked field
/// carries no namesRole, so it is never checked as a role.
#[test]
fn a_role_field_says_so_and_no_other_field_does() {
    let v: serde_json::Value = serde_json::from_str(&athena_gen::emit::tool_inputs_json(&fixture_inputs())).unwrap();
    let p = &v["PingInput"]["properties"];
    assert_eq!(p["owner"]["namesRole"], "peer");
    assert!(p["host"].get("namesRole").is_none(), "an unmarked field was marked a role");
}

/// #4515 — sh:pattern reaches the runner's table (athena-gen dropped it, so
/// the nudge recipient's pattern was never checked). NEGATIVE PROOF: a field
/// with no sh:pattern carries none.
#[test]
fn a_pattern_on_the_shape_reaches_the_table_and_no_other_field_gets_one() {
    let v: serde_json::Value = serde_json::from_str(&athena_gen::emit::tool_inputs_json(&fixture_inputs())).unwrap();
    let p = &v["PingInput"]["properties"];
    assert_eq!(p["slug"]["pattern"], "^[a-z][a-z0-9-]*$");
    assert!(p["host"].get("pattern").is_none(), "a field with no sh:pattern got one");
}
