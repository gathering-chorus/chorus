// @test-type: unit
// @domain: builds
//! #5018 AC1: the land promotes a crate only when a built file has the demo cdhash;
//! otherwise it stops before anything is copied, naming the crate and both cdhashes.
//! #4555 land (11-20 10:15) promoted run 8 werk-test (cdhash ed73d4f9) while
//! the demo Jeff saw was run 9 (ec8ee06a); the divergence only warned and the land went green.
use werk_deploy::{promote_plan, promote_refusal, promote_verdict, PromoteVerdict};

const RUN8: &str = "ed73d4f980e7876f67cbdabd20420f37894d2931";
const RUN9: &str = "ec8ee06a219faed7efc7ebf2e918de7962198ae6";
const OTHER: &str = "0123456789abcdef0123456789abcdef01234567";

fn p(c: &str, h: &str) -> (String, String) { (c.to_string(), h.to_string()) }
fn some(h: &str) -> Option<String> { Some(h.to_string()) }

#[test]
fn the_demos_own_binary_ships() {
    assert_eq!(promote_verdict(&[p("werk-test", RUN9)], "werk-test", &[some(RUN9)]), PromoteVerdict::Ship(RUN9.into()));
}

#[test]
fn a_different_binary_is_refused() {
    assert_eq!(promote_verdict(&[p("werk-test", RUN9)], "werk-test", &[some(RUN8)]),
        PromoteVerdict::Refuse { demo: RUN9.into(), file: RUN8.into() });
}

#[test]
fn a_multi_binary_crate_ships_when_its_demoed_binary_matches() {
    // the demo records chorus-hooks by its shim; the daemon binary is listed first
    let files = [some(OTHER), some(RUN9)];
    assert_eq!(promote_verdict(&[p("chorus-hooks", RUN9)], "chorus-hooks", &files), PromoteVerdict::Ship(RUN9.into()));
}

#[test]
fn the_land_stops_with_a_line_naming_the_crate_and_both_cdhashes() {
    let err = promote_plan(&[p("werk-test", RUN9)], &[("werk-test".into(), vec![some(RUN8)])]).unwrap_err();
    assert_eq!(err, promote_refusal("werk-test", RUN9, RUN8));
    assert!(err.contains("werk-test") && err.contains(RUN9) && err.contains(RUN8), "{err}");
}

#[test]
fn no_demo_cdhash_ships_unverified_and_no_readable_file_rebuilds() {
    assert_eq!(promote_verdict(&[p("werk-build", RUN9)], "werk-test", &[some(RUN8)]), PromoteVerdict::Unverified(RUN8.into()));
    assert_eq!(promote_verdict(&[p("werk-test", RUN9)], "werk-test", &[None]), PromoteVerdict::Unreadable);
}
