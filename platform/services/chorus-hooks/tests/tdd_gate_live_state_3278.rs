//! #3278 Mechanism 2 — the TDD gate must not depend on the lagging transcript.
//!
//! Root cause (found 2026-06-07): tdd_gate scans the session transcript JSONL for a
//! test-file edit, but the transcript flushes ~a turn late. A code edit right after a
//! test edit scans a transcript that doesn't have the test yet → false "no test" block.
//! It blinds the gate for normal write-test-then-write-code TDD in a werk.
//!
//! Fix: the daemon SEES every Edit/Write live (PreToolUse), so it records test-file
//! edits in sync AppState the instant they happen; the gate checks that live state,
//! no transcript dependency. This test proves the live-state path.

use chorus_hooks::AppState;

#[test]
fn test_edit_recorded_live_is_visible_immediately() {
    let state = AppState::new();
    let sid = "sess-3278-abc";

    // No test edit seen yet → gate would have to fall back to the (lagging) transcript.
    assert!(!state.has_test_edit(sid), "should start with no recorded test edit");

    // Daemon sees a test-file Edit this dispatch and records it — no transcript flush needed.
    state.mark_test_edit(sid);

    // The very next dispatch (the code edit) sees it immediately.
    assert!(state.has_test_edit(sid), "a recorded test edit must be visible at once");

    // Scoped per session — another session is unaffected.
    assert!(!state.has_test_edit("other-session"), "test-edit state must be per-session");
}
