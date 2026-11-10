// @test-type: unit — what the sweep, keep and seen send for a Session row, against a fake door
// @domain: identity
//! #4944 AC1 — measured 2026-11-10 07:44: session-sweep.log, every minute since
//! 11-09 13:19, "abby-normal-qcdoezvh-1a1c0eb7621 not closed (HTTP 422)". The close
//! PUT the whole listing row back (closed_row via putable); keep and seen PUT the
//! cached session.row.json. A Session write now sends only the fields it changes.
mod common_4944;
use chorus_principal::rows::{changed_fields, session_close};
use common_4944::*;
use serde_json::json;

#[test]
fn the_sweep_closes_an_expired_session_with_a_patch_of_two_fields() {
    let home = tempdir("close");
    door(&home, "wren", "200");
    let (code, out) = sweep(&home);
    let calls = read(home.join("calls.log"));
    assert_eq!(code, 0, "sweep: {out}\ncalls: {calls}");
    assert!(out.contains("1 expired session(s) closed"), "{out}");
    let line = calls.lines().find(|l| l.contains("identity/sessions/wren-s-1")).unwrap_or_else(|| panic!("no write: {calls}"));
    assert!(line.starts_with("PATCH "), "a PATCH, never a PUT: {line}");
    let body: serde_json::Value = serde_json::from_str(line.splitn(3, ' ').nth(2).unwrap()).unwrap();
    assert_eq!(body.as_object().unwrap().len(), 2, "only the two fields it changes: {body}");
    assert_eq!(body["sessionState"], "closed");
    assert!(body["endedAt"].as_str().unwrap_or("").ends_with('Z'), "{body}");
}

#[test]
fn a_close_names_its_owner_and_carries_ended_by_only_when_given() {
    let row = json!({"name":"wren-s-1","ownedBy":"principal-wren","status":"","tokenId":"t","sessionState":"open"});
    let (owner, name, body) = session_close(&row, "2026-11-10T12:00:00Z", None).unwrap();
    assert_eq!((owner.as_str(), name.as_str()), ("wren", "wren-s-1"));
    assert_eq!(body, json!({"sessionState":"closed","endedAt":"2026-11-10T12:00:00Z"}));
    let (_, _, by) = session_close(&row, "2026-11-10T12:00:00Z", Some("jeff")).unwrap();
    assert_eq!(by["endedBy"], "jeff");
    assert!(session_close(&json!({"name":"x"}), "t", None).is_none(), "no owner, no close");
}

/// Silas (pair 4944): the cache is stale (the door's row moved on since it was saved);
/// diffing the cache before and after our change sends only what our code set.
/// NEGATIVE PROOF: the old put_row sent `after` whole, so `status`, `runOf` and the
/// stale `expiresAt` would all be in the body and these asserts go red.
#[test]
fn a_stale_cache_still_sends_only_last_seen() {
    let cached = json!({"name":"wren-s-1","status":"","runOf":"session-wren-s-1","expiresAt":"2026-11-09T10:10:00Z","lastSeenAt":"2026-11-09T10:00:00Z","sessionState":"open"});
    let mut seen = cached.clone();
    seen["lastSeenAt"] = json!("2026-11-10T12:00:00Z");
    let (name, body) = changed_fields(&cached, &seen).unwrap();
    assert_eq!(name, "wren-s-1");
    assert_eq!(body, json!({"lastSeenAt":"2026-11-10T12:00:00Z"}));
}

#[test]
fn keep_sends_only_the_new_expiry_and_nothing_when_nothing_moved() {
    let cached = json!({"name":"wren-s-1","status":"","expiresAt":"2026-11-10T11:00:00Z"});
    let mut kept = cached.clone();
    kept["expiresAt"] = json!("2026-11-10T12:10:00Z");
    assert_eq!(changed_fields(&cached, &kept).unwrap().1, json!({"expiresAt":"2026-11-10T12:10:00Z"}));
    assert_eq!(changed_fields(&cached, &cached), None, "no change, no write");
    assert_eq!(changed_fields(&json!({}), &json!({"lastSeenAt":"t"})), None, "no name, no write");
}

/// Silas (pair 4944), measured 07:54: abby-normal's session read expired while her run was
/// live (runOf = "session-<name>", the minted form the door returns). A live run blocks the close.
#[test]
fn a_session_a_live_run_points_at_is_never_closed() {
    let home = tempdir("live-run");
    door_with_runs(&home, "wren", "200", r#"{"data":[{"name":"wren-run-1","runOf":"session-wren-s-1","runEndedAt":""}]}"#);
    let (code, out) = sweep(&home);
    assert_eq!(code, 0, "{out}");
    assert!(read(home.join("calls.log")).is_empty(), "no write to a live run's session: {}", read(home.join("calls.log")));
    assert!(out.contains("0 expired session(s) closed"), "{out}");
}

/// Negative proof: the same run, ended, no longer blocks the close.
#[test]
fn negative_proof_an_ended_run_does_not_block_the_close() {
    let home = tempdir("ended-run");
    door_with_runs(&home, "wren", "200", r#"{"data":[{"name":"wren-run-1","runOf":"session-wren-s-1","runEndedAt":"2026-11-10T11:00:00Z"}]}"#);
    let (_, out) = sweep(&home);
    assert!(read(home.join("calls.log")).contains("PATCH http://door.test/v1/identity/sessions/wren-s-1"), "{out}");
}

/// Fail closed: an unreadable runs listing closes nothing and counts as a failure.
#[test]
fn an_unreadable_runs_listing_closes_nothing() {
    let home = tempdir("runs-down");
    door_with_runs(&home, "wren", "200", "not json");
    let (code, out) = sweep(&home);
    assert_eq!(code, 1, "{out}");
    assert!(read(home.join("calls.log")).is_empty(), "nothing closed blind");
    assert!(out.contains("closing nothing this pass"), "{out}");
}
