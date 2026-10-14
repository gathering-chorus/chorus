// @test-type: unit
// @domain: pipelines
//! #4514 AC4 — a pipeline step whose skill is a runner tool calls the verb
//! through the runner; a step whose skill is not stays exactly as today.
use chorus_make::{generate, parse_rows};

const ROWS: &str = "commit\t1\t1\tskill-werk-commit\twerk-commit\tdeterministic\twerk-commit\t{card} {caller} [--atomic {atomic}]\t\tWerkCardInput\tatomic=true
build\t2\t1\tskill-werk-build\twerk-build\tdeterministic\twerk-build\t${CARD} ${ROLE} --target werk\t
demo\t3\t1\tskill-werk-go\tJeff's go\thuman\t\t\t
";

#[test]
fn a_runner_tool_step_calls_the_runner_with_its_inputs_signed_as_role() {
    let y = generate("cicd", &parse_rows(ROWS).unwrap()).unwrap();
    assert!(y.contains("command: \"chorus-runner call werk-commit card=${CARD} atomic=true\""), "{y}");
    assert!(!y.contains("caller="), "the caller is the signer, never an argument: {y}");
}

#[test]
fn negative_proof_a_step_without_an_input_shape_runs_its_binary_as_today() {
    let y = generate("cicd", &parse_rows(ROWS).unwrap()).unwrap();
    assert!(y.contains("command: \"werk-build ${CARD} ${ROLE} --target werk\""), "{y}");
    assert!(!y.contains("chorus-runner call werk-build"), "{y}");
}

/// #4514 reopen 1 — NEGATIVE PROOF: a runner step that leaves a required
/// argument unfilled (werk-sync's mode) is refused at generate, by name.
#[test]
fn a_runner_step_missing_a_required_argument_is_refused_by_name() {
    let rows = "land\t1\t1\tskill-werk-sync\twerk-sync\tdeterministic\twerk-sync\t{mode}\t\tWerkSyncInput\n\
demo\t2\t1\tskill-werk-go\tJeff's go\thuman\t\t\t\n";
    let errs = generate("cicd", &parse_rows(rows).unwrap()).unwrap_err();
    assert!(errs.iter().any(|e| e.contains("werk-sync without mode")), "{errs:?}");
    let fixed = rows.replace("\tWerkSyncInput\n", "\tWerkSyncInput\tmode=ff\n");
    let y = generate("cicd", &parse_rows(&fixed).unwrap()).unwrap();
    assert!(y.contains("chorus-runner call werk-sync mode=ff"), "{y}");
}
