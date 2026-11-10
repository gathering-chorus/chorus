// @test-type: unit — a refused close names its http code and why on the spine, against a fake door
// @domain: identity
//! #4944 AC2 — the 11-09 sweep failures were eprintln-only: "not closed (HTTP 422)",
//! no reason and no event. A refused write now emits session.row.failed with the
//! http code and the door's why. #4948: the sweep's write is a crashed run's end
//! (it no longer closes sessions), so the refused write is that PATCH.
mod common_4944;
use common_4944::*;

/// wren's open run, started long ago, and a registry whose only wren pid is dead.
fn crashed(home: &std::path::Path, code: &str) {
    door_with_runs(home, "wren", code, r#"{"data":[{"name":"wren-run-1","ownedBy":"principal-wren","runOf":"session-wren-s-1","startedAt":"2026-11-09T16:18:51Z","runEndedAt":""}]}"#);
    let d = home.join(".chorus/sessions");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("wren-99999999.json"), r#"{"role":"wren","pid":99999999,"tty":"/dev/ttys001","host":"tmux"}"#).unwrap();
}

#[test]
fn a_door_answering_422_makes_session_row_failed_fire_with_http_422_and_why() {
    let home = tempdir("refused");
    crashed(&home, "422");
    let (code, out) = sweep(&home);
    assert_eq!(code, 1, "a refused write fails the sweep: {out}");
    let spine = read(home.join("spine.log"));
    let ev = spine.lines().find(|l| l.starts_with("session.row.failed")).unwrap_or_else(|| panic!("no session.row.failed: {spine}"));
    assert!(ev.contains("http=422"), "{ev}");
    assert!(ev.contains("row=wren-run-1"), "{ev}");
    assert!(ev.contains("why=") && !ev.contains("why= ") && !ev.ends_with("why="), "the door's reason is named: {ev}");
}

/// Negative proof: the same door answering 200 fires no session.row.failed, so the
/// test above can tell a refused write from an accepted one.
#[test]
fn negative_proof_a_door_answering_200_fires_no_failure() {
    let home = tempdir("accepted");
    crashed(&home, "200");
    let (code, out) = sweep(&home);
    assert_eq!(code, 0, "{out}");
    assert!(!read(home.join("spine.log")).contains("session.row.failed"));
}
