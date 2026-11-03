// @test-type: unit
// @domain: builds
//! #4838 — the model door deploys before any caller that asks it a new query.
use werk_deploy::deploy_order;

fn v(a: &[&str]) -> Vec<String> { a.iter().map(|s| s.to_string()).collect() }

#[test]
fn the_door_deploys_first_whatever_order_the_diff_names_crates() {
    let got = deploy_order(&v(&["chorus-hooks", "chorus-principal", "athena-make"]), &v(&["platform/api", "platform/pulse"]));
    assert_eq!(got, v(&["athena-make", "chorus-hooks", "chorus-principal", "platform/api", "platform/pulse"]));
    // NEGATIVE PROOF: without the door in the land, order is the diff's, TS last
    assert_eq!(deploy_order(&v(&["chorus-principal", "chorus-hooks"]), &v(&["platform/api"])), v(&["chorus-principal", "chorus-hooks", "platform/api"]));
}
