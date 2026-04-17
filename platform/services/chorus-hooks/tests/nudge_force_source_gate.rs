//! DEC-107 source gate: nudge --force is always on at the SOURCE level.
//!
//! **This is not a behavior test.** It is a build-time lint that reads
//! nudge.rs and asserts a literal string is present. It runs under
//! cargo test because Rust makes custom lints harder than custom tests,
//! but it does NOT exercise the runtime. If someone changes the variable
//! name without changing the behavior, this fails; if someone changes
//! the behavior without changing the variable name, this passes. Treat
//! it as clippy-with-more-ceremony.
//!
//! Renamed 2026-04-17 per #2155 to make the role honest. See also
//! loom-principles:quality-at-source.
//!
//! Related: #1898 introduced level-based branching that made non-force
//! nudges passive. Log evidence (2026-04-03): wren→silas at 11:37
//! delivered mode=queued despite wrapper appending --force.

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
