// @test-type: unit
// @domain: builds
//! #5018 — the land installs the binary the demo proved, or stops. #4555's land
//! (11-20 10:15) promoted run 8's werk-test (cdhash ed73d4f9…) while Jeff's demo
//! was run 9's (ec8ee06a…): deploy.cdhash.diverged was only a warning and the
//! land finished green with the other binary in prod.
use werk_deploy::{may_promote, promote_refusal, promote_verdict, PromoteVerdict};

const RUN8: &str = "ed73d4f980e7876f67cbdabd20420f37894d2931";
const RUN9: &str = "ec8ee06a219faed7efc7ebf2e918de7962198ae6";

fn p(c: &str, h: &str) -> (String, String) { (c.to_string(), h.to_string()) }

#[test]
fn the_demos_own_binary_ships() {
    let demo = vec![p("werk-test", RUN9)];
    assert_eq!(promote_verdict(&demo, "werk-test", Some(RUN9)), PromoteVerdict::Ship);
}

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

#[test]
fn a_promoted_divergence_stops_the_land_before_any_install() {
    // structural: the refusal returns Err from promote_crates (before the copy and before
    // any install), and the later divergence block also returns Err for a promoted crate,
    // so deploy.cdhash.diverged on a promoted crate can never be followed by deploy.completed
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs")).unwrap();
    let promote = src.find("fn promote_crates(").expect("promote_crates is gone: this check must be moved, not dropped");
    let body = &src[promote..];
    let refuse = body.find("promote_verdict(demo, c, file_hash.as_deref())").expect("promote no longer checks the demo cdhash");
    let copy = body.find("\"cp\", &[\"-c\"").expect("promote copy moved");
    assert!(refuse < copy, "the demo check must come before the copy");
    assert!(src.contains("promoted_names.contains(c)"), "the divergence block no longer stops on a promoted crate");
    assert!(src.contains("&crates, &demo)?;"), "deploy_canonical no longer stops on a refused promote");
}
