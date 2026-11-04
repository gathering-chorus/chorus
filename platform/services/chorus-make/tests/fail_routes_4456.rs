// @test-type: unit
// @domain: pipelines
// #4456 — a step that names where its failure goes (onFail) makes the
// failure write that verdict. #4888 — the failure only routes; a failed route,
// like any red, reaches the builder through the run's one exit wake (run-finished).
use chorus_make::{fail_routes, generate, generate_land, parse_rows};

const ROWS: &str = "step\tstepOrder\tskillOrder\tskill\tlabel\texecutor\timplementedBy\tmode\tonFail
intake\t1\t1\tskill-borg-intake\tborg-intake\tdeterministic\tborg-prove\tintake ${CARD} ${ROLE}\thttps://jeffbridwell.com/chorus#value-stream-step-designing
stage\t3\t1\tskill-borg-stage\tborg-stage\tdeterministic\tborg-prove\tstage ${CARD} ${ROLE}\thttps://jeffbridwell.com/chorus#value-stream-step-build
accept\t5\t1\tskill-werk-go\tJeff's go\thuman\t\t\t
verify-in-prod\t6\t1\tskill-borg-verify-in-prod\tborg-verify-in-prod\tdeterministic\tborg-prove\tprod ${CARD} ${ROLE}\thttps://jeffbridwell.com/chorus#value-stream-step-borg-surface
";

#[test]
fn every_step_that_names_a_route_is_routed_by_its_failure() {
    let rows = parse_rows(ROWS).unwrap();
    let card = generate("borg-prove", &rows).unwrap();
    // #4752 — a paired card's red goes to its pair first (`pair routed … ||`)
    assert!(card.contains("command: '${CHORUS_HOME}/platform/scripts/pair routed ${CARD} ${DAG_NAME} ${DAG_RUN_ID} || ${CHORUS_HOME}/platform/scripts/borg-prove route ${CARD} ${ROLE} ${DAG_RUN_ID} skill-borg-intake=value-stream-step-designing,skill-borg-stage=value-stream-step-build || true'"), "{card}");
    assert!(card.contains("  exit:\n    command: '${CHORUS_HOME}/platform/scripts/run-finished "), "a failed route still reaches the builder: {card}");
    let land = generate_land("borg-prove", &rows).unwrap();
    assert!(land.contains("skill-borg-verify-in-prod=value-stream-step-borg-surface ||"), "{land}");
    assert!(!land.contains("skill-borg-intake="), "the land routes only its own steps");
}

#[test]
fn negative_proof_a_dropped_route_is_gone_and_no_routes_means_no_route() {
    let mut rows = parse_rows(ROWS).unwrap();
    rows[1].on_fail.clear();
    assert_eq!(fail_routes(&rows), "skill-borg-intake=value-stream-step-designing,skill-borg-verify-in-prod=value-stream-step-borg-surface");
    for r in rows.iter_mut() { r.on_fail.clear(); }
    let card = generate("borg-prove", &rows).unwrap();
    assert!(!card.contains("borg-prove route") && !card.contains("ops-nudge"), "{card}");
    assert!(card.contains("platform/scripts/run-finished"), "{card}");
}
