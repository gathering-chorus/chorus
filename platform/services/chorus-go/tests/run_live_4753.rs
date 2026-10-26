// @test-type: unit — dagu's own status line as given; no dagu, no network.
// @domain: security
// @card: #4753
// @owner: kade
//! #4753 — src/lib.rs `go_run_open`: the go page opens only on a run still
//! waiting at the go. A run cicd-supersede stopped (a newer run of the card
//! exists) is refused before Jeff signs, by name, never "signed, but the run
//! did not take it" after he has.
use chorus_go::*;

#[test]
fn a_run_waiting_at_the_go_opens() {
    assert!(go_run_open("Waiting - 2026-10-26T11:20:47-04:00").is_ok());
}

/// NEGATIVE PROOF: a stopped (superseded) run, a finished run and an unreadable
/// status are each refused, with the reason in Jeff's words.
#[test]
fn a_superseded_finished_or_unknown_run_is_refused_before_signing() {
    let e = go_run_open("Aborted - 2026-10-26T12:20:00-04:00").unwrap_err();
    assert!(e.contains("superseded by a newer run"), "{e}");
    let e = go_run_open("Succeeded - 2026-10-26T12:20:00-04:00").unwrap_err();
    assert!(e.contains("already answered"), "{e}");
    let e = go_run_open("").unwrap_err();
    assert!(e.contains("cannot read"), "{e}");
}
