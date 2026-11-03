// @test-type: unit — pure: the owner's demo brief; no door, no network.
// @domain: decisions
// @card: #4875
//! #4875 AC1/AC3: the automated announce goes to the owner, not Jeff. It never carries a go
//! link, and a red gate is named. Log evidence (Loki, platform-chorus, 2026-11-03 06:58:45):
//! "#4873 · gates ✗ · AC 0/2 · decide: …/go/approvals" was sent with two reds.
use werk_demo::{approvals_link, owner_brief};

const LINE: &str = "#4875 round abc1234 ready · gates ✓ · reviews 3/3 · tests 9 ok · AC 4/4";

#[test]
fn a_green_brief_tells_the_owner_to_show_then_vouch_and_carries_no_go_link() {
    let b = owner_brief(4875, LINE, true, &[]);
    assert!(!b.contains(&approvals_link()), "no go link in the automated announce: {b}");
    assert!(!b.contains("/go/"), "no go page of any kind: {b}");
    assert!(b.contains("werk-demo vouch 4875"), "tells the owner the one next step: {b}");
}

#[test]
fn negative_proof_a_red_brief_names_each_red_gate_and_offers_no_vouch() {
    let reds = vec!["gate-product fail".to_string(), "gate-ops fail".to_string()];
    let b = owner_brief(4873, LINE, false, &reds);
    assert!(b.contains("gate-product") && b.contains("gate-ops"), "names the reds: {b}");
    assert!(!b.contains("werk-demo vouch"), "a red demo is not vouchable: {b}");
    assert!(!b.contains("/go/"), "{b}");
}

#[test]
fn negative_proof_a_gate_that_did_not_run_is_not_green() {
    // gates_green can be true while a gate never reported ("-"): that is still not vouchable.
    let b = owner_brief(4875, LINE, true, &["gate-arch -".to_string()]);
    assert!(!b.contains("werk-demo vouch"), "an unrun gate blocks the vouch: {b}");
    assert!(b.contains("gate-arch"), "{b}");
}
