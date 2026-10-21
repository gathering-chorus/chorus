// @test-type: unit
// @domain: pipelines
//! #4683 AC1 — cicd-land (pipelines/cicd-land.yaml, generated from the rows)
//! runs merge, sync, deploy-prod, borg-prove prod, borg-prove record, accept,
//! in that order, as plain command steps. prod continues on failure so record
//! runs on the red path too; record is the gate, so a red verdict leaves accept
//! unrun.
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

/// Accept runs only when record succeeded: accept depends on record, record
/// depends on prod, and record never continues on failure.
fn accept_is_gated(yaml: &str) -> Result<(), String> {
    let s = steps(yaml);
    let get = |n: &str| s.iter().find(|(name, _, _)| name == n).ok_or(format!("no step {n}"));
    let (_, accept_deps, _) = get("skill-werk-accept")?;
    if accept_deps != "[skill-borg-record]" { return Err(format!("accept depends on {accept_deps}")); }
    let (_, record_deps, record) = get("skill-borg-record")?;
    if record_deps != "[skill-borg-verify-in-prod]" { return Err(format!("record depends on {record_deps}")); }
    if record.contains("continue_on") { return Err("record continues on failure: a red verdict would reach accept".into()); }
    Ok(())
}

#[test]
fn the_land_runs_merge_sync_deploy_prove_record_accept_in_order() {
    let names: Vec<String> = steps(&land()).into_iter().map(|(n, _, _)| n).collect();
    assert_eq!(names, ["skill-werk-merge", "skill-werk-sync", "skill-werk-deploy-prod",
        "skill-borg-verify-in-prod", "skill-borg-record", "skill-werk-accept"]);
    let y = land();
    assert!(y.contains("command: \"borg-prove prod ${CARD} ${ROLE}\""), "{y}");
    assert!(y.contains("command: \"borg-prove record ${CARD} ${ROLE}\""), "{y}");
    assert!(!y.contains("dag.enqueue") && !y.contains("dag.run") && !y.contains("human.task"), "plain rows only: {y}");
}

#[test]
fn prod_continues_on_failure_so_record_runs_on_the_red_path() {
    let s = steps(&land());
    let (_, _, prod) = s.iter().find(|(n, _, _)| n == "skill-borg-verify-in-prod").unwrap();
    assert!(prod.contains("continue_on:\n      failure: true"), "{prod}");
    accept_is_gated(&land()).unwrap();
}

#[test]
fn the_committed_pipeline_is_the_generated_one() {
    assert_eq!(fixture("../../pipelines/cicd-land.yaml"), land());
}

#[test]
fn negative_proof_a_record_that_continues_on_failure_or_an_accept_off_record_is_refused() {
    let y = land();
    let leaky = y.replace("command: \"borg-prove record ${CARD} ${ROLE}\"", "command: \"borg-prove record ${CARD} ${ROLE}\"\n    continue_on:\n      failure: true");
    assert!(accept_is_gated(&leaky).is_err(), "a record that continues on failure must be caught");
    let skipped = y.replace("depends: [skill-borg-record]", "depends: [skill-werk-deploy-prod]");
    assert!(accept_is_gated(&skipped).is_err(), "an accept not behind record must be caught");
    let main = fixture("tests/fixtures/cicd-land-golden.yaml").replace("skill-borg-record", "gone");
    assert!(accept_is_gated(&main).is_err(), "a missing record step must be caught, never pass");
}
