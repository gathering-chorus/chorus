//! #4465 — chorus-make turns a pipeline's rows into the werk v2 workflow, the
//! same rows always give the same bytes, and a hand edit is caught.
use chorus_make::{check, drift, generate, parse_rows};

fn fixture(name: &str) -> String {
    // run time, not env!(): the werk-test 4030 guard refuses a compile-time path
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    std::fs::read_to_string(format!("{dir}/tests/{name}")).expect("fixture")
}

fn cicd() -> Vec<chorus_make::Row> {
    parse_rows(&fixture("fixtures/cicd-rows.tsv")).expect("fixture rows parse")
}

#[test]
fn the_cicd_rows_generate_the_golden_workflow() {
    let out = generate("cicd", &cicd()).expect("generates");
    assert_eq!(out, fixture("fixtures/cicd-golden.yaml"));
}

#[test]
fn the_same_rows_give_byte_identical_output_in_any_row_order() {
    let mut shuffled = cicd();
    shuffled.reverse();
    assert_eq!(generate("cicd", &cicd()).unwrap(), generate("cicd", &shuffled).unwrap());
}

#[test]
fn steps_run_in_order_and_the_go_waits_between_demo_and_land() {
    let out = generate("cicd", &cicd()).unwrap();
    let ids: Vec<&str> = out.lines().filter_map(|l| l.strip_prefix("  - id: ")).collect();
    assert_eq!(ids.first(), Some(&"commit_werk_commit"));
    assert_eq!(ids.last(), Some(&"land_werk_accept"));
    let go = ids.iter().position(|i| *i == "demo_go").expect("the go is a step");
    assert_eq!(ids[go - 1], "demo_demo");
    assert_eq!(ids[go + 1], "land_werk_merge");
    assert!(out.contains("  - id: demo_go\n    description: \"demo · Jeff's go\"\n    depends: [demo_demo]\n    action: human.task\n"));
    assert!(out.starts_with(chorus_make::HEADER));
}

#[test]
fn negative_proof_a_hand_edited_step_goes_red_in_the_drift_check() {
    let expected = generate("cicd", &cicd()).unwrap();
    assert!(drift(&expected, &expected).is_ok(), "an untouched file matches");
    let edited = expected.replace("werk-test ${CARD} ${ROLE}", "werk-test ${CARD} ${ROLE} --type=unit");
    assert_ne!(edited, expected, "the edit must change the file, or this proves nothing");
    let err = drift(&expected, &edited).expect_err("a hand edit must be caught");
    assert!(err.contains("--type=unit"), "the failure names what changed: {err}");
    let truncated: String = expected.lines().take(10).map(|l| format!("{l}\n")).collect();
    assert!(drift(&expected, &truncated).is_err(), "a cut-short file is drift too");
}

#[test]
fn negative_proof_a_deterministic_skill_with_no_binary_is_refused() {
    // The door does not run sh:sparql (athena-model lib.rs:4453), so this rule
    // lives here; without this test it would be a rule nothing enforces.
    let mut rows = cicd();
    rows[3].implemented_by.clear();
    let errs = generate("cicd", &rows).expect_err("refused, not written");
    assert!(errs.iter().any(|e| e.contains("skill-werk-test") && e.contains("needs implementedBy")), "{errs:?}");
    assert!(check(&cicd()).is_empty(), "the real rows pass, so the refusal is about the edit");
}

#[test]
fn negative_proof_two_skills_at_one_order_and_no_rows_are_refused() {
    let mut rows = cicd();
    rows[1].skill_order = 1; // push now collides with commit inside step commit
    assert!(check(&rows).iter().any(|e| e.contains("two skills hold order 1 in step commit")));
    assert!(generate("cicd", &[]).is_err(), "no rows is refused, never an empty workflow");
}

#[test]
fn every_step_id_is_one_dagu_accepts() {
    // dagu 2.18: ^[a-zA-Z][a-zA-Z0-9_]*$ — the first generated file used '-'
    // and dagu refused all 13 steps.
    let out = generate("cicd", &cicd()).unwrap();
    for id in out.lines().filter_map(|l| l.strip_prefix("  - id: ")) {
        let ok = id.starts_with(|c: char| c.is_ascii_alphabetic()) && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        assert!(ok, "dagu would refuse step id {id}");
    }
}

#[test]
fn a_bad_row_is_refused_by_line_never_skipped() {
    let errs = parse_rows("step\tstepOrder\tskillOrder\tskill\tlabel\texecutor\timplementedBy\tmode\ncommit\tone\t1\ts\tl\tdeterministic\tb\t\n")
        .expect_err("a non-number order is refused");
    assert_eq!(errs, vec!["line 2: stepOrder and skillOrder must be numbers".to_string()]);
}

// Fixtures by their repo path, so the pipeline's test step sees which files these tests read.
const REPO_FIXTURES: [&str; 4] = [
    "platform/services/chorus-make/tests/fixtures/cicd-golden.yaml",
    "platform/services/chorus-make/tests/fixtures/cicd-rows.tsv",
    "platform/services/chorus-make/tests/fixtures/cicd-graph.trig",
    "platform/services/chorus-make/tests/fixtures/mcp-server-names-20261009.txt",
];

#[test]
fn every_fixture_named_here_exists() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    for f in REPO_FIXTURES {
        let local = f.strip_prefix("platform/services/chorus-make/").unwrap();
        assert!(std::path::Path::new(&dir).join(local).is_file(), "missing fixture {f}");
    }
}
