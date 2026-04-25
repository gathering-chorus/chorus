//! DEC-107 source gate: nudge force path is unconditional at the SOURCE level.
//!
//! **This is not a behavior test.** It is a build-time lint that reads
//! nudge.rs and asserts DEC-107's invariant holds at the code shape:
//! no `force` variable, no conditional branching on a force flag, no
//! mutable toggle that could make a path passive. Persist AND deliver
//! always fire; there is nothing for a future edit to flip.
//!
//! Prior shape (pre-#2283): `let force = true;` as a hardcoded binding.
//! Current shape (post-#2283): the variable is gone entirely — the
//! `--force` CLI flag is accepted-and-ignored, with both persist and
//! deliver called unconditionally. The current shape is strictly
//! stronger than the prior one: no binding means nothing to toggle.
//!
//! Renamed 2026-04-17 per #2155 to make the role honest. See also
//! loom-principles:quality-at-source.
//!
//! Related: #1898 introduced level-based branching that made non-force
//! nudges passive. Log evidence (2026-04-03): wren→silas at 11:37
//! delivered mode=queued despite wrapper appending --force.

use chorus_hooks::shared::state_paths::chorus_root;
#[test]
fn nudge_has_no_passive_force_path() {
    let source = std::fs::read_to_string(
        &format!("{}/platform/services/chorus-hooks/src/nudge.rs", chorus_root())
    ).expect("nudge.rs should exist");

    assert!(
        !source.contains("let mut force = false;"),
        "nudge.rs must NOT have `let mut force = false;` — that's the passive path (DEC-107)"
    );
    assert!(
        !source.contains("let force = false;"),
        "nudge.rs must NOT have `let force = false;` — that's the passive path (DEC-107)"
    );
    assert!(
        !source.contains("if force {") && !source.contains("if force\n") && !source.contains("if !force"),
        "nudge.rs must NOT branch on a `force` variable — DEC-107 requires both paths fire unconditionally"
    );
}
