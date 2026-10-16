// @test-type: unit
// @domain: pipelines
//! #4515 — chorus-athena runs v2 dagu pipelines generated from the graph, GO-LESS
//! (Jeff 2026-10-14: one go per card, in werk). `athena` = Model → Make → Deploy
//! (staging) → Validate, and on success it initiates the card's werk run
//! (werk-v2). `athena-land` = the prod Land, its own one-step pipeline that only a
//! werk land starts. What changed in chorus-make to say it, each with its
//! negative proof.
use chorus_make::{
    check, dagu_name, generate, generate_land, generate_named, parse_rows, pipeline_of, walk_check, walk_from_sparql_tsv,
    MODEL_LAND_QUEUE,
};

fn fixture(name: &str) -> String {
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    std::fs::read_to_string(format!("{dir}/tests/fixtures/{name}")).expect("fixture")
}

fn rows(name: &str) -> Vec<chorus_make::Row> {
    parse_rows(&fixture(name)).expect("rows parse")
}

fn athena() -> String {
    generate("athena", &rows("athena-rows.tsv")).expect("generates athena")
}

fn athena_land() -> String {
    generate_named("athena-land", &rows("athena-land-rows.tsv")).expect("generates athena-land")
}

/// The handler_on block of a workflow, split at `success:`.
fn handlers(y: &str) -> (String, String) {
    let h = &y[y.find("handler_on:\n").expect("handlers")..y.find("steps:\n").unwrap()];
    match h.find("  success:\n") {
        Some(i) => (h[..i].to_string(), h[i..].to_string()),
        None => (h.to_string(), String::new()),
    }
}

#[test]
fn athena_runs_model_make_deploy_validate_with_no_go_and_no_land() {
    let y = athena();
    let order = [
        "call athena-validate args=--card args=${CARD}\"",
        "call athena-gen args=generate args=--card args=${CARD}\"",
        "call athena-deploy args=staging args=--card args=${CARD}\"",
        "call athena-validate args=--staging\"",
    ];
    let at: Vec<usize> = order.iter().map(|o| y.find(o).unwrap_or_else(|| panic!("{o} missing:\n{y}"))).collect();
    assert!(at.windows(2).all(|w| w[0] < w[1]), "out of order: {at:?}\n{y}");
    for gone in ["human.task", "dag.enqueue", "--landed", "continue_on", "ACCEPTER"] {
        assert!(!y.contains(gone), "{gone} in a go-less pipeline:\n{y}");
    }
    assert!(y.contains("\nqueue: werk\n"), "{y}");
}

#[test]
fn athena_initiates_the_cards_werk_run_on_success_only() {
    // Jeff: "if chorus-athena requires chorus-werk, it initiates chorus-werk"
    let (failure, success) = handlers(&athena());
    assert!(success.contains("command: '${CHORUS_HOME}/platform/scripts/werk-v2 ${CARD} ${ROLE} || "), "{success}");
    // NEGATIVE PROOF: a failed athena run enqueues nothing; it only nudges
    assert!(!failure.contains("werk-v2") && !failure.contains("enqueue"), "{failure}");
    assert!(failure.contains("platform/scripts/ops-nudge ${ROLE}"), "{failure}");
    // and no other pipeline starts a werk run when it succeeds
    let land = generate_land("cicd", &rows("cicd-rows.tsv")).unwrap();
    assert!(!land.contains("werk-v2"), "{land}");
    assert!(!athena_land().contains("werk-v2"));
}

#[test]
fn athena_land_is_one_prod_step_on_its_own_queue_that_never_enqueues_itself() {
    let y = athena_land();
    assert!(y.contains("call athena-deploy args=--card args=${CARD} args=--landed args=--target args=canonical\""), "{y}");
    assert!(y.contains(&format!("\nqueue: {MODEL_LAND_QUEUE}\n")) && !y.contains("queue: werk-land"), "{y}");
    assert!(y.contains("params:\n  - CARD: \"\"\n  - ROLE: \"\"\nenv:"), "CARD and ROLE only: {y}");
    let (failure, success) = handlers(&y);
    assert!(success.is_empty(), "the model land has no success handler: {success}");
    assert!(failure.contains("land failed"), "{failure}");
    assert_eq!(y.matches("  - name: ").count(), 1, "{y}");
}

#[test]
fn athena_land_is_a_pipeline_of_its_own_not_athenas_land_half() {
    assert_eq!(pipeline_of("athena-land"), "athena-land");
    assert_eq!(pipeline_of("cicd-land"), "cicd", "every other -land is still a land half");
    // NEGATIVE PROOF: athena has no land half to generate
    assert!(generate_land("athena", &rows("athena-rows.tsv")).unwrap_err()[0].contains("go-less"));
}

