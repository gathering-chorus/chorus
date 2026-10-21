// @test-type: unit
// @domain: pipelines
//! #4675 — a pipeline row marked queued=true runs its child ON THE CHILD'S QUEUE:
//! the step calls platform/scripts/dagu-enqueue-wait (enqueue, then wait for the
//! verdict) instead of a dag.run sub-run, which ignores its queue (#4494). The
//! pair's local proof uses it so a pair's tests wait behind cicd runs.
use chorus_make::{generate_straight, parse_pipe, parse_rows, Pipe, Row};

fn fixture(name: &str) -> String {
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    std::fs::read_to_string(format!("{dir}/tests/fixtures/{name}")).expect("fixture")
}

fn pipe() -> Pipe {
    parse_pipe(&fixture("pair-pipe.tsv")).unwrap()
}

fn known() -> Vec<String> {
    ["cicd", "cicd-test", "nightly", "athena", "pair"].iter().map(|s| s.to_string()).collect()
}

fn rows_with(local_proof_args: &str) -> Vec<Row> {
    let base = fixture("pair-rows.tsv");
    let row = "cicd-test\t\t\t\tSCOPE=diff queued=true";
    assert!(base.contains(row), "fixture row moved");
    parse_rows(&base.replace(row, &format!("cicd-test\t\t\t\t{local_proof_args}"))).expect("rows parse")
}

#[test]
fn a_queued_row_enqueues_the_child_and_waits() {
    let y = generate_straight("pair", &pipe(), &rows_with("SCOPE=diff queued=true"), &known()).unwrap();
    assert!(
        y.contains("    command: \"${CHORUS_HOME}/platform/scripts/dagu-enqueue-wait cicd-test CARD=${CARD} ROLE=${ROLE} TRACE=${CHORUS_TRACE_ID} SCOPE=diff\"\n"),
        "{y}"
    );
    assert!(!y.contains("queued=true"), "the marker is not a param: {y}");
    assert!(!y.contains("dag: cicd-test"), "no sub-run left: {y}");
}

// NEGATIVE PROOF: without the marker the local proof stays a dag.run sub-run (today's shape)
#[test]
fn an_unmarked_row_is_still_a_sub_run() {
    let y = generate_straight("pair", &pipe(), &rows_with("SCOPE=diff"), &known()).unwrap();
    assert!(y.contains("    action: dag.run\n    with:\n      dag: cicd-test\n"), "{y}");
    assert!(!y.contains("dagu-enqueue-wait"), "{y}");
}
