//! Test: done-gate.sh receives CHORUS_ROOT env var from demo_gate dispatcher.
//!
//! Bug: demo_gate.rs spawned done-gate.sh without passing CHORUS_ROOT.
//! The script's dirname fallback miscalculated the repo root (4 levels up
//! from skills/demo/gates/ overshoots by 1), so it couldn't find demo briefs
//! even when they exist. Jeff sees: /acp blocked despite running /demo.

use std::process::Command;
use chorus_hooks::shared::state_paths::chorus_root;

/// Resolve CHORUS_ROOT the same way the shim does

#[test]
fn done_gate_finds_brief_when_chorus_root_set() {
    // Run done-gate.sh WITH CHORUS_ROOT set — should find the demo brief for #1815
    let script = format!("{}/skills/demo/gates/done-gate.sh", chorus_root());
    let output = Command::new("bash")
        .args([&script, "1815", "kade"])
        .env("CHORUS_ROOT", chorus_root())
        .output()
        .expect("failed to run done-gate.sh");

    assert!(
        output.status.success(),
        "done-gate.sh should pass with CHORUS_ROOT set. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn done_gate_allows_with_empty_chorus_root() {
    // done-gate.sh is fail-open: if CHORUS_ROOT is empty, cards view fails,
    // script hits "Card not found — let other gates handle" and exits 0.
    // This is intentional — done-gate should never hard-block on env issues.
    let script = format!("{}/skills/demo/gates/done-gate.sh", chorus_root());
    let output = Command::new("bash")
        .args([&script, "1815", "kade"])
        .env("CHORUS_ROOT", "")
        .output()
        .expect("failed to run done-gate.sh");

    assert!(
        output.status.success(),
        "done-gate.sh should allow (exit 0) with empty CHORUS_ROOT — fail-open design. \
         stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
