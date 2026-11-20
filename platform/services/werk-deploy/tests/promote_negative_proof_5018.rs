// @test-type: unit
// @domain: builds
//! #5018 — AC2 negative proof: the #4555 land state is refused. #4555's land
//! (11-20 10:15) promoted run 8's werk-test (cdhash ed73d4f9…) while Jeff's demo
//! was run 9's (ec8ee06a…): deploy.cdhash.diverged was only a warning and the
//! land finished green with the other binary in prod.
use werk_deploy::{may_promote, promote_verdict, PromoteVerdict};

const RUN8: &str = "ed73d4f980e7876f67cbdabd20420f37894d2931";
const RUN9: &str = "ec8ee06a219faed7efc7ebf2e918de7962198ae6";

fn p(c: &str, h: &str) -> (String, String) { (c.to_string(), h.to_string()) }

#[test]
fn negative_proof_the_4555_land_state_is_refused_where_todays_gate_let_it_through() {
    // the 10:15 state: werk and main trees equal, so today's only gate promotes …
    let trees = vec![p("platform/services/werk-test", "abc"), p("platform/services/shared", "def")];
    assert!(may_promote(&trees, &trees.clone()), "today's gate ships this fixture");
    // … while the file it would install is run 8's build and the demo was run 9's
    let demo = vec![p("werk-test", RUN9)];
    assert_eq!(promote_verdict(&demo, "werk-test", Some(RUN8)),
        PromoteVerdict::Refuse { demo: RUN9.into(), file: RUN8.into() });
}

#[test]
fn an_unreadable_file_for_a_demoed_crate_is_refused_never_shipped_blind() {
    let demo = vec![p("werk-test", RUN9)];
    assert!(matches!(promote_verdict(&demo, "werk-test", None), PromoteVerdict::Refuse { .. }));
}
