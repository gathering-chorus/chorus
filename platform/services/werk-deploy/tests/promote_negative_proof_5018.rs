// @test-type: unit
// @domain: builds
//! #5018 AC2 NEGATIVE PROOF: the 10:15 state of the #4555 land is refused; the same
//! fixture goes through today's only gate (may_promote: werk and main trees equal).
//! #4555 land (11-20 10:15) promoted run 8 werk-test (cdhash ed73d4f9) while
//! the demo Jeff saw was run 9 (ec8ee06a); the divergence only warned and the land went green.
use werk_deploy::{may_promote, promote_plan};

const RUN8: &str = "ed73d4f980e7876f67cbdabd20420f37894d2931";
const RUN9: &str = "ec8ee06a219faed7efc7ebf2e918de7962198ae6";

fn p(c: &str, h: &str) -> (String, String) { (c.to_string(), h.to_string()) }

#[test]
fn negative_proof_the_4555_land_state_is_refused_where_todays_gate_let_it_through() {
    let trees = vec![p("platform/services/werk-test", "abc"), p("platform/services/shared", "def")];
    assert!(may_promote(&trees, &trees.clone()), "today's gate ships this fixture");
    let r = promote_plan(&[p("werk-test", RUN9)], &[("werk-test".into(), vec![Some(RUN8.into())])]);
    assert!(r.is_err(), "the land must refuse run 8's binary for run 9's demo, got {r:?}");
}

#[test]
fn control_the_demos_own_build_in_the_same_fixture_ships() {
    let r = promote_plan(&[p("werk-test", RUN9)], &[("werk-test".into(), vec![Some(RUN9.into())])]);
    assert!(r.is_ok(), "{r:?}");
}
