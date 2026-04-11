//! Test: done-gate.sh receives CHORUS_ROOT env var from demo_gate dispatcher.
//!
//! Bug: demo_gate.rs spawned done-gate.sh without passing CHORUS_ROOT.
//! The script's dirname fallback miscalculated the repo root (4 levels up
//! from skills/demo/gates/ overshoots by 1), so it couldn't find demo briefs
//! even when they exist. Jeff sees: /acp blocked despite running /demo.

use std::process::Command;

/// Resolve CHORUS_ROOT the same way the shim does
fn chorus_root() -> String {
    std::env::var("CHORUS_ROOT")
        .unwrap_or_else(|_| "/Users/jeffbridwell/CascadeProjects/chorus".to_string())
}

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
fn done_gate_fails_with_empty_chorus_root() {
    // Bug: CHORUS_ROOT="" in the shim's environment. Bash ${CHORUS_ROOT:-default}
    // doesn't trigger the fallback because the var IS set (just empty).
    // All paths become relative to "" — brief search breaks, acp blocked.
    let script = format!("{}/skills/demo/gates/done-gate.sh", chorus_root());
    let output = Command::new("bash")
        .args([&script, "1815", "kade"])
        .env("CHORUS_ROOT", "")
        .output()
        .expect("failed to run done-gate.sh");

    assert!(
        !output.status.success(),
        "done-gate.sh should fail with empty CHORUS_ROOT — this proves the bug. \
         The fix: demo_gate.rs must explicitly pass .env(\"CHORUS_ROOT\", chorus_root())."
    );
}
