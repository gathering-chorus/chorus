// @domain: spine — placed by chorus-crawl's domain rules (#4538)
//! #4895 AC1 — `reachable` means a delivery just landed, never that a pane was recorded.
//! Measured 11-05 14:02: Abby's and BQ's Presences read reachable on panes %16/%14
//! while every nudge to them failed "can't find pane". Both directions are proven:
//! a failed delivery makes the presence unreachable at once, and a moved pane is not
//! reachable until a delivery lands there.
use chorus_principal::rows::*;
use serde_json::json;

fn presence(pane: &str, reach: &str) -> serde_json::Value {
    json!({"name": "abby-normal-presence-x", "pane": pane, "tty": "/dev/ttys015", "reachability": reach, "lastDeliveredAt": "2026-11-05T17:44:39Z"})
}

#[test]
fn a_failed_delivery_on_the_recorded_pane_makes_it_unreachable() {
    let p = undelivered_presence(presence("%16", "reachable"), "%16").expect("written");
    assert_eq!(p["reachability"], "unreachable");
    assert_eq!(p["pane"], "%16", "the pane stays: the sweep moves it, not the failure");
}

// NEGATIVE PROOF (#3734) — a failure on a pane the sweep already moved away from says
// nothing about the new pane: it must not be marked.
#[test]
fn negative_proof_a_failure_on_an_old_pane_leaves_the_new_pane_alone() {
    assert!(undelivered_presence(presence("%17", "unknown"), "%16").is_none());
}

#[test]
fn an_already_unreachable_presence_is_not_rewritten() {
    assert!(undelivered_presence(presence("%16", "unreachable"), "%16").is_none());
}

#[test]
fn a_moved_pane_is_unknown_until_a_delivery_lands() {
    let p = repaned_presence(presence("%16", "reachable"), "%17", "/dev/ttys016").expect("moved");
    assert_eq!(p["pane"], "%17");
    assert_eq!(p["reachability"], "unknown", "a pane nobody delivered to is not reachable");
    let p = delivered_presence(p, "2026-11-06T14:42:12Z").unwrap();
    assert_eq!(p["reachability"], "reachable");
    assert_eq!(p["lastDeliveredAt"], "2026-11-06T14:42:12Z");
}

// NEGATIVE PROOF — the 11-05 state: before #4895 a moved presence kept "reachable" from
// the old pane. Same pane in → no write, so reachability is never touched by a no-op.
#[test]
fn negative_proof_the_same_pane_is_not_a_move() {
    assert!(repaned_presence(presence("%17", "reachable"), "%17", "").is_none());
}
