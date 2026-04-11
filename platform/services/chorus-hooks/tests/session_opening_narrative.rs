//! #1902 — Reflective session opening
//! Boot template should guide narrative synthesis, not dashboard readout.
//! AC:
//!   1. Opening synthesizes activity log and Chorus search into narrative — pace, direction, friction
//!   2. Stale briefs and health issues framed as process signals with a position
//!   3. Session-start template updated if structural change needed

use std::fs;

const SHIM: &str = "/Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-hooks/target/release/chorus-hook-shim";

/// Boot instruction guides narrative synthesis with arc, pace, friction framing
#[test]
fn boot_instruction_guides_narrative_synthesis() {
    // Build fresh context cache
    let output = std::process::Command::new(SHIM)
        .args(["context-cache", "silas"])
        .output()
        .expect("context-cache should execute");
    assert!(output.status.success(), "context-cache should succeed");

    let content = fs::read_to_string("/tmp/session-context-silas.md")
        .expect("session-context-silas.md should exist");

    // AC1: must guide toward arc, pace, friction — not just "synthesize"
    assert!(
        content.contains("Arc") && content.contains("Pace") && content.contains("Friction"),
        "boot instruction must frame synthesis around arc, pace, and friction — got boot section: {}",
        extract_boot_section(&content)
    );
}

/// Boot instruction explicitly discourages dashboard readout patterns
#[test]
fn boot_instruction_discourages_readout() {
    let content = fs::read_to_string("/tmp/session-context-silas.md")
        .expect("session-context-silas.md should exist (run narrative_synthesis test first)");

    // AC1+AC3: template must explicitly say no card lists, no metric bullet points
    let boot = extract_boot_section(&content);
    assert!(
        boot.contains("No card lists") || boot.contains("no card lists"),
        "boot instruction must explicitly discourage card list readout — got: {}",
        boot
    );
    assert!(
        boot.contains("not a status report") || boot.contains("No health metric") || boot.contains("no health metric"),
        "boot instruction must discourage status report / health metric readout — got: {}",
        boot
    );
}

/// Boot instruction requires positions on problems, not bare facts
#[test]
fn boot_instruction_requires_positions() {
    let content = fs::read_to_string("/tmp/session-context-silas.md")
        .expect("session-context-silas.md should exist");

    // AC2: stale briefs and health issues must be framed as process signals with a position
    let boot = extract_boot_section(&content);
    assert!(
        boot.contains("position") || boot.contains("process signal"),
        "boot instruction must require positions on problems or frame issues as process signals — got: {}",
        boot
    );
}

/// Boot section header changed from "Synthesize Before Speaking" to something less mechanical
#[test]
fn boot_header_not_mechanical() {
    let content = fs::read_to_string("/tmp/session-context-silas.md")
        .expect("session-context-silas.md should exist");

    // AC3: template structural update — the old header primed mechanical behavior
    assert!(
        !content.contains("## Boot: Synthesize Before Speaking"),
        "boot header should not be the old mechanical 'Synthesize Before Speaking' — template needs structural update"
    );
}

/// Extract the boot instruction section from session context
fn extract_boot_section(content: &str) -> String {
    let mut in_boot = false;
    let mut lines = Vec::new();
    for line in content.lines() {
        if line.starts_with("## Boot:") {
            in_boot = true;
        } else if in_boot && line.starts_with("## ") {
            break;
        }
        if in_boot {
            lines.push(line);
        }
    }
    lines.join("\n")
}
