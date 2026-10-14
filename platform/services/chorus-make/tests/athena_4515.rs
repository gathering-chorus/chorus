// @test-type: unit
// @domain: pipelines
//! #4515 — chorus-athena runs a v2 dagu pipeline for one model card, generated
//! from the graph like cicd: Model → Make → Deploy (staging) → Validate → Jeff's
//! go → Land (prod). What changed in chorus-make to say it, each with its
//! negative proof.
use chorus_make::{check, dagu_name, generate, generate_land, parse_rows, MODEL_LAND_QUEUE};

fn fixture(name: &str) -> String {
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    std::fs::read_to_string(format!("{dir}/tests/fixtures/{name}")).expect("fixture")
}

fn athena() -> Vec<chorus_make::Row> {
    parse_rows(&fixture("athena-rows.tsv")).expect("athena rows parse")
}

fn cicd() -> Vec<chorus_make::Row> {
    parse_rows(&fixture("cicd-rows.tsv")).expect("cicd rows parse")
}

#[test]
fn the_steps_run_model_make_deploy_validate_then_the_go_then_the_land() {
    let card = generate("athena", &athena()).expect("generates");
    let land = generate_land("athena", &athena()).expect("generates the land");
    let order = [
        "call athena-validate args=--card args=${CARD}\"",
        "call athena-gen args=generate args=--card args=${CARD}\"",
        "call athena-deploy args=staging args=--card args=${CARD}\"",
        "call athena-validate args=--sweep\"",
        "action: human.task",
        "action: dag.enqueue",
    ];
    let at: Vec<usize> = order.iter().map(|o| card.find(o).unwrap_or_else(|| panic!("{o} missing:\n{card}"))).collect();
    assert!(at.windows(2).all(|w| w[0] < w[1]), "out of order: {at:?}\n{card}");
    // the prod deploy is only in the land half, and says canonical on purpose (#4080)
    assert!(!card.contains("--landed"), "{card}");
    assert!(land.contains("call athena-deploy args=--card args=${CARD} args=--landed args=--target args=canonical\""), "{land}");
}

#[test]
fn a_skill_two_steps_call_is_named_by_skill_and_step_and_a_unique_one_is_not() {
    let rows = athena();
    let names: Vec<String> = rows.iter().map(|r| dagu_name(&rows, r)).collect();
    assert_eq!(names, vec![
        "skill-athena-validate-shape", "skill-athena-gen", "skill-athena-deploy-seed",
        "skill-athena-validate-validate", "skill-werk-go", "skill-athena-deploy-land",
    ]);
    assert!(check(&rows).is_empty(), "{:?}", check(&rows));
    // cicd calls no skill twice: every name stays the Skill row's own (its golden holds)
    let c = cicd();
    assert!(c.iter().all(|r| dagu_name(&c, r) == chorus_make::step_name(r)));
}

#[test]
fn negative_proof_one_skill_twice_in_one_step_still_collides() {
    let mut rows = athena();
    rows[3].step = "shape".into(); // validate now runs inside shape, at its own order
    rows[3].skill_order = 2;
    rows[3].step_order = 1;
    assert!(check(&rows).iter().any(|e| e.contains("skill-athena-validate: called twice in one step")), "{:?}", check(&rows));
}

#[test]
fn the_model_land_runs_on_its_own_queue_and_never_enqueues_itself() {
    let land = generate_land("athena", &athena()).unwrap();
    assert!(land.contains(&format!("\nqueue: {MODEL_LAND_QUEUE}\n")), "{land}");
    // a werk land's success handler waits on this run: on werk-land it would
    // queue behind the werk land holding werk-land
    assert!(!land.contains("queue: werk-land"), "{land}");
    // it IS the model deploy: an athena-land handler would enqueue it again
    assert!(!land.contains("athena-land ${CARD}") && !land.contains("  success:"), "{land}");
    let card = generate("athena", &athena()).unwrap();
    assert!(card.contains("enqueue the land on the athena-land queue"), "{card}");
}

#[test]
fn negative_proof_every_other_land_keeps_werk_land_and_the_model_hook() {
    let land = generate_land("cicd", &cicd()).unwrap();
    assert!(land.contains("\nqueue: werk-land\n"), "{land}");
    assert!(land.contains("platform/scripts/athena-land ${CARD} ${ROLE}"), "{land}");
}

#[test]
fn an_unmeasured_validate_before_the_go_is_a_failure_never_held() {
    // athena-validate exits 2 when the sweep could not run; werk-demo exits 2
    // for "presented, held for the go". Only the second is the step's success.
    let card = generate("athena", &athena()).unwrap();
    assert!(!card.contains("continue_on"), "{card}");
    // NEGATIVE PROOF: werk-demo before cicd's go keeps its held exit
    assert!(generate("cicd", &cicd()).unwrap().contains("continue_on:\n      exit_code: [2]"));
}

#[test]
fn negative_proof_an_athena_step_without_its_args_list_is_refused() {
    let mut rows = athena();
    rows[0].with_args.clear();
    let errs = check(&rows);
    assert!(errs.iter().any(|e| e.contains("step shape calls athena-validate without args")), "{errs:?}");
}
