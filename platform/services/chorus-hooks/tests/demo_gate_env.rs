//! Test: done-gate.sh receives CHORUS_ROOT env var from demo_gate dispatcher.
//!
//! Bug this guards: demo_gate.rs spawned done-gate.sh without passing
//! CHORUS_ROOT. The script's dirname fallback miscalculated the repo root
//! (4 levels up from skills/demo/gates/ overshoots by 1), so it resolved
//! paths wrong. Jeff sees: /acp blocked despite running /demo.
//!
//! #2910 reframe: done-gate.sh's evidence check is now the single
//! `demo:preflight-pass` card-comment grep (not brief-file discovery). So
//! "propagation works" is proven by done-gate.sh *reaching the evidence
//! check* — emitting a card-specific verdict (`reason=no_evidence` or a
//! pass) rather than failing on path resolution. With CHORUS_ROOT empty it
//! fail-opens (cards view can't run → exit 0). The contrast between the two
//! is the propagation test.

use std::process::Command;
use chorus_hooks::shared::state_paths::chorus_root;

#[test]
fn done_gate_reaches_evidence_check_with_chorus_root_set() {
    // With CHORUS_ROOT set, done-gate.sh resolves its paths, runs `cards
    // view`, and reaches the real evidence check. Card #1815 is an old card
    // with no `demo:preflight-pass` comment, so the verdict is exit 1 with
    // reason=no_evidence — which is exactly the proof we want: the script
    // got *past* path resolution into the card-specific check. A propagation
    // failure would look different (path error, card-not-found).
    let script = format!("{}/skills/demo/gates/done-gate.sh", chorus_root());
    let output = Command::new("bash")
        .args([&script, "1815", "kade"])
        .env("CHORUS_ROOT", chorus_root())
        .output()
        .expect("failed to run done-gate.sh");

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    // It must have reached the evidence check — a card-specific verdict, not
    // a path-resolution failure.
    assert!(
        combined.contains("demo.done_gate.") || combined.contains("Demo gate:"),
        "done-gate.sh should reach its evidence check with CHORUS_ROOT set. output: {combined}"
    );
    assert!(
        combined.contains("no_evidence") || combined.contains("no demo evidence")
            || output.status.success(),
        "verdict must be evidence-based (no_evidence) or a pass — not a path error. output: {combined}"
    );
}

#[test]
fn done_gate_allows_with_empty_chorus_root() {
    // done-gate.sh is fail-open: if CHORUS_ROOT is empty, cards view fails,
    // the script hits "Card not found — let other gates handle" and exits 0.
    // Intentional — done-gate should never hard-block on env issues.
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
