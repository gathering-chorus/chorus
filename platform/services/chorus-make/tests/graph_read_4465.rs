//! #4465 — chorus-make's graph read, run with Jena's `sparql` against a fixture
//! graph shaped like the store (steps in the pipelines graph, skills in the
//! skills graph). Until the Skill shape lands, this is the only place the real
//! query runs. Wren: fail loudly on zero rows.
use chorus_make::{generate, parse_rows, rows_from_sparql_tsv, rows_query};
use std::process::Command;

fn dir() -> String {
    std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR")
}

/// The query's TSV result over the fixture graph. No `sparql` on the box is a
/// failure that says so, never a pass.
fn run(pipeline: &str) -> String {
    let q = format!("{}/target-q-{}-{}.rq", std::env::temp_dir().display(), pipeline, std::process::id());
    std::fs::write(&q, rows_query(pipeline)).unwrap();
    let out = Command::new("sparql")
        .args(["--data", &format!("{}/tests/fixtures/cicd-graph.trig", dir()), "--results", "TSV", "--query", &q])
        .output()
        .expect("Jena `sparql` must be on PATH for this test (brew install jena) — not skipped");
    let _ = std::fs::remove_file(&q);
    assert!(out.status.success(), "sparql failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn the_graph_read_gives_the_same_workflow_as_the_rows_file() {
    let from_graph = rows_from_sparql_tsv(&run("cicd")).expect("rows");
    assert_eq!(from_graph.len(), 13);
    let from_file = parse_rows(&std::fs::read_to_string(format!("{}/tests/fixtures/cicd-rows.tsv", dir())).unwrap()).unwrap();
    assert_eq!(generate("cicd", &from_graph).unwrap(), generate("cicd", &from_file).unwrap());
}

#[test]
fn negative_proof_a_pipeline_with_no_skill_rows_fails_loudly() {
    // athena has a step in the fixture but no StepSkill rows: the same state as
    // the store before the shape lands. It must refuse, not write an empty file.
    let errs = rows_from_sparql_tsv(&run("athena")).expect_err("zero rows must be refused");
    assert!(errs[0].contains("zero rows"), "{errs:?}");
    let errs = rows_from_sparql_tsv(&run("no-such-pipeline")).expect_err("an unknown pipeline is zero rows too");
    assert!(errs[0].contains("zero rows"));
}

#[test]
fn negative_proof_a_result_that_is_not_ours_is_refused() {
    assert!(rows_from_sparql_tsv("<html>404</html>\n").is_err(), "an error page is not rows");
    assert!(rows_from_sparql_tsv("").is_err());
}

#[test]
fn a_missing_optional_binary_reaches_the_refusal() {
    // the go has no implementedBy and no mode: the trailing columns are empty,
    // and a deterministic skill with the same gap is still refused downstream.
    let tsv = "?step\t?stepOrder\t?skillOrder\t?skill\t?label\t?executor\t?implementedBy\t?mode\n\"test\"\t3\t1\t<https://jeffbridwell.com/chorus#skill-werk-test>\t\"werk-test\"\t\"deterministic\"\t\t\n";
    let rows = rows_from_sparql_tsv(tsv).expect("parses");
    assert!(generate("cicd", &rows).unwrap_err()[0].contains("needs implementedBy"));
}
