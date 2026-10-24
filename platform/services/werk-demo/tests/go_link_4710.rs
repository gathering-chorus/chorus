// @test-type: unit — pure string builder, no env beyond its own default
// @domain: security
// @card: #4710
// @owner: silas
//! #4710 — src/lib.rs `go_link`: the demo-ready nudge carries the page where
//! Jeff's phone signs this run's go — the card, the cicd run and the role, so
//! the page asks for the right round.
use werk_demo::go_link;

#[test]
fn the_go_link_names_the_card_the_run_and_the_role() {
    std::env::remove_var("CHORUS_GO_ORIGIN");
    assert_eq!(go_link(4710, "034lcDEndMqLApO6LeLeee", "silas"),
        "https://lightlifeurbangardens.com/go/?card=4710&run=034lcDEndMqLApO6LeLeee&role=silas");
}
