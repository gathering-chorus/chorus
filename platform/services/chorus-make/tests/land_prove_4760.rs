// @test-type: unit
// @domain: pipelines
//! #4760 AC1 — cicd-land (pipelines/cicd-land.yaml, generated from the rows in
//! tests/fixtures/cicd-rows.tsv) proves itself: the land runs
//! `borg-prove record` (record hands itself to Borg Queen's account through
//! sudo, so the verdict is hers) and accept runs only behind it. No human step
//! is left in the land, so no land waits on a session to wake. These are the
//! rows the prod store already holds (5 borg-record, 6 accept, 7 evaluate).
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

/// Accept runs only on her recorded verdict: accept behind record, record
/// behind prod; record never continues on failure (it exits red on any verdict
/// but Proven or unproven), and no step of the land waits on a person.
fn accept_is_gated(yaml: &str) -> Result<(), String> {
    let s = steps(yaml);
    let get = |n: &str| s.iter().find(|(name, _, _)| name == n).ok_or(format!("no step {n}"));
    if yaml.contains("action: human.task") { return Err("a step of the land waits on a person".into()); }
    let (_, accept_deps, _) = get("skill-werk-accept")?;
    if accept_deps != "[skill-borg-record]" { return Err(format!("accept depends on {accept_deps}")); }
    let (_, record_deps, record) = get("skill-borg-record")?;
    if record_deps != "[skill-borg-verify-in-prod]" { return Err(format!("record depends on {record_deps}")); }
    if record.contains("continue_on") { return Err("record continues on failure: a red verdict would reach accept".into()); }
    if !record.contains("command: \"borg-prove record ${CARD} ${ROLE}\"") { return Err("the land does not run record".into()); }
    Ok(())
}

#[test]
fn the_land_records_her_proof_itself_and_accept_waits_on_it() {
    let y = land();
    let names: Vec<String> = steps(&y).into_iter().map(|(n, _, _)| n).collect();
    assert_eq!(names, ["skill-werk-merge", "skill-werk-sync", "skill-werk-deploy-prod",
        "skill-borg-verify-in-prod", "skill-borg-record", "skill-werk-accept", "skill-clearing-evaluate"]);
    accept_is_gated(&y).unwrap();
}

#[test]
fn prod_continues_on_failure_so_record_runs_on_the_red_path() {
    let s = steps(&land());
    let (_, _, prod) = s.iter().find(|(n, _, _)| n == "skill-borg-verify-in-prod").unwrap();
    assert!(prod.contains("continue_on:\n      failure: true"), "{prod}");
}

// NEGATIVE PROOF: each way the gate can leak is caught
#[test]
fn negative_proof_a_leaky_skipped_or_waiting_land_is_refused() {
    let y = land();
    let record = "command: \"borg-prove record ${CARD} ${ROLE}\"";
    let leaky = y.replace(record, &format!("{record}\n    continue_on:\n      failure: true"));
    assert!(accept_is_gated(&leaky).is_err(), "a record that continues on failure must be caught");
    let skipped = y.replace("depends: [skill-borg-record]", "depends: [skill-werk-deploy-prod]");
    assert!(accept_is_gated(&skipped).is_err(), "an accept not behind record must be caught");
    let human = y.replace(record, "action: human.task");
    assert!(accept_is_gated(&human).is_err(), "a land that waits on her session must be caught");
}
