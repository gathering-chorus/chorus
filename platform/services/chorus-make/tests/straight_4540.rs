// @test-type: unit
// @domain: pipelines
//! #4540 — chorus-make writes the pipelines whose Pipeline row says how they
//! run: cicd-test (werk-test as steps) and the 03:00 nightly that calls it.
use chorus_make::{generate_straight, parse_pipe, parse_rows, Row};

fn fixture(name: &str) -> String {
    // run time, not env!(): the werk-test 4030 guard refuses a compile-time path
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    std::fs::read_to_string(format!("{dir}/tests/fixtures/{name}")).expect("fixture")
}

fn rows(name: &str) -> Vec<Row> {
    parse_rows(&fixture(name)).expect("rows parse")
}

fn known() -> Vec<String> {
    ["cicd", "cicd-test", "nightly", "athena"].iter().map(|s| s.to_string()).collect()
}

#[test]
fn cicd_test_and_nightly_generate_their_goldens() {
    let pipe = parse_pipe(&fixture("cicd-test-pipe.tsv")).unwrap();
    assert_eq!(generate_straight("cicd-test", &pipe, &rows("cicd-test-rows.tsv"), &known()).unwrap(), fixture("cicd-test-golden.yaml"));
    let pipe = parse_pipe(&fixture("nightly-pipe.tsv")).unwrap();
    assert_eq!(generate_straight("nightly", &pipe, &rows("nightly-rows.tsv"), &known()).unwrap(), fixture("nightly-golden.yaml"));
}

#[test]
fn the_child_carries_the_parents_trace_and_the_nightly_calls_it_with_scope_all() {
    let child = generate_straight("cicd-test", &parse_pipe(&fixture("cicd-test-pipe.tsv")).unwrap(), &rows("cicd-test-rows.tsv"), &known()).unwrap();
    assert!(child.contains("  - CHORUS_TRACE_ID: ${TRACE}\n"), "{child}");
    let nightly = generate_straight("nightly", &parse_pipe(&fixture("nightly-pipe.tsv")).unwrap(), &rows("nightly-rows.tsv"), &known()).unwrap();
    assert!(nightly.contains("    action: dag.run\n    with:\n      dag: cicd-test\n"), "{nightly}");
    assert!(nightly.contains("TRACE=${CHORUS_TRACE_ID} SCOPE=all"), "{nightly}");
    assert!(nightly.contains("schedule: \"0 3 * * *\"\nqueue: nightly\n"), "{nightly}");
    assert!(nightly.contains("  abort:\n") && nightly.contains("  exit:\n"), "{nightly}");
}

/// Wren 10-16: every type runs to the end and only the verdict fails.
#[test]
fn the_type_steps_continue_on_failure_and_the_verdict_does_not() {
    let child = generate_straight("cicd-test", &parse_pipe(&fixture("cicd-test-pipe.tsv")).unwrap(), &rows("cicd-test-rows.tsv"), &known()).unwrap();
    let block = |name: &str| -> String {
        let i = child.find(&format!("  - name: {name}\n")).unwrap();
        let rest = &child[i + 4..];
        rest[..rest.find("\n  - name: ").unwrap_or(rest.len())].to_string()
    };
    for s in ["skill-werk-test-run", "skill-werk-test-perf", "skill-werk-test-report"] {
        assert!(block(s).contains("failure: true"), "{s}: {}", block(s));
    }
    assert!(!block("skill-werk-test-verdict").contains("continue_on"), "the verdict is the gate");
}

/// AC7 NEGATIVE PROOF: change the modeled order and the generated order changes.
#[test]
fn negative_proof_swapping_two_modeled_steps_swaps_them_in_the_dag() {
    let pipe = parse_pipe(&fixture("cicd-test-pipe.tsv")).unwrap();
    let names = |rs: &[Row]| -> Vec<String> {
        generate_straight("cicd-test", &pipe, rs, &known()).unwrap().lines()
            .filter_map(|l| l.strip_prefix("  - name: ")).map(String::from).collect()
    };
    let mut rs = rows("cicd-test-rows.tsv");
    let before = names(&rs);
    for r in rs.iter_mut() {
        match r.step.as_str() {
            "run" => r.step_order = 4,
            "perf" => r.step_order = 3,
            _ => {}
        }
    }
    let after = names(&rs);
    assert_ne!(before, after);
    let pos = |v: &[String], n: &str| v.iter().position(|x| x == n).unwrap();
    assert!(pos(&after, "skill-werk-test-perf") < pos(&after, "skill-werk-test-run"));
}

/// Wren 10-16 NEGATIVE PROOF: a step that calls a pipeline the graph does not
/// hold is refused, never written as a call to nothing.
#[test]
fn negative_proof_a_call_to_a_pipeline_the_graph_lacks_is_refused() {
    let pipe = parse_pipe(&fixture("nightly-pipe.tsv")).unwrap();
    let errs = generate_straight("nightly", &pipe, &rows("nightly-rows.tsv"), &["cicd".to_string()]).unwrap_err();
    assert!(errs.iter().any(|e| e.contains("calls pipeline 'cicd-test', which the graph does not hold")), "{errs:?}");
}

#[test]
fn negative_proof_a_pipeline_row_with_no_queue_or_a_stray_field_is_refused() {
    assert!(parse_pipe("params\tCARD\n").is_err(), "no queue");
    assert!(parse_pipe("queue\tq\nqeueu\tx\n").unwrap_err().iter().any(|e| e.contains("unknown Pipeline field 'qeueu'")));
    assert!(parse_pipe("queue\tq\nparams\tCARD\ntraceParam\tTRACE\n").is_err(), "a trace param it does not take");
}
