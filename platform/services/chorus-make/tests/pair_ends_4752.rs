// @test-type: unit
// @domain: pipelines
//! #4752 — a pair ends when its card is proven in prod, not at the hand-off
//! (Jeff 10-30 14:56). The generated pipelines carry it: the pair hands off and
//! stays registered; a cicd or land red goes back to the card's pair first
//! (`pair routed`, else the builder's nudge); the land's success ends the pair
//! after borg-prove; the pair's own local proof is gone and cicd's test step,
//! the same cicd-test SCOPE=diff, is the proof (Silas 19:02: the gate moves).
use chorus_make::{generate, generate_land, generate_straight, parse_pipe, parse_rows, Row};

fn fixture(name: &str) -> String {
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    std::fs::read_to_string(format!("{dir}/tests/fixtures/{name}")).expect("fixture")
}

fn cicd_rows() -> Vec<Row> {
    parse_rows(&fixture("cicd-rows.tsv")).unwrap()
}

fn pair() -> String {
    let known: Vec<String> = ["cicd", "cicd-test", "nightly", "athena", "pair"].iter().map(|s| s.to_string()).collect();
    generate_straight("pair", &parse_pipe(&fixture("pair-pipe.tsv")).unwrap(), &parse_rows(&fixture("pair-rows.tsv")).unwrap(), &known).unwrap()
}

fn failure_handler(yaml: &str) -> String {
    yaml.split("  failure:\n    command: ").nth(1).and_then(|r| r.lines().next()).unwrap_or("").to_string()
}

const ROUTED: &str = "${CHORUS_HOME}/platform/scripts/pair routed ${CARD} ${DAG_NAME} ${DAG_RUN_ID} || ";

#[test]
fn ac2_a_cicd_red_goes_to_the_cards_pair_first_then_the_exit_wake() {
    let y = generate("cicd", &cicd_rows()).unwrap();
    let h = failure_handler(&y);
    assert!(h.starts_with(&format!("'{ROUTED}")), "{h}");
    // #4888 — unpaired, nothing routes and the run's exit wake reaches the builder
    assert!(!h.contains("ops-nudge"), "{h}");
    assert!(y.contains("platform/scripts/run-finished ${CARD} ${ROLE}"), "{y}");
}

#[test]
fn ac2_a_land_red_goes_to_the_cards_pair_first() {
    let h = failure_handler(&generate_land("cicd", &cicd_rows()).unwrap());
    assert!(h.starts_with(&format!("'{ROUTED}")), "{h}");
}

#[test]
fn ac1_the_land_proven_in_prod_ends_the_pair() {
    let y = generate_land("cicd", &cicd_rows()).unwrap();
    let ok = y.split("  success:\n    command: ").nth(1).and_then(|r| r.lines().next()).unwrap_or("");
    assert!(ok.starts_with("'${CHORUS_HOME}/platform/scripts/pair proven ${CARD}; "), "{ok}");
    // success fires only after every land step, borg-prove record among them
    assert!(y.contains("borg-prove record ${CARD} ${ROLE}"), "{y}");
}

#[test]
fn ac1_the_pair_hands_off_and_does_not_end_at_the_hand_off() {
    let y = pair();
    assert!(y.contains("pair handoff ${CARD} ${ROLE} ${NAVIGATOR}"), "{y}");
    assert!(!y.contains("pair end ${CARD}"), "the pair must not end at the hand-off: {y}");
}

#[test]
fn ac3_the_pair_runs_no_local_proof_and_cicd_still_runs_cicd_test_scope_diff() {
    let y = pair();
    assert!(!y.contains("cicd-test"), "the pair's duplicate local proof must be gone: {y}");
    let c = generate("cicd", &cicd_rows()).unwrap();
    let test = c.split("\n  - name: ").find(|b| b.starts_with("skill-werk-test-pipeline")).expect("cicd has its test step");
    assert!(test.contains("dag: cicd-test") && test.contains("SCOPE=diff"), "the gate moved here, it must still run: {test}");
}

/// Negative proof (AC3): a cicd whose test step stopped running cicd-test
/// SCOPE=diff is caught by the same check that passes above.
#[test]
fn negative_proof_ac3_a_cicd_without_the_test_step_is_caught() {
    let rows: Vec<Row> = cicd_rows().into_iter().filter(|r| r.implemented_by != "cicd-test").collect();
    let c = generate("cicd", &rows).unwrap();
    assert!(c.split("\n  - name: ").find(|b| b.contains("dag: cicd-test") && b.contains("SCOPE=diff")).is_none(), "{c}");
}

/// Negative proof (AC2): a handler that kept only the builder's nudge fails the check.
#[test]
fn negative_proof_ac2_a_handler_without_the_route_is_caught() {
    let plain = "'${CHORUS_HOME}/platform/scripts/ops-nudge ${ROLE} \"werk v2\"'";
    assert!(!plain.starts_with(&format!("'{ROUTED}")));
}
