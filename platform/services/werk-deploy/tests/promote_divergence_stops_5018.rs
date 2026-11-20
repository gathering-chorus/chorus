// @test-type: unit
// @domain: builds
//! #5018 — AC3: a promoted divergence never ends green. #4555's land
//! (11-20 10:15) promoted run 8's werk-test (cdhash ed73d4f9…) while Jeff's demo
//! was run 9's (ec8ee06a…): deploy.cdhash.diverged was only a warning and the
//! land finished green with the other binary in prod.


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
