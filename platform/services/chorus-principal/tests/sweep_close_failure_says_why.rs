// @test-type: unit — a refused close names its http code and why on the spine, against a fake door
// @domain: identity
//! #4944 AC2 — the 11-09 sweep failures were eprintln-only: "not closed (HTTP 422)",
//! no reason and no event. A refused close now emits session.row.failed with the
//! http code and the door's why.
mod common_4944;
use common_4944::*;

#[test]
fn a_door_answering_422_makes_session_row_failed_fire_with_http_422_and_why() {
    let home = tempdir("refused");
    door(&home, "wren", "422");
    let (code, out) = sweep(&home);
    assert_eq!(code, 1, "a failed close fails the sweep: {out}");
    let spine = read(home.join("spine.log"));
    let ev = spine.lines().find(|l| l.starts_with("session.row.failed")).unwrap_or_else(|| panic!("no session.row.failed: {spine}"));
    assert!(ev.contains("http=422"), "{ev}");
    assert!(ev.contains("row=wren-s-1"), "{ev}");
    assert!(ev.contains("why=") && !ev.contains("why= ") && !ev.ends_with("why="), "the door's reason is named: {ev}");
}

/// Negative proof: the same door answering 200 fires no session.row.failed, so the
/// test above can tell a refused close from a closed one.
#[test]
fn negative_proof_a_door_answering_200_fires_no_failure() {
    let home = tempdir("accepted");
    door(&home, "wren", "200");
    let (code, out) = sweep(&home);
    assert_eq!(code, 0, "{out}");
    assert!(!read(home.join("spine.log")).contains("session.row.failed"));
}
