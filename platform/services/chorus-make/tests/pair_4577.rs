// @test-type: unit
// @domain: pipelines
//! #4577 — /pair as a dagu pipeline: a human step a role answers (the
//! navigator's green), a hand-off that enqueues cicd, and a failure handler
//! that sends the pair back to slice.
use chorus_make::{generate_straight, parse_pipe, parse_rows, Pipe, Row};

fn fixture(name: &str) -> String {
    // run time, not env!(): the werk-test 4030 guard refuses a compile-time path
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    std::fs::read_to_string(format!("{dir}/tests/fixtures/{name}")).expect("fixture")
}

fn rows() -> Vec<Row> {
    parse_rows(&fixture("pair-rows.tsv")).expect("rows parse")
}

fn pipe() -> Pipe {
    parse_pipe(&fixture("pair-pipe.tsv")).unwrap()
}

fn known() -> Vec<String> {
    ["cicd", "cicd-test", "nightly", "athena", "pair"].iter().map(|s| s.to_string()).collect()
}

#[test]
fn the_pair_rows_generate_the_golden() {
    assert_eq!(generate_straight("pair", &pipe(), &rows(), &known()).unwrap(), fixture("pair-golden.yaml"));
}

#[test]
fn the_green_is_the_navigators_human_step_and_its_check_reads_who_answered() {
    let y = generate_straight("pair", &pipe(), &rows(), &known()).unwrap();
    assert!(y.contains("    id: skill_pair_green\n    action: human.task\n"), "{y}");
    assert!(y.contains("(${NAVIGATOR} answers)"), "{y}");
    assert!(y.contains("            enum: [green, stop]\n        required: [by, decision]\n"), "{y}");
    assert!(y.contains("pair check ${CARD} ${NAVIGATOR} ${skill_pair_green.outputs.by}"), "{y}");
}

#[test]
fn negative_proof_a_human_row_with_no_by_is_refused() {
    let mut r = rows();
    let g = r.iter_mut().find(|x| x.executor == "human").unwrap();
    g.with_args = String::new();
    let errs = generate_straight("pair", &pipe(), &r, &known()).unwrap_err();
    assert!(errs.iter().any(|e| e.contains("no by= and choices=")), "{errs:?}");
}

#[test]
fn negative_proof_a_human_row_answered_by_a_param_the_pipeline_lacks_is_refused() {
    let mut r = rows();
    r.iter_mut().find(|x| x.executor == "human").unwrap().with_args = "by=ACCEPTER choices=go|no".into();
    let errs = generate_straight("pair", &pipe(), &r, &known()).unwrap_err();
    assert!(errs.iter().any(|e| e.contains("ACCEPTER, which is not one of the pipeline's params")), "{errs:?}");
}

#[test]
fn the_hand_off_enqueues_cicd_and_the_local_proof_waits_for_cicd_test() {
    let y = generate_straight("pair", &pipe(), &rows(), &known()).unwrap();
    assert!(y.contains("    command: \"env -u CHORUS_ROLE -u ROLE dagu --config ${CHORUS_HOME}/platform/config/dagu.yaml enqueue cicd -- CARD=${CARD} ROLE=${ROLE}\"\n"), "{y}");
    // #4675 — the local proof waits its turn on cicd-test's queue, never a sub-run beside a cicd run.
    assert!(y.contains("    command: \"${CHORUS_HOME}/platform/scripts/dagu-enqueue-wait cicd-test CARD=${CARD} ROLE=${ROLE} TRACE=${CHORUS_TRACE_ID} SCOPE=diff\"\n"), "{y}");
    assert!(!y.contains("dag: cicd-test\n"), "{y}");
}

/// #4577 pair run 1 (10-18 07:43): dagu ran the dag.enqueue hand-off as a
/// sub-DAG and refused it, "human task steps are not allowed in sub-DAGs:
/// cicd". Negative proof: no step hands cicd (which waits on Jeff's go) to
/// dagu's sub-DAG actions.
#[test]
fn negative_proof_cicd_is_never_handed_off_as_a_sub_dag() {
    let y = generate_straight("pair", &pipe(), &rows(), &known()).unwrap();
    assert!(!y.contains("action: dag.enqueue"), "{y}");
    assert!(!y.contains("dag: cicd\n"), "{y}");
}

#[test]
fn a_failure_runs_the_pairs_own_handler_not_the_builders_nudge() {
    let y = generate_straight("pair", &pipe(), &rows(), &known()).unwrap();
    assert!(y.contains("  failure:\n    command: \"${CHORUS_HOME}/platform/scripts/pair red ${CARD} ${ROLE} ${NAVIGATOR} ${DAG_RUN_ID}\"\n"), "{y}");
    assert!(!y.contains("ops-nudge"), "{y}");
    // negative proof: without onFailure the builder's nudge comes back
    let mut p = pipe();
    p.on_failure.clear();
    let y = generate_straight("pair", &p, &rows(), &known()).unwrap();
    assert!(y.contains("ops-nudge ${ROLE}") && !y.contains("pair red"), "{y}");
}

#[test]
fn the_graph_read_carries_on_failure_from_the_pipeline_row() {
    let tsv = "?queue\t?schedule\t?params\t?traceParam\t?onAbort\t?onExit\t?onFailure\n\"pair\"\t\t\"CARD ROLE NAVIGATOR\"\t\t\t\t\"${CHORUS_HOME}/platform/scripts/pair red ${CARD} ${ROLE} ${NAVIGATOR} ${DAG_RUN_ID}\"\n";
    let p = parse_pipe(&chorus_make::pipe_tsv_from_sparql(tsv)).unwrap();
    assert_eq!(p, pipe());
    assert!(chorus_make::pipe_query("pair").contains("chorus:onFailureRun ?onFailure"));
}
