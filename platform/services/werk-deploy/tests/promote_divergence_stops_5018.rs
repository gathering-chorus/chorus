// @test-type: unit
// @domain: builds
//! #5018 AC3: a promoted crate's divergence never ends green. The plan is decided for
//! every crate before any copy: one diverged crate fails the whole plan, so no crate of
//! that land is copied, installed or reported deployed.
//! #4555 land (11-20 10:15) promoted run 8 werk-test (cdhash ed73d4f9) while
//! the demo Jeff saw was run 9 (ec8ee06a); the divergence only warned and the land went green.
use werk_deploy::{promote_plan, PromoteVerdict};

const RUN8: &str = "ed73d4f980e7876f67cbdabd20420f37894d2931";
const RUN9: &str = "ec8ee06a219faed7efc7ebf2e918de7962198ae6";

fn p(c: &str, h: &str) -> (String, String) { (c.to_string(), h.to_string()) }

#[test]
fn one_diverged_crate_fails_the_whole_plan_even_after_a_matching_one() {
    let demo = [p("werk-build", RUN9), p("werk-test", RUN9)];
    let cands = [("werk-build".to_string(), vec![Some(RUN9.to_string())]), ("werk-test".to_string(), vec![Some(RUN8.to_string())])];
    let r = promote_plan(&demo, &cands);
    assert!(r.is_err(), "werk-build matched but werk-test diverged: nothing may ship, got {r:?}");
}

#[test]
fn control_an_all_matching_plan_ships_every_crate() {
    let demo = [p("werk-build", RUN9), p("werk-test", RUN8)];
    let cands = [("werk-build".to_string(), vec![Some(RUN9.to_string())]), ("werk-test".to_string(), vec![Some(RUN8.to_string())])];
    let plan = promote_plan(&demo, &cands).unwrap();
    assert_eq!(plan, vec![("werk-build".to_string(), PromoteVerdict::Ship(RUN9.into())), ("werk-test".to_string(), PromoteVerdict::Ship(RUN8.into()))]);
}
