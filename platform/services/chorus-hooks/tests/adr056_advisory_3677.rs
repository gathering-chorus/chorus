// @test-type: unit
//! #3677 — the ADR-056 hollow-module resolution, pinned.
//!
//! Three modules documented themselves as blocking and returned allow
//! (ADR-056: "Three modules document themselves as blocking and return
//! allow"). The honest resolution is ADVISORY, stated in each module's own
//! doc-comment — and THIS suite is the fixture for the downgrade: it pins the
//! advisory contract so a future edit that makes one of them deny must flip a
//! test here WITH a real negative fixture, never silently.
//!
//! NEGATIVE PROOF (#3734): each assertion fails if the module starts denying —
//! the two states (advisory vs blocking) are distinguishable by this suite.

use chorus_hooks::{pair_gate, quality_gate, AppState, HookInput};

fn input(tool: &str) -> HookInput {
    HookInput {
        tool_name: Some(tool.to_string()),
        ..Default::default()
    }
}

#[test]
fn pair_gate_is_advisory_never_denies() {
    let state = AppState::new();
    let r = pair_gate::check(&input("Edit"), &state);
    assert!(
        r.exit_code == 0 && r.stdout.as_deref().map(|j| !j.contains("deny")).unwrap_or(true),
        "pair_gate documented advisory (#3677); a deny here requires flipping this pin with a real fixture"
    );
}

#[test]
fn quality_gate_post_edit_is_advisory_never_denies() {
    let r = quality_gate::post_edit_check(&input("Edit"));
    assert!(
        r.exit_code == 0 && r.stdout.as_deref().map(|j| !j.contains("deny")).unwrap_or(true),
        "quality_gate documented advisory (#3677); the blocking demo review lives in werk-demo preflight (#3443)"
    );
}
