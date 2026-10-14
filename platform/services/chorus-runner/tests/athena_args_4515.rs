// @test-type: unit
// @domain: services
//! #4515 — the athena pipeline's steps pass AthenaVerbInput's `args` list as
//! repeated `args=` pairs (chorus-make writes the StepSkill's withArguments into
//! the dagu command). Read the fixture's own withArgs, fill ${CARD} the way dagu
//! does, and run them through the runner's real path: pairs → arguments →
//! coerced to the shape → checked → the verb's argv.
use chorus_runner::client::args_from_pairs;
use chorus_runner::skills::{argv, check_args, coerce_args};
use serde_json::{Map, Value};

fn dir() -> String {
    std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR")
}

fn athena_schema() -> Map<String, Value> {
    let all: Value = serde_json::from_str(&std::fs::read_to_string(format!("{}/src/tool_inputs.json", dir())).unwrap()).unwrap();
    all["AthenaVerbInput"].as_object().cloned().expect("AthenaVerbInput in tool_inputs.json")
}

/// The withArgs of the fixture row for (step, skill), with ${CARD} filled.
fn with_args(step: &str) -> Vec<String> {
    let tsv = std::fs::read_to_string(format!("{}/../chorus-make/tests/fixtures/athena-rows.tsv", dir())).unwrap();
    let row = tsv.lines().find(|l| l.split('\t').next() == Some(step)).unwrap_or_else(|| panic!("no {step} row"));
    let wa = row.split('\t').nth(10).unwrap_or("");
    wa.split_whitespace().map(|p| p.replace("${CARD}", "4515")).collect()
}

fn verb_argv(pairs: &[String]) -> Result<Vec<String>, String> {
    let schema = athena_schema();
    let args = args_from_pairs(pairs)?.as_object().cloned().unwrap_or_default();
    let args = coerce_args(&args, &schema);
    check_args(&args, &schema)?;
    Ok(argv("{args}", &args, "wren"))
}

#[test]
fn each_athena_step_hands_its_verb_the_list_in_order() {
    assert_eq!(verb_argv(&with_args("shape")).unwrap(), vec!["--card", "4515"]);
    assert_eq!(verb_argv(&with_args("forge")).unwrap(), vec!["generate", "--card", "4515"]);
    assert_eq!(verb_argv(&with_args("seed")).unwrap(), vec!["staging", "--card", "4515"]);
    assert_eq!(verb_argv(&with_args("land")).unwrap(), vec!["--card", "4515", "--landed", "--target", "canonical"]);
}

#[test]
fn a_single_args_pair_is_still_a_list_of_one() {
    // args_from_pairs gives a string for one pair; the shape says list
    assert_eq!(verb_argv(&with_args("validate")).unwrap(), vec!["--sweep"]);
}

#[test]
fn negative_proof_a_step_with_no_args_pair_is_refused_not_run_bare() {
    // a bare athena-validate is the store sweep and a bare athena-deploy a full
    // prod deploy: an empty list must never reach the verb
    assert_eq!(verb_argv(&[]).unwrap_err(), "args is required");
}
