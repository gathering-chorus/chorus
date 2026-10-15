// @test-type: unit — signal:api is fixture-data
// @domain: athena
//! #4531 — the source-deleted refusal names a way to stage a retirement that
//! exists. On 10-14 it named `athena-model retire-subject`, which athena-model
//! does not have, so the person reading it had nothing to run.
use athena_deploy::{parse_retirement, stage_retirement_hint};

fn repo() -> std::path::PathBuf {
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    std::path::Path::new(&dir).join("../../..").canonicalize().unwrap()
}

#[test]
fn the_hint_names_the_retirements_file_and_a_line_that_parses_as_staged() {
    let hint = stage_retirement_hint("logdomain-x");
    assert!(hint.contains("designing/schemas/model-retirements.jsonl"), "{hint}");
    assert!(repo().join("designing/schemas/model-retirements.jsonl").is_file(), "the file the hint names exists");
    let line = hint.lines().find(|l| l.trim_start().starts_with('{')).expect("an example line").trim();
    let r = parse_retirement(line).expect("parses").expect("is a retirement");
    assert_eq!(r.status, "staged");
    assert_eq!(r.retire_subject, "https://jeffbridwell.com/chorus#logdomain-x");
}

/// NEGATIVE PROOF: the old advice named a command athena-model does not have.
#[test]
fn negative_proof_the_hint_names_no_command_that_does_not_exist() {
    let hint = stage_retirement_hint("logdomain-x");
    assert!(!hint.contains("retire-subject <iri>"), "{hint}");
    let old = "athena-model retire-subject <iri> --card <id>";
    assert!(old.contains("retire-subject <iri>"), "the check can see the old text");
}
