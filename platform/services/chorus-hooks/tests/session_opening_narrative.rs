//! @test-type: integration — reads /tmp/session-context-<role>.md, written at
//! session start by the LIVE hook (commands/session.rs, csc_guard.rs). Those
//! files are runtime artifacts: nothing in the repo generates them (git
//! ls-files: 0 matches), so on a CI runner they do not exist and every case
//! panics "<role> should exist" or reads an empty boot section. It passes
//! locally only because this machine has three live sessions that wrote them.
//!
//! #3721 — this ran for the first time when the socket-path fix let the job
//! reach the test step. Asserting on live session output is a real check —
//! it is how we know the boot template still guides narrative synthesis — but
//! it belongs where sessions exist, which is the local nightly, not a runner
//! with no roles booted.
//!
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

// #3721 — RETIRED (content call: Wren, 2026-08-01), replaced by
// boot_requires_transcript_reconstruction below.
//
// This asserted the boot literally contains "No card lists" and "not a status
// report" — anti-patterns spelled out as prohibitions. The 2026-07-29 rewrite
// dropped that framing on purpose: it states the POSITIVE contract instead
// ("reconstruct where you and Jeff actually left off — from primary source"),
// on the same reasoning that retired thesis-first. Asserting the prohibitions
// would pin wording the rewrite deliberately removed.
//
// Retired, not loosened: the discipline it guarded — the opening is a
// reconstruction, not a dashboard readout — is now guarded positively by
// boot_requires_transcript_reconstruction, which reads the SOURCE rather than
// the rendered /tmp copy this one depended on.


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

// ── #3721 — RETIRED + REPLACED (content call: Wren, 2026-08-01) ─────────────
//
// RETIRED: boot_includes_five_beat_shape, boot_includes_inline_example,
// boot_instruction_discourages_readout, and the "thesis" beat assert.
//
// WHY, so nobody reads this as caving to a red: #1902/#2114 guarded a
// five-beat, thesis-first opening WITH a worked example. That opening was
// deliberately REPLACED on 2026-07-29. The live boot now says, verbatim,
// "Don't manufacture a thesis to sound synthesized", and its Rules record that
// it "replaced a thesis-first opening precisely because that one rewarded
// sounding synthesized over being accurate, and produced fabricated openings."
// Restoring an example and five beats to turn these green would re-introduce
// the exact behaviour that rewrite removed. The shape is superseded; the
// discipline is not.
//
// The sharpest one was not even failing. `boot.contains("thesis")` PASSED —
// because the boot contains "not a thesis" and "manufacture a thesis". A
// substring assert cannot tell "include a thesis beat" from "never manufacture
// a thesis", so it read green while the content meant the OPPOSITE of what it
// guarded. A check that cannot distinguish the two states it exists to separate
// is not a weak check; it is not a check.
//
// WHERE THESE READ FROM: the boot text is built as inline string literals in
// commands/context_cache.rs — NOT a designing/claudemd fragment. That file is
// the source of truth and it is committed, so these are hermetic source guards:
// no live session, no /tmp, no RUN_INTEGRATION. The retired versions read the
// RENDERED /tmp copy, which is exactly why they were skipped locally and
// unreachable on CI — two layers of green over a file nobody opened.

fn boot_source() -> String {
    let src = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("src/commands/context_cache.rs");
    fs::read_to_string(&src)
        .unwrap_or_else(|e| panic!("boot source unreadable at {}: {e}", src.display()))
}

/// The opening must be rebuilt from primary source, not from memory.
#[test]
fn boot_requires_transcript_reconstruction() {
    let s = boot_source();
    assert!(s.contains("transcript"), "boot must direct reconstruction from the transcript");
    assert!(s.contains("pulse-latest.json"), "boot must name the durable pulse source");
}

/// The rule that replaced thesis-first — the single most load-bearing line.
#[test]
fn boot_requires_verify_before_asserting() {
    let s = boot_source();
    assert!(
        s.contains("Verify before asserting"),
        "boot must carry the verify-before-asserting rule that replaced thesis-first"
    );
}

/// A position is optional and earned, never manufactured.
#[test]
fn boot_makes_the_position_optional() {
    let s = boot_source();
    assert!(s.contains("only if earned"), "boot must make the position optional and earned");
}

/// The close is one question, not a menu.
#[test]
fn boot_requires_one_question_close() {
    let s = boot_source();
    assert!(s.contains("One-question close"), "boot must require the one-question close");
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
