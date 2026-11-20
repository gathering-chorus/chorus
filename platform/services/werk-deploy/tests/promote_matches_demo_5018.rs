// @test-type: unit
// @domain: builds
//! #5018 — AC1: the land installs the binary the demo proved, or stops. #4555's land
//! (11-20 10:15) promoted run 8's werk-test (cdhash ed73d4f9…) while Jeff's demo
//! was run 9's (ec8ee06a…): deploy.cdhash.diverged was only a warning and the
//! land finished green with the other binary in prod.
use werk_deploy::{promote_refusal, promote_verdict, PromoteVerdict};

const RUN8: &str = "ed73d4f980e7876f67cbdabd20420f37894d2931";
const RUN9: &str = "ec8ee06a219faed7efc7ebf2e918de7962198ae6";

fn p(c: &str, h: &str) -> (String, String) { (c.to_string(), h.to_string()) }

#[test]
fn the_demos_own_binary_ships() {
    let demo = vec![p("werk-test", RUN9)];
    assert_eq!(promote_verdict(&demo, "werk-test", Some(RUN9)), PromoteVerdict::Ship);
}

#[test]
fn a_crate_with_no_demo_cdhash_is_unverified_not_refused() {
    let demo = vec![p("werk-build", RUN9)];
    assert_eq!(promote_verdict(&demo, "werk-test", Some(RUN8)), PromoteVerdict::Unverified);
    assert_eq!(promote_verdict(&[], "werk-test", Some(RUN8)), PromoteVerdict::Unverified);
}

#[test]
fn the_stop_line_names_the_crate_and_both_cdhashes() {
    let m = promote_refusal("werk-test", RUN9, RUN8);
    assert!(m.contains("werk-test") && m.contains(RUN9) && m.contains(RUN8), "{m}");
    assert!(m.contains("nothing installed"), "{m}");
}
