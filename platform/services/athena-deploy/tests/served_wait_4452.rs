//! #4452 — a served snapshot taken while the land restarts the door waits for
//! the door instead of failing the athena run (2026-10-12 #4081: "the discovery
//! document lists no collection", so the model never reached the store and was
//! finished by hand). A door that never answers is still a refusal, named.

use athena_deploy::{served_snapshot, wait_until_served};
use std::cell::Cell;

const RESTARTING: &str = r#"{"error":"unknown route","served":[]}"#;
const UP: &str = r#"{"collections":[{"collection":"/v1/schedule/scheduledjobs"},{"collection":"/v1/cards/cards"}]}"#;
const COUNT: &str = "n\n59\n";

#[test]
fn a_door_mid_restart_is_waited_for_and_then_snapshotted() {
    let calls = Cell::new(0);
    let paused = Cell::new(0);
    let got = wait_until_served(
        || {
            calls.set(calls.get() + 1);
            served_snapshot(if calls.get() < 3 { RESTARTING } else { UP }, COUNT)
        },
        24,
        &|_| paused.set(paused.get() + 1),
    )
    .expect("the door came back on the third read");
    assert_eq!(calls.get(), 3);
    assert_eq!(paused.get(), 2, "one pause between reads, none after the good one");
    assert!(got.contains("route /v1/schedule/scheduledjobs") && got.ends_with("domains 59\n"), "{got}");
}

#[test]
fn negative_proof_a_door_that_never_answers_is_still_refused_with_its_reason() {
    let calls = Cell::new(0);
    let err = wait_until_served(
        || {
            calls.set(calls.get() + 1);
            served_snapshot(RESTARTING, COUNT)
        },
        4,
        &|_| {},
    )
    .unwrap_err();
    assert_eq!(calls.get(), 4, "bounded: exactly the tries it was given");
    assert!(err.contains("lists no collection") && err.contains("after 4 tries"), "{err}");
}

#[test]
fn negative_proof_an_unanswered_count_is_waited_on_too_never_read_as_zero() {
    let err = wait_until_served(|| served_snapshot(UP, ""), 2, &|_| {}).unwrap_err();
    assert!(err.contains("did not answer the Domain count"), "{err}");
}

#[test]
fn zero_tries_still_reads_once() {
    let calls = Cell::new(0);
    let _ = wait_until_served(|| { calls.set(calls.get() + 1); Err("x".into()) }, 0, &|_| {});
    assert_eq!(calls.get(), 1);
}
