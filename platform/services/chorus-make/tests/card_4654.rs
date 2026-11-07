// @test-type: unit
// @domain: cicd
//! #4654 — a card's pull and its coding as a dagu pipeline: werk-pull, then a
//! human step the card's own role completes ("done"), its check, then the
//! hand-off that starts athena or cicd.
use chorus_make::{generate_straight, parse_pipe, parse_rows, Pipe, Row};

fn fixture(name: &str) -> String {
    // run time, not env!(): the werk-test 4030 guard refuses a compile-time path
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    std::fs::read_to_string(format!("{dir}/tests/fixtures/{name}")).expect("fixture")
}

fn rows() -> Vec<Row> {
    parse_rows(&fixture("card-rows.tsv")).expect("rows parse")
}

fn pipe() -> Pipe {
    parse_pipe(&fixture("card-pipe.tsv")).unwrap()
}

fn known() -> Vec<String> {
    ["cicd", "cicd-test", "nightly", "athena", "pair", "demo", "card"].iter().map(|s| s.to_string()).collect()
}

#[test]
fn the_card_rows_generate_the_golden() {
    assert_eq!(generate_straight("card", &pipe(), &rows(), &known()).unwrap(), fixture("card-golden.yaml"));
}

#[test]
fn the_run_pulls_first_then_waits_on_the_cards_role() {
    let y = generate_straight("card", &pipe(), &rows(), &known()).unwrap();
    let pull = y.find("werk-card pull ${CARD} ${ROLE}").expect("a pull step");
    let code = y.find("    id: skill_werk_code\n    action: human.task\n").expect("a human code step");
    assert!(pull < code, "the pull runs before the coding wait\n{y}");
    assert!(y.contains("(${ROLE} answers)"), "{y}");
    assert!(y.contains("            enum: [done, stop]\n        required: [by, decision]\n"), "{y}");
    assert!(y.contains("werk-card check ${CARD} ${ROLE} ${skill_werk_code.outputs.by} ${skill_werk_code.outputs.decision}"), "{y}");
    assert!(y.contains("    depends: [skill-werk-card-check]\n    command: \"${CHORUS_HOME}/platform/scripts/werk-card handoff ${CARD} ${ROLE}\""), "{y}");
    assert!(y.contains("queue: card\n"), "{y}");
}

#[test]
fn negative_proof_a_code_step_answered_by_a_param_the_pipeline_lacks_is_refused() {
    let mut r = rows();
    r.iter_mut().find(|x| x.executor == "human").unwrap().with_args = "by=NAVIGATOR choices=done|stop".into();
    let errs = generate_straight("card", &pipe(), &r, &known()).unwrap_err();
    assert!(errs.iter().any(|e| e.contains("answered by NAVIGATOR")), "{errs:?}");
}

#[test]
fn negative_proof_a_hand_edited_card_yaml_is_not_the_golden() {
    let y = generate_straight("card", &pipe(), &rows(), &known()).unwrap();
    let edited = y.replace("(${ROLE} answers)", "(jeff answers)");
    assert_ne!(edited, y);
    assert_ne!(edited, fixture("card-golden.yaml"));
}
