// #4456 — a step that names where its failure goes (onFail) makes the
// failure write that verdict; a pipeline that names none keeps the nudge.
use chorus_make::{fail_routes, generate, generate_land, parse_rows};

const ROWS: &str = "step\tstepOrder\tskillOrder\tskill\tlabel\texecutor\timplementedBy\tmode\tonFail
intake\t1\t1\tskill-prove-intake\tprove-intake\tdeterministic\tborg-prove\tintake ${CARD} ${ROLE}\thttps://jeffbridwell.com/chorus#value-stream-step-designing
stage\t3\t1\tskill-prove-stage\tprove-stage\tdeterministic\tborg-prove\tstage ${CARD} ${ROLE}\thttps://jeffbridwell.com/chorus#value-stream-step-build
accept\t5\t1\tskill-werk-go\tJeff's go\thuman\t\t\t
verify-in-prod\t6\t1\tskill-prove-in-prod\tprove-in-prod\tdeterministic\tborg-prove\tprod ${CARD} ${ROLE}\thttps://jeffbridwell.com/chorus#value-stream-step-borg-surface
";

#[test]
fn every_step_that_names_a_route_is_routed_by_its_failure() {
    let rows = parse_rows(ROWS).unwrap();
    let card = generate("borg-prove", &rows).unwrap();
    assert!(card.contains("command: '${CHORUS_HOME}/platform/scripts/borg-prove route ${CARD} ${ROLE} ${DAG_RUN_ID} skill-prove-intake=value-stream-step-designing,skill-prove-stage=value-stream-step-build || ${CHORUS_HOME}/platform/scripts/ops-nudge ${ROLE}"), "{card}");
    assert!(card.contains("its route failed too"), "a failed route still reaches the builder");
    let land = generate_land("borg-prove", &rows).unwrap();
    assert!(land.contains("skill-prove-in-prod=value-stream-step-borg-surface ||"), "{land}");
    assert!(!land.contains("skill-prove-intake="), "the land routes only its own steps");
}

#[test]
fn negative_proof_a_dropped_route_is_gone_and_no_routes_means_the_nudge() {
    let mut rows = parse_rows(ROWS).unwrap();
    rows[1].on_fail.clear();
    assert_eq!(fail_routes(&rows), "skill-prove-intake=value-stream-step-designing,skill-prove-in-prod=value-stream-step-borg-surface");
    for r in rows.iter_mut() { r.on_fail.clear(); }
    let card = generate("borg-prove", &rows).unwrap();
    assert!(card.contains("ops-nudge") && !card.contains("borg-prove route"), "{card}");
}
