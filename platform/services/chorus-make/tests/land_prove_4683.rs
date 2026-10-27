// @test-type: unit
// @domain: pipelines
//! #4683 AC1 — cicd-land (pipelines/cicd-land.yaml, generated from the rows)
//! runs merge, sync, deploy-prod, borg-prove prod, then the proof, then accept.
//! prod continues on failure so the proof runs on the red path too.
//! The proof itself (#4691, then #4760: the land runs record as her account,
//! no human step) is tested in land_prove_4760.rs.
use chorus_make::{generate_land, parse_rows};

fn fixture(name: &str) -> String {
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    std::fs::read_to_string(format!("{dir}/{name}")).expect("fixture")
}

fn land() -> String {
    generate_land("cicd", &parse_rows(&fixture("tests/fixtures/cicd-rows.tsv")).unwrap()).unwrap()
}

/// Each step as (name, depends, its own lines).
fn steps(yaml: &str) -> Vec<(String, String, String)> {
    yaml.split("\n  - name: ").skip(1).map(|b| {
        let name = b.lines().next().unwrap().trim().to_string();
        let deps = b.lines().find_map(|l| l.trim().strip_prefix("depends: ")).unwrap_or("").to_string();
        (name, deps, b.to_string())
    }).collect()
}

/// dagu refuses a human task in a run it started as a sub-DAG (#4577, measured:
/// "human task steps are not allowed in sub-DAGs"), so cicd hands the land off
/// with the CLI, a top-level run, the way the pair hands off to cicd.
#[test]
fn cicd_hands_off_the_land_as_a_top_level_run() {
    let rows = parse_rows(&fixture("tests/fixtures/cicd-rows.tsv")).unwrap();
    let y = chorus_make::generate("cicd", &rows).unwrap();
    // #4758 — the go half hands the land off; the card half hands off the go
    let g = chorus_make::generate_go("cicd", &rows).unwrap();
    let s = steps(&g);
    let (_, deps, hand) = s.iter().find(|(n, _, _)| n == "cicd-land").expect("a cicd-land step");
    assert_eq!(deps, "[skill-werk-go]");
    assert!(hand.contains("enqueue cicd-land -- CARD=${CARD} ROLE=${ROLE} ACCEPTER=${skill_werk_go.outputs.accepter}"), "{hand}");
    assert!(hand.contains("expected: \"go\""), "the land runs on a go only: {hand}");
    assert!(!hand.contains("dag.enqueue"), "a dag.enqueue land is a sub-DAG, and her step would be refused: {hand}");
    assert_eq!(fixture("../../pipelines/cicd.yaml"), y, "the committed cicd is the generated one");
    assert_eq!(fixture("../../pipelines/cicd-go.yaml"), g, "the committed cicd-go is the generated one");
}

#[test]
fn the_committed_pipeline_is_the_generated_one() {
    assert_eq!(fixture("../../pipelines/cicd-land.yaml"), land());
}
