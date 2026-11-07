// @test-type: unit
// @domain: pipelines
//! #4682 — clearing chunk lifecycle as Dagu pipelines generated from the graph:
//! 1. clearing-plan: chunk -> prioritize -> sequence -> plan-go -> pull -> pair
//! 2. clearing-evaluate: evaluate -> loop (re-enqueue clearing-plan)

use chorus_make::{generate_straight, parse_pipe, parse_rows, Pipe, Row};

fn fixture(name: &str) -> String {
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    std::fs::read_to_string(format!("{dir}/tests/fixtures/{name}")).expect("fixture")
}

fn rows(name: &str) -> Vec<Row> {
    parse_rows(&fixture(name)).expect("rows parse")
}

fn pipe(name: &str) -> Pipe {
    parse_pipe(&fixture(name)).unwrap()
}

fn known() -> Vec<String> {
    [
        "cicd", "cicd-test", "nightly", "athena", "pair", "demo", "card",
        "clearing-plan", "clearing-evaluate",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

#[test]
fn the_clearing_plan_pipeline_enforces_chunk_before_cards() {
    let y = generate_straight(
        "clearing-plan",
        &pipe("clearing-plan-pipe.tsv"),
        &rows("clearing-plan-rows.tsv"),
        &known(),
    )
    .unwrap();

    let chunk = y.find("clearing-chunk enforce ${CHUNK}").expect("chunk enforce step");
    let prio = y.find("clearing-chunk prioritize ${CHUNK}").expect("cards prioritize step");
    let seq = y.find("clearing-chunk sequence ${CHUNK}").expect("cards sequence step");
    let go = y.find("id: skill_clearing_plan_go").expect("human plan-go step");

    assert!(chunk < prio, "clearing-chunk enforce runs before cards prioritize\n{y}");
    assert!(prio < seq, "prioritize runs before sequence\n{y}");
    assert!(seq < go, "sequence runs before plan-go\n{y}");
    assert!(y.contains("queue: clearing\n"), "{y}");
    assert!(y.contains("  - CARD: \"\"\n"), "CARD declared as param\n{y}");
    assert!(y.contains("output: CARD"), "sequence step captures CARD output\n{y}");
    assert!(y.contains("prompt: \"Jeff approval before pulling to WIP for #${CARD}? (${JEFF} answers)\""), "Jeff answers plan-go\n{y}");
    assert!(y.contains("navigator:\n            type: string"), "plan-go form has navigator field\n{y}");
    assert!(y.contains("required: [by, decision, navigator]"), "navigator is required in plan-go form\n{y}");
    assert!(y.contains("clearing-chunk check \\\"${CARD}\\\" \\\"${skill_clearing_plan_go.outputs.by}\\\" \\\"${skill_clearing_plan_go.outputs.decision}\\\" \\\"${skill_clearing_plan_go.outputs.navigator}\\\" \\\"${ROLE}\\\""), "plan-check validates Jeff and navigator with quoted args\n{y}");
    assert!(y.contains("continue_on:\n      exit_code: [2]\n      mark_success: true"), "stop exit code 2 marked success\n{y}");
    assert!(y.contains("preconditions:\n      - condition: \"${skill_clearing_plan_go.outputs.decision}\"\n        expected: \"go\""), "pull gated on go\n{y}");
    assert!(y.contains("werk-card pull ${CARD} ${ROLE}"), "pull step calls werk-card pull\n{y}");
    assert!(y.contains("NAVIGATOR=${skill_clearing_plan_go.outputs.navigator}"), "pair enqueue passes navigator from plan-go\n{y}");
}

#[test]
fn the_clearing_evaluate_pipeline_runs_gate_and_loops() {
    let y = generate_straight(
        "clearing-evaluate",
        &pipe("clearing-evaluate-pipe.tsv"),
        &rows("clearing-evaluate-rows.tsv"),
        &known(),
    )
    .unwrap();

    assert!(y.contains("id: skill_clearing_evaluate"), "evaluate human step present\n{y}");
    assert!(y.contains("queue: werk-land\n"), "runs on werk-land queue\n{y}");
    assert!(y.contains("skill-clearing-chunk-of"), "chunk-of step present\n{y}");
    assert!(y.contains("preconditions:\n      - condition: \"$CHUNK\"\n        expected: \"re:.+\""), "evaluate gated on non-empty chunk\n{y}");
    assert!(y.contains("preconditions:\n      - condition: \"${skill_clearing_evaluate.outputs.decision}\"\n        expected: \"resequence\""), "loop gated on resequence decision\n{y}");
    assert!(y.contains("skill-clearing-switch"), "switch step present\n{y}");
    assert!(y.contains("preconditions:\n      - condition: \"${skill_clearing_evaluate.outputs.decision}\"\n        expected: \"switch\""), "switch gated on switch decision\n{y}");
    assert!(y.contains("clearing-chunk switch ${CARD} ${CHUNK} ${ROLE} ${JEFF}"), "switch command runs clearing-chunk switch\n{y}");
    assert!(y.contains("NAVIGATOR=${NAVIGATOR} JEFF=${JEFF}"), "loop passes NAVIGATOR and JEFF\n{y}");
}

#[test]
fn negative_proof_clearing_evaluate_without_switch_step_differs() {
    let p = pipe("clearing-evaluate-pipe.tsv");
    let full = generate_straight("clearing-evaluate", &p, &rows("clearing-evaluate-rows.tsv"), &known()).unwrap();

    let filtered: Vec<Row> = rows("clearing-evaluate-rows.tsv")
        .into_iter()
        .filter(|r| r.skill != "skill-clearing-switch")
        .collect();
    let without_switch = generate_straight("clearing-evaluate", &p, &filtered, &known()).unwrap();

    assert_ne!(full, without_switch);
    assert!(!without_switch.contains("skill-clearing-switch"));
}

#[test]
fn negative_proof_clearing_plan_without_chunk_cap_changes_pipeline() {
    let p = pipe("clearing-plan-pipe.tsv");
    let full = generate_straight("clearing-plan", &p, &rows("clearing-plan-rows.tsv"), &known()).unwrap();

    // Remove the chunk enforce row
    let filtered: Vec<Row> = rows("clearing-plan-rows.tsv")
        .into_iter()
        .filter(|r| r.step != "chunk")
        .collect();
    let without_chunk = generate_straight("clearing-plan", &p, &filtered, &known()).unwrap();

    assert_ne!(full, without_chunk);
    assert!(!without_chunk.contains("skill-clearing-chunk"));
}

#[test]
fn negative_proof_precondition_without_equals_separator_is_refused() {
    let mut bad_rows = rows("clearing-plan-rows.tsv");
    bad_rows[0].with_args = "precondition=malformed_without_equals".to_string();
    let errs = chorus_make::check(&bad_rows);
    assert!(!errs.is_empty(), "must fail check");
    assert!(errs.iter().any(|e| e.contains("missing '=' separator")), "{errs:?}");
}
