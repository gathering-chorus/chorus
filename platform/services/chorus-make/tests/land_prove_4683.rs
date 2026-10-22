// @test-type: unit
// @domain: pipelines
//! #4683 AC1 — cicd-land (pipelines/cicd-land.yaml, generated from the rows)
//! runs merge, sync, deploy-prod, borg-prove prod, then the proof, then accept.
//! prod continues on failure so the proof runs on the red path too.
//! #4691 AC4 — the proof is Borg Queen's, the way a pair's green is its
//! navigator's: ask nudges her, the land waits at her step (a human task she
//! completes after running record in her own session), and check is the gate:
//! accept runs only on her signed, green verdict.
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

/// Accept runs only when her check passed: accept depends on check, check on
/// her step, her step on ask, ask on prod; check never continues on failure,
/// and it is handed who signed her step.
fn accept_is_gated(yaml: &str) -> Result<(), String> {
    let s = steps(yaml);
    let get = |n: &str| s.iter().find(|(name, _, _)| name == n).ok_or(format!("no step {n}"));
    let (_, accept_deps, _) = get("skill-werk-accept")?;
    if accept_deps != "[skill-borg-check]" { return Err(format!("accept depends on {accept_deps}")); }
    let (_, check_deps, check) = get("skill-borg-check")?;
    if check_deps != "[skill-borg-queen-proves]" { return Err(format!("check depends on {check_deps}")); }
    if check.contains("continue_on") { return Err("check continues on failure: a red or unsigned verdict would reach accept".into()); }
    if !check.contains("${skill_borg_queen_proves.outputs.by}") { return Err("check is not handed who signed her step".into()); }
    let (_, proves_deps, proves) = get("skill-borg-queen-proves")?;
    if proves_deps != "[skill-borg-ask]" { return Err(format!("her step depends on {proves_deps}")); }
    if !proves.contains("action: human.task") { return Err("her step is not a human task".into()); }
    let (_, ask_deps, _) = get("skill-borg-ask")?;
    if ask_deps != "[skill-borg-verify-in-prod]" { return Err(format!("ask depends on {ask_deps}")); }
    Ok(())
}

#[test]
fn the_land_runs_merge_sync_deploy_prove_ask_her_step_check_accept_in_order() {
    let names: Vec<String> = steps(&land()).into_iter().map(|(n, _, _)| n).collect();
    assert_eq!(names, ["skill-werk-merge", "skill-werk-sync", "skill-werk-deploy-prod",
        "skill-borg-verify-in-prod", "skill-borg-ask", "skill-borg-queen-proves", "skill-borg-check", "skill-werk-accept", "skill-clearing-evaluate"]);
    let y = land();
    assert!(y.contains("command: \"borg-prove prod ${CARD} ${ROLE}\""), "{y}");
    assert!(y.contains("command: \"borg-prove ask ${CARD} ${ROLE}\""), "{y}");
    assert!(y.contains("id: skill_borg_queen_proves"), "{y}");
    assert!(y.contains("prompt: \"Borg Queen proves it in prod for #${CARD}? (borg-queen answers)\""), "{y}");
    assert!(y.contains("enum: [recorded, stop]"), "{y}");
    assert!(y.contains("command: \"borg-prove check ${CARD} ${skill_borg_queen_proves.outputs.by}\""), "{y}");
    assert!(!y.contains("borg-prove record"), "record runs in her session, not in the land: {y}");
    assert!(!y.contains("dag.enqueue") && !y.contains("dag.run"), "{y}");
}

/// dagu refuses a human task in a run it started as a sub-DAG (#4577, measured:
/// "human task steps are not allowed in sub-DAGs"), so cicd hands the land off
/// with the CLI, a top-level run, the way the pair hands off to cicd.
#[test]
fn cicd_hands_off_the_land_as_a_top_level_run() {
    let y = chorus_make::generate("cicd", &parse_rows(&fixture("tests/fixtures/cicd-rows.tsv")).unwrap()).unwrap();
    let s = steps(&y);
    let (_, deps, hand) = s.iter().find(|(n, _, _)| n == "cicd-land").expect("a cicd-land step");
    assert_eq!(deps, "[skill-werk-go]");
    assert!(hand.contains("enqueue cicd-land -- CARD=${CARD} ROLE=${ROLE} ACCEPTER=${skill_werk_go.outputs.accepter}"), "{hand}");
    assert!(hand.contains("expected: \"go\""), "the land runs on a go only: {hand}");
    assert!(!hand.contains("dag.enqueue"), "a dag.enqueue land is a sub-DAG, and her step would be refused: {hand}");
    assert_eq!(fixture("../../pipelines/cicd.yaml"), y, "the committed cicd is the generated one");
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
fn negative_proof_a_check_that_continues_or_is_unsigned_or_an_accept_off_check_is_refused() {
    let y = land();
    let check = "command: \"borg-prove check ${CARD} ${skill_borg_queen_proves.outputs.by}\"";
    let leaky = y.replace(check, &format!("{check}\n    continue_on:\n      failure: true"));
    assert!(accept_is_gated(&leaky).is_err(), "a check that continues on failure must be caught");
    let skipped = y.replace("depends: [skill-borg-check]", "depends: [skill-werk-deploy-prod]");
    assert!(accept_is_gated(&skipped).is_err(), "an accept not behind check must be caught");
    let unsigned = y.replace("${skill_borg_queen_proves.outputs.by}", "borg-queen");
    assert!(accept_is_gated(&unsigned).is_err(), "a check that assumes the signer must be caught");
    let main = fixture("tests/fixtures/cicd-land-golden.yaml").replace("skill-borg-check", "gone");
    assert!(accept_is_gated(&main).is_err(), "a missing check step must be caught, never pass");
}

/// NEGATIVE (#4691): a human step in the land that names no answerer, or one
/// answer only, is refused at generation, never rendered as a step anyone may sign.
#[test]
fn negative_proof_a_land_human_step_with_no_answerer_is_refused() {
    let rows = fixture("tests/fixtures/cicd-rows.tsv");
    for bad in ["", "by=borg-queen choices=recorded"] {
        let r = parse_rows(&rows.replace("by=borg-queen choices=recorded|stop", bad)).unwrap();
        let e = generate_land("cicd", &r).expect_err(bad);
        assert!(e.iter().any(|m| m.contains("skill-borg-queen-proves") && m.contains("no by= and choices=")), "{e:?}");
    }
}