#[test]
fn negative_proof_a_go_row_in_a_go_less_pipeline_is_refused() {
    let mut r = rows("athena-rows.tsv");
    r.extend(parse_rows("go\t5\t1\tskill-werk-go\tJeff's go\thuman\t\t\t\n").unwrap());
    let e = generate("athena", &r).unwrap_err();
    assert!(e[0].contains("pipeline athena has no go") && e[0].contains("skill-werk-go"), "{e:?}");
}

#[test]
fn negative_proof_any_other_pipeline_without_its_go_is_still_refused() {
    // a cicd whose go row went missing must never become a land that runs without Jeff
    let r: Vec<_> = rows("cicd-rows.tsv").into_iter().filter(|r| r.executor != "human").collect();
    assert!(generate("cicd", &r).unwrap_err()[0].contains("no human go"));
    assert!(generate_land("cicd", &r).is_err());
}

#[test]
fn negative_proof_every_other_land_keeps_werk_land_and_the_model_hook() {
    let land = generate_land("cicd", &rows("cicd-rows.tsv")).unwrap();
    assert!(land.contains("\nqueue: werk-land\n"), "{land}");
    assert!(land.contains("platform/scripts/athena-land ${CARD} ${ROLE}"), "{land}");
}

#[test]
fn a_skill_two_steps_call_is_named_by_skill_and_step_and_a_unique_one_is_not() {
    let r = rows("athena-rows.tsv");
    let names: Vec<String> = r.iter().map(|x| dagu_name(&r, x)).collect();
    // #4562 — the fixture now holds #4536's baseline steps: two deploys and three validates, each named by its step
    assert_eq!(names, vec!["skill-athena-validate-shape", "skill-athena-gen", "skill-athena-deploy-athena-baseline-copy", "skill-athena-validate-athena-baseline", "skill-athena-deploy-seed", "skill-athena-validate-validate"]);
    assert!(check(&r).is_empty(), "{:?}", check(&r));
    let c = rows("cicd-rows.tsv");
    assert!(c.iter().all(|x| dagu_name(&c, x) == chorus_make::step_name(x)), "cicd names unchanged");
}

#[test]
fn negative_proof_one_skill_twice_in_one_step_still_collides() {
    let mut r = rows("athena-rows.tsv");
    r[3].step = "shape".into();
    r[3].step_order = 1;
    r[3].skill_order = 2;
    assert!(check(&r).iter().any(|e| e.contains("skill-athena-validate: called twice in one step")), "{:?}", check(&r));
}

#[test]
fn negative_proof_an_athena_step_without_its_args_list_is_refused() {
    let mut r = rows("athena-rows.tsv");
    r[0].with_args.clear();
    assert!(check(&r).iter().any(|e| e.contains("step shape calls athena-validate without args")), "{:?}", check(&r));
}

/// The walk of value-stream-athena once the to-be rows land: both pipelines
/// automate it, every pipeline step covers a stream step, every skill a domain.
const WALK: &str = "?pipeline\t?vsStep\t?pStep\t?o\t?skill\t?domain
\"athena\"\t\"athena-model\"\t\"shape\"\t\"1\"\t\"athena-validate\"\t\"domains\"
\"athena\"\t\"athena-make\"\t\"forge\"\t\"1\"\t\"athena-gen\"\t\"code\"
\"athena\"\t\"athena-deploy\"\t\"seed\"\t\"1\"\t\"athena-deploy\"\t\"deploys\"
\"athena\"\t\"athena-validate\"\t\"validate\"\t\"1\"\t\"athena-validate\"\t\"domains\"
\"athena-land\"\t\"athena-deploy\"\t\"land\"\t\"1\"\t\"athena-deploy\"\t\"deploys\"
";

#[test]
fn the_walk_of_the_athena_stream_flags_nothing() {
    let w = walk_from_sparql_tsv(WALK).unwrap();
    assert!(w.uncovered.is_empty(), "{:?}", w.uncovered);
    assert_eq!(walk_check(&w), Ok(()));
    // NEGATIVE PROOF: the same walk with the land step covering no stream step is flagged
    let broken = WALK.replace("\"athena-land\"\t\"athena-deploy\"\t\"land\"\t\"1\"\t\"athena-deploy\"\t\"deploys\"", "\"athena-land\"\t\"\"\t\"land\"\t\"\"\t\"\"\t\"\"");
    let e = walk_check(&walk_from_sparql_tsv(&broken).unwrap()).unwrap_err();
    assert!(e.iter().any(|x| x.contains("pipeline step `land` covers no value-stream step")), "{e:?}");
}
