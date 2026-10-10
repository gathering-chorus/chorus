//! #4474 — werk v2's dagu step demos a card's werk without asserting an
//! identity: `werk-demo <card> <role>` names the builder role like every other
//! werk verb. Jeff 2026-10-09: "isnt deploy role = identity".
use werk_demo::demo_role;

fn argv(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

#[test]
fn the_ceremony_takes_the_role_from_its_argument_with_no_deploy_role() {
    assert_eq!(demo_role(&argv(&["4474", "kade"]), "").unwrap(), "kade");
}

#[test]
fn without_the_argument_deploy_role_still_answers_as_in_v1() {
    assert_eq!(demo_role(&argv(&["4474"]), "wren").unwrap(), "wren");
}

#[test]
fn a_subcommand_never_reads_its_operands_as_the_role() {
    // `gate <card> <gate>`: "4474" is not the first word, so the role is DEPLOY_ROLE
    assert_eq!(demo_role(&argv(&["gate", "4474", "code"]), "silas").unwrap(), "silas");
}

// NEGATIVE PROOF: no argument and no DEPLOY_ROLE is refused, never a blank role.
#[test]
fn no_role_anywhere_is_refused() {
    assert!(demo_role(&argv(&["4474"]), "").is_err());
    assert!(demo_role(&argv(&["4474", "  "]), " ").is_err());
}
