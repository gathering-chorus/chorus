// @test-type: unit
// @domain: builds
//! #4838 — the model door deploys before any caller that asks it a new query.
use werk_deploy::{deploy_order, split_door_restarts};

fn v(a: &[&str]) -> Vec<String> { a.iter().map(|s| s.to_string()).collect() }

#[test]
fn the_door_deploys_first_whatever_order_the_diff_names_crates() {
    let got = deploy_order(&v(&["chorus-hooks", "chorus-principal", "athena-make"]), &v(&["platform/api", "platform/pulse"]));
    assert_eq!(got, v(&["athena-make", "chorus-hooks", "chorus-principal", "platform/api", "platform/pulse"]));
    // NEGATIVE PROOF: without the door in the land, order is the diff's, TS last
    assert_eq!(deploy_order(&v(&["chorus-principal", "chorus-hooks"]), &v(&["platform/api"])), v(&["chorus-principal", "chorus-hooks", "platform/api"]));
}

/// Kade 15:07: the door's restart, not its install, is what serves the new query.
/// The restart list comes sorted by label, so com.chorus.api sorts first; the
/// door's restart is taken out and run before any TS caller deploys.
#[test]
fn the_doors_restart_is_split_out_to_run_before_the_callers() {
    let sorted = v(&["com.chorus.api", "com.chorus.athena-make", "com.chorus.pulse"]);
    assert_eq!(sorted[0], "com.chorus.api", "NEGATIVE PROOF: sorted, a caller restarts before the door");
    let (door, rest) = split_door_restarts(sorted);
    assert_eq!(door, v(&["com.chorus.athena-make"]));
    assert_eq!(rest, v(&["com.chorus.api", "com.chorus.pulse"]));
}
