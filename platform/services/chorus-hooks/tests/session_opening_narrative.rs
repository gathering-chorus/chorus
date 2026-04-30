//! #1902 — Reflective session opening
//! Boot template should guide narrative synthesis, not dashboard readout.
//! AC:
//!   1. Opening synthesizes activity log and Chorus search into narrative — pace, direction, friction
//!   2. Stale briefs and health issues framed as process signals with a position
//!   3. Session-start template updated if structural change needed

use std::fs;

const SHIM: &str = env!("CARGO_BIN_EXE_chorus-hook-shim");

/// #2614: tests in this file run `chorus-hook-shim context-cache <role>` which
/// writes /tmp/session-context-<role>.md — the same file the live role session
/// reads at boot. On a developer machine this races whoever's session is open.
/// Gated behind `RUN_INTEGRATION=1`.
fn skip_unless_integration(reason: &str) -> bool {
    if std::env::var("RUN_INTEGRATION").is_err() {
        eprintln!("SKIP: axis-4 — {reason} (set RUN_INTEGRATION=1 to run)");
        return true;
    }
    false
}

/// Build fresh context cache for silas — required setup for every test reading the file.
/// Each test calls this (idempotent) so ordering doesn't matter.
fn build_silas_cache() {
    let output = std::process::Command::new(SHIM)
        .args(["context-cache", "silas"])
        .output()
        .expect("context-cache should execute");
    assert!(output.status.success(), "context-cache should succeed");
}

/// Boot instruction explicitly discourages dashboard readout patterns
#[test]
fn boot_instruction_discourages_readout() {
    if skip_unless_integration("writes /tmp/session-context-silas.md via context-cache, races live silas session") { return; }
    build_silas_cache();
    let content = fs::read_to_string("/tmp/session-context-silas.md")
        .expect("session-context-silas.md should exist");

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
    if skip_unless_integration("writes /tmp/session-context-silas.md via context-cache, races live silas session") { return; }
    build_silas_cache();
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
    if skip_unless_integration("writes /tmp/session-context-silas.md via context-cache, races live silas session") { return; }
    build_silas_cache();
    let content = fs::read_to_string("/tmp/session-context-silas.md")
        .expect("session-context-silas.md should exist");

    // AC3: template structural update — the old header primed mechanical behavior
    assert!(
        !content.contains("## Boot: Synthesize Before Speaking"),
        "boot header should not be the old mechanical 'Synthesize Before Speaking' — template needs structural update"
    );
}

/// #2114 — Boot prompt includes 5-beat shape (thesis, reframe, friction-with-position, flinch, single-question close)
#[test]
fn boot_includes_five_beat_shape() {
    if skip_unless_integration("writes /tmp/session-context-silas.md via context-cache, races live silas session") { return; }
    build_silas_cache();
    let content = fs::read_to_string("/tmp/session-context-silas.md")
        .expect("session-context-silas.md should exist");
    let boot = extract_boot_section(&content);

    for beat in ["thesis", "reframe", "flinch"] {
        assert!(
            boot.to_lowercase().contains(beat),
            "boot must name the '{}' beat — got: {}", beat, boot
        );
    }
    assert!(
        boot.to_lowercase().contains("one question") || boot.to_lowercase().contains("single question") || boot.to_lowercase().contains("one-question"),
        "boot must describe the single-question close — got: {}", boot
    );
}

/// #2114 — Boot prompt includes an inline example opening so agent has concrete pattern
#[test]
fn boot_includes_inline_example() {
    if skip_unless_integration("writes /tmp/session-context-silas.md via context-cache, races live silas session") { return; }
    build_silas_cache();
    let content = fs::read_to_string("/tmp/session-context-silas.md")
        .expect("session-context-silas.md should exist");
    let boot = extract_boot_section(&content);

    assert!(
        boot.to_lowercase().contains("example"),
        "boot must include a labeled example opening — got: {}", boot
    );
    assert!(
        boot.contains("Example") || boot.contains("example opening") || boot.contains("example:"),
        "boot must present a concrete example opening — got: {}", boot
    );
}

/// #2114 — All three roles render the new prompt
#[test]
fn all_three_roles_render_shape_and_example() {
    if skip_unless_integration("writes /tmp/session-context-{wren,silas,kade}.md via context-cache, races all live sessions") { return; }
    for role in ["wren", "silas", "kade"] {
        let output = std::process::Command::new(SHIM)
            .args(["context-cache", role])
            .output()
            .expect("context-cache should execute");
        assert!(output.status.success(), "context-cache should succeed for {}", role);

        let path = format!("/tmp/session-context-{}.md", role);
        let content = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("{} should exist", path));
        let boot = extract_boot_section(&content);

        assert!(
            boot.to_lowercase().contains("thesis"),
            "{}: boot must include thesis beat", role
        );
        assert!(
            boot.to_lowercase().contains("example"),
            "{}: boot must include inline example", role
        );
    }
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
