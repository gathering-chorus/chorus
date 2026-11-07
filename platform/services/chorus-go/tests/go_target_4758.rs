// @test-type: unit — dagu's status line as given; no dagu, no network.
// @domain: security
// @card: #4758
// @owner: silas
//! #4758 — src/lib.rs `go_target` (read by src/main.rs go_run): a /go link
//! names the card half's run; its go waits in the go half, `go-<run>` on
//! cicd-go. A link made before #4758 still opens on its cicd run.
use chorus_go::*;

#[test]
fn a_link_finds_its_go_in_the_go_half_by_name() {
    let (dag, id) = go_target("034mogqVAn0nLTGzUSbKof", "Waiting - 2026-10-26T18:01:21-04:00");
    assert_eq!((dag, id.as_str()), ("cicd-go", "go-034mogqVAn0nLTGzUSbKof"));
    // a go half that answered or was superseded is still the target: go_run_open
    // refuses it by its own state, never by falling back to the card half
    assert_eq!(go_target("r1", "Aborted - 2026-10-26T18:05:00-04:00").0, "cicd-go");
}

/// NEGATIVE PROOF: with no go-half run (a run from before #4758, still waiting
/// at its own go in cicd), the card half's run is the target, unchanged.
#[test]
fn negative_proof_a_run_from_before_the_split_keeps_its_own_go() {
    let (dag, id) = go_target("034mf00", "");
    assert_eq!((dag, id.as_str()), ("cicd", "034mf00"));
    assert_eq!(go_target("034mf00", "   ").0, "cicd");
}
