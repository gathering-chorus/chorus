// @test-type: unit — the open row a logout reads from the door, made fit to PUT back
// @domain: identity
//! #4934 AC1 — measured 2026-11-09 08:52:31, borg-queen logout
//! (trace login-logout-borg-queen-1794232350457-66953): the run and presence
//! PUTs were 422 "off-model property 'status' is not in the shape". The row came
//! from the door's listing, which adds `status` (and "" for absent fields), and
//! went back unchanged, so the run never ended and the presence stayed reachable.
//!
//! NEGATIVE PROOF (#3734): with the listing row returned as-is, the `status`
//! assertions below go red.

use chorus_principal::rows::{ended_run, gone_presence, open_row_from_listing};
use serde_json::{json, Value};

fn listing(rows: Value) -> String { json!({ "data": rows }).to_string() }

#[test]
fn the_open_run_read_from_the_listing_carries_no_status_and_no_empty_fields() {
    let l = listing(json!([
        {"name":"borg-queen-run-1a1bc2dd380","runOf":"borg-queen-1uyysty-1a1bc2dcf53","status":"open","runEndedAt":"","endReason":""},
    ]));
    let run = open_row_from_listing(&l, "borg-queen", "identity/sessionruns", "run", "runEndedAt").expect("the open run");
    assert!(run.get("status").is_none(), "status is off-model: {run}");
    assert!(run.get("runEndedAt").is_none(), "an empty field is refused as a name: {run}");
    let ended = ended_run(run, "2026-11-09T13:52:31Z", "logout").expect("logout is a reason");
    assert!(ended.get("status").is_none(), "the PUT a logout sends: {ended}");
    assert_eq!(ended["endReason"], "logout");
}

#[test]
fn the_presence_read_from_the_listing_carries_no_status() {
    let l = listing(json!([
        {"name":"borg-queen-presence-1a1bc2dd380","reachability":"reachable","status":"live","pane":"%21"},
    ]));
    let p = open_row_from_listing(&l, "borg-queen", "identity/presences", "presence", "").expect("the presence");
    let gone = gone_presence(p).expect("named");
    assert!(gone.get("status").is_none(), "the PUT a logout sends: {gone}");
    assert_eq!(gone["reachability"], "unreachable");
}

#[test]
fn the_newest_open_row_of_the_role_is_the_one_read_never_another_roles() {
    let l = listing(json!([
        {"name":"borg-queen-run-1a0","status":"open"},
        {"name":"borg-queen-run-1a1","status":"open"},
        {"name":"kade-run-1a9","status":"open"},
        {"name":"borg-queen-run-1a2","runEndedAt":"2026-11-09T10:00:00Z","status":"ended"},
    ]));
    let run = open_row_from_listing(&l, "borg-queen", "identity/sessionruns", "run", "runEndedAt").expect("one");
    assert_eq!(run["name"], "borg-queen-run-1a1");
}

#[test]
fn an_unreachable_presence_is_not_read_as_open() {
    let l = listing(json!([{"name":"borg-queen-presence-1a0","reachability":"unreachable","status":"gone"}]));
    assert_eq!(open_row_from_listing(&l, "borg-queen", "identity/presences", "presence", ""), None);
}
