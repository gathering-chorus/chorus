//! #2640 — assert the worktree_contamination_guard hook is retired from the
//! dispatcher hot path (per Jeff direction 2026-05-01: sibling worktrees were
//! retired, the guard's recommended-fix paths point at directories that no
//! longer exist, so the guard is net-negative friction).
//!
//! Module remains on disk for audit history; only the dispatcher hookup is
//! removed.

use std::fs;

#[test]
fn dispatcher_does_not_call_worktree_contamination_guard_check() {
    let main_rs = fs::read_to_string("src/main.rs")
        .expect("read src/main.rs from chorus-hooks crate root");

    // Strip comment lines so a comment mentioning the historical name doesn't
    // false-trigger.
    let active: String = main_rs
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        !active.contains("worktree_contamination_guard::check"),
        "main.rs still calls worktree_contamination_guard::check — \
         hook was retired 2026-05-01 (#2640). Remove the call site."
    );
}
