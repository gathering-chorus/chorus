// @test-type: unit — what a logout sends to close the open run and presence it reads from the door
// @domain: identity
//! #4934 AC1 — measured 2026-11-09 08:52:31, borg-queen logout
//! (trace login-logout-borg-queen-1794232350457-66953): the run and presence
//! PUTs were 422 "off-model property 'status' is not in the shape". The row came
//! from the door's listing and went back whole. Stripping `status` was not enough:
//! on the demo door (09:17) the same row was then 422 "double-prefix:
//! 'session-wren-…'" on `runOf`, a minted link the listing returns. A logout now
//! sends only what it changes; the demo door answered that PATCH 200.
//!
//! NEGATIVE PROOF (#3734): the bodies below are the listing rows the door refused.
//! Sent whole (the old put_row), each assertion on `status`, `runOf`, `presenceOf`
//! or `reachableOver` goes red.

use chorus_principal::rows::{ended_run, gone_presence, open_row_from_listing, patch_of};
use serde_json::{json, Value};

fn listing(rows: Value) -> String { json!({ "data": rows }).to_string() }

/// The run as the demo door's listing returned it at 09:16.
fn run_row() -> Value {
    json!({"name":"wren-run-1a1bb0046e0","label":"wren run 2026-11-08T11:51:59Z","status":"","conversationId":"pending",
           "endReason":"","ownedBy":"principal-wren","runEndedAt":"","runOf":"session-wren-s94suie6-1a1b371f17b","startedAt":"2026-11-08T11:51:59Z"})
}

/// The presence as the demo door's listing returned it at 09:18.
fn presence_row() -> Value {
    json!({"name":"wren-presence-1a1bb0046e0","status":"","hostAccount":"chorus-wren","onMachine":"","ownedBy":"principal-wren","pane":"%1",
           "presenceOf":"session-run-wren-run-1a1bb0046e0","reachability":"reachable","reachableOver":"channel-nudge","tty":"/dev/ttys011"})
}

#[test]
fn logout_sends_only_the_run_fields_it_ends() {
    let run = open_row_from_listing(&listing(json!([run_row()])), "wren", "identity/sessionruns", "run", "runEndedAt").expect("the open run");
    let ended = ended_run(run, "2026-11-09T13:52:31Z", "logout").expect("logout is a reason");
    let (name, body) = patch_of(&ended, &["runEndedAt", "endReason"]).expect("named, both fields set");
    assert_eq!(name, "wren-run-1a1bb0046e0");
    assert_eq!(body, json!({"runEndedAt":"2026-11-09T13:52:31Z","endReason":"logout"}));
    for refused in ["status", "runOf", "conversationId", "label"] {
        assert!(body.get(refused).is_none(), "{refused} is not the logout's to send: {body}");
    }
}

#[test]
fn logout_sends_only_the_presence_reachability() {
    let p = open_row_from_listing(&listing(json!([presence_row()])), "wren", "identity/presences", "presence", "").expect("the presence");
    let gone = gone_presence(p).expect("named");
    let (name, body) = patch_of(&gone, &["reachability"]).expect("named");
    assert_eq!(name, "wren-presence-1a1bb0046e0");
    assert_eq!(body, json!({"reachability":"unreachable"}));
    for refused in ["status", "presenceOf", "reachableOver", "onMachine"] {
        assert!(body.get(refused).is_none(), "{refused} is not the logout's to send: {body}");
    }
}

#[test]
fn a_field_the_row_does_not_have_is_no_patch_never_a_null() {
    assert_eq!(patch_of(&json!({"name":"x"}), &["runEndedAt"]), None);
    assert_eq!(patch_of(&json!({"runEndedAt":"t"}), &["runEndedAt"]), None, "no name, no write");
}

#[test]
fn the_newest_open_row_of_the_role_is_the_one_read_never_another_roles() {
    let l = listing(json!([
        {"name":"borg-queen-run-1a0","runEndedAt":""},
        {"name":"borg-queen-run-1a1","runEndedAt":""},
        {"name":"kade-run-1a9","runEndedAt":""},
        {"name":"borg-queen-run-1a2","runEndedAt":"2026-11-09T10:00:00Z"},
    ]));
    let run = open_row_from_listing(&l, "borg-queen", "identity/sessionruns", "run", "runEndedAt").expect("one");
    assert_eq!(run["name"], "borg-queen-run-1a1");
}

#[test]
fn an_unreachable_presence_is_not_read_as_open() {
    let l = listing(json!([{"name":"borg-queen-presence-1a0","reachability":"unreachable"}]));
    assert_eq!(open_row_from_listing(&l, "borg-queen", "identity/presences", "presence", ""), None);
}
