//! DEC-107: nudge --force is always on.
//! No passive/queued-only path for role-to-role nudges.
//!
//! Git history: no prior commits for this file (new test).
//! Related: #1898 (silas: acp — nudge respects pulse level) introduced
//! level-based branching that made non-force nudges passive.
//! Log evidence (2026-04-03): wren→silas at 11:37 delivered mode=queued
//! despite wrapper appending --force.

#[test]
fn nudge_force_is_always_true() {
    let source = std::fs::read_to_string(
        "/Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-hooks/src/nudge.rs"
    ).expect("nudge.rs should exist");

    assert!(
        source.contains("let force = true;"),
        "nudge.rs must have `let force = true;` — no passive path allowed (DEC-107)"
    );
    assert!(
        !source.contains("let mut force = false;"),
        "nudge.rs must NOT have `let mut force = false;` — that's the passive path"
    );
}
