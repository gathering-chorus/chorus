// @test-type: unit
// @domain: identity
//! #4444 reopen 2 — Jeff, 2026-10-14 08:15: "generating properties on agent
//! config rather than hard coding". A role session's runtime (agy, Gemini CLI,
//! Claude Code) is an AgentRuntime row; athena-gen writes the table the
//! launcher and the hook adapter read. Nothing in the table is hand-written.
use std::path::PathBuf;

fn repo() -> PathBuf {
    // run-time, not env!(): the werk-test 4030 guard
    PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR")).join("../../..")
}

const SHAPE: &str = r#"
@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
chorus:AgentRuntimeShape a sh:NodeShape ; sh:targetClass chorus:AgentRuntime ;
    sh:property [ sh:path chorus:runtimeBinary ; sh:minCount 1 ; sh:maxCount 1 ] ;
    sh:property [ sh:path chorus:eventBeforePrompt ; sh:minCount 1 ; sh:maxCount 1 ] ;
    sh:property [ sh:path chorus:runtimeToolMap ] .
"#;

fn table(rows: &str) -> Result<athena_gen::model::RuntimeTable, String> {
    // one dir per call: tests run in parallel, and two fixtures can be the same length
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("agent-runtimes-4444-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("fixture.ttl");
    std::fs::write(&f, format!("{SHAPE}{rows}")).unwrap();
    let (store, skipped) = athena_gen::model::load(&[f]).unwrap();
    assert!(skipped.is_empty(), "{skipped:?}");
    athena_gen::model::agent_runtimes(&store).map_err(|e| e.0)
}

const AGY: &str = r#"chorus:runtime-agy a chorus:AgentRuntime ; chorus:runtimeBinary "agy" ; chorus:eventBeforePrompt "PreInvocation" ;
    chorus:runtimeToolMap "run_command=Bash" .
<https://jeffbridwell.com/chorus#principal-abby-normal> chorus:agentRuntime chorus:runtime-agy .
"#;

#[test]
fn a_runtime_row_becomes_the_table_and_a_principal_is_assigned_to_it() {
    let t = table(AGY).unwrap();
    let v: serde_json::Value = serde_json::from_str(&athena_gen::emit::agent_runtimes_json(&t)).unwrap();
    assert_eq!(v["runtimes"]["agy"]["runtimeBinary"], "agy", "a maxCount-1 property is one value");
    assert_eq!(v["runtimes"]["agy"]["runtimeToolMap"], serde_json::json!(["run_command=Bash"]), "a many property is a list");
    assert_eq!(v["runsOn"]["abby-normal"], "agy");
}

/// NEGATIVE PROOF: the event name in the table is the row's, so changing the
/// row changes what the hooks file names.
#[test]
fn changing_the_rows_event_changes_the_table() {
    let t = table(&AGY.replace("PreInvocation", "BeforePrompt2")).unwrap();
    assert_eq!(t.runtimes["agy"]["eventBeforePrompt"], vec!["BeforePrompt2".to_string()]);
}

/// NEGATIVE PROOF: a row missing a property the shape requires is refused by
/// that property's name, and so is a principal assigned to no runtime row.
#[test]
fn a_row_missing_a_required_property_is_refused_by_name() {
    let why = table(&AGY.replace(r#"chorus:eventBeforePrompt "PreInvocation" ;"#, "")).unwrap_err();
    assert!(why.contains("AgentRuntime agy is missing eventBeforePrompt"), "{why}");
    let why = table(&AGY.replace("chorus:agentRuntime chorus:runtime-agy", "chorus:agentRuntime chorus:runtime-nope")).unwrap_err();
    assert!(why.contains("runs on nope, which is not an AgentRuntime row"), "{why}");
}

#[test]
fn the_committed_table_is_what_the_generator_writes_today() {
    let root = repo();
    let (store, skipped) = athena_gen::model::load(&athena_gen::model::model_files(&root)).unwrap();
    assert!(skipped.is_empty(), "{skipped:?}");
    let want = athena_gen::emit::agent_runtimes_json(&athena_gen::model::agent_runtimes(&store).unwrap());
    let have = std::fs::read_to_string(root.join("platform/services/shared/agent_runtimes.json")).unwrap();
    assert_eq!(have, want, "regenerate: athena-gen runtimes --root . --out platform/services/shared/agent_runtimes.json");
    assert!(want.contains("\"abby-normal\": \"agy\""), "Abby runs on agy in the model");
}
