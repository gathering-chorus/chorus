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
    sh:property [ sh:path chorus:host ; sh:minCount 1 ; sh:datatype xsd:string ; sh:order 1 ; sh:name "host" ;
                  sh:in ( "a" "b" ) ; sh:description "Which host." ] ;
    sh:property [ sh:path chorus:times ; sh:datatype xsd:integer ; sh:order 2 ; sh:name "times" ] ;
    sh:property [ sh:path chorus:quiet ; sh:datatype xsd:boolean ; sh:order 3 ; sh:name "quiet" ] .
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
    assert!(want.contains("HeraldRunInput"), "herald_run's input is in the model");
}
