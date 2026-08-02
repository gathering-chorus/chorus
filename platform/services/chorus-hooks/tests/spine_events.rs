//! #1846 + #1847 — Spine event tests for context cache failures and nudge acknowledgment
//!
//! #1846: session.context.error emitted when cache is empty/failed at boot
//! #1847: nudge.acknowledged emitted when role processes a nudge
//!
//! #2614: tests in this file mutate real role-state paths
//! (`/tmp/session-context-kade.md`, `/tmp/voice-inbox/kade/...`) — same paths the
//! live daemon reads. Running them on a developer machine while kade is in a
//! session can wipe kade's context. Gated behind `RUN_INTEGRATION=1`; pre-commit
//! and default `cargo test` runs skip them.

//! #3721 — these tests were reading the WRONG FILE and racing every live session.
//!
//! 1. WRONG FILE. `chorus_log()` pointed at `<chorus_root>/platform/logs/chorus.log`,
//!    but the shim under test writes to `~/.chorus/chorus.log` — state_paths moved
//!    it there on 2026-05-04 after branch checkouts clobbered the in-tree file
//!    mid-write. Both files exist (chorus-api/mcp still write the in-tree one via
//!    CHORUS_LOG_FILE), so the read succeeded and looked plausible while asserting
//!    against a log the process under test never touches. These could only ever
//!    pass through the `|| stdout.contains(...)` half of their assertion.
//!
//! 2. RACED. Even against the right file, `log_tail(5)` is a fixed-size window on
//!    an append-only log that three live role sessions write to continuously. The
//!    failure that surfaced this had five `session.context.built` events from live
//!    wren/silas/kade sessions occupying the whole window, having pushed out the
//!    event under test. Nothing was wrong with the code; the test just looked at
//!    the wrong five lines.
//!
//! Both are fixed by windowing on the emitting file BY BYTE OFFSET: record the
//! length before running the shim, then read only what was appended after. A
//! concurrent writer can add lines to that window but can no longer push the
//! event out of it, so the assertion is about this run's output and nothing else.
//! (Safe because the spine is strictly append-only and never rotated — Jeff's
//! standing ruling that rotation would truncate reach.)

use std::fs;
use std::process::Command;
use chorus_hooks::shared::state_paths::chorus_log_file;

const SHIM: &str = env!("CARGO_BIN_EXE_chorus-hook-shim");

/// Byte length of the spine log right now — the start of this test's window.
fn log_mark() -> u64 {
    fs::metadata(chorus_log_file()).map(|m| m.len()).unwrap_or(0)
}

/// Everything appended to the spine log since `mark`.
fn log_since(mark: u64) -> String {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = match fs::File::open(chorus_log_file()) {
        Ok(f) => f,
        Err(_) => return String::new(),
    };
    if f.seek(SeekFrom::Start(mark)).is_err() {
        return String::new();
    }
    let mut s = String::new();
    // Lossy on purpose: a concurrent writer can leave a partial UTF-8 line at the
    // tail, and that must not fail the read we are asserting on.
    let mut buf = Vec::new();
    if f.read_to_end(&mut buf).is_ok() {
        s = String::from_utf8_lossy(&buf).into_owned();
    }
    s
}

/// #2614: returns true (and prints a skip line) when RUN_INTEGRATION is unset.
/// Use at the top of any test that mutates real role-state paths or APIs so
/// default `cargo test` runs don't race the live daemon.
fn skip_unless_integration(reason: &str) -> bool {
    if std::env::var("RUN_INTEGRATION").is_err() {
        eprintln!("SKIP: axis-4 — {reason} (set RUN_INTEGRATION=1 to run)");
        return true;
    }
    false
}

// === #1846: context cache behaviour at session-start ===

/// #3721 — this test was right and the CODE was broken. Worth stating plainly,
/// because the first four things this card chased were stale tests and it would
/// have been easy to retire this one too.
///
/// `session_start_emits_context_error_on_empty_cache` (#1846) asserted that
/// booting with an empty cache emits `session.context.error`. It did not, and
/// once the log window was fixed the reason was visible in session.rs: the
/// `lines < 10` alarm was counting `content` at the END of the function, where
/// `content` is the cache PLUS next-session notes, crash recovery, the regen
/// banner, live principles and the Athena tree. Those clear 10 lines on their
/// own, so the alarm could not fire no matter how empty the cache was. Verified
/// against a zero-byte cache: no error, no repair, and the file was still empty
/// ten seconds later — a role would have booted on nothing, silently.
///
/// Fixed in session.rs: the alarm now measures the cache as read, and an empty
/// cache forces a rebuild instead of only an old one doing so. This test asserts
/// both halves of that contract.
#[test]
fn session_start_repairs_or_reports_an_empty_context_cache() {
    if skip_unless_integration("writes /tmp/session-context-kade.md, races live daemon") { return; }
    let cache_file = "/tmp/session-context-kade.md";
    let cache_backup = fs::read_to_string(cache_file).ok();
    let _ = fs::write(cache_file, "");

    let mark = log_mark();
    let output = Command::new(SHIM)
        .args(["session-start", "kade"])
        .output()
        .expect("session-start should run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let log = log_since(mark);

    // The contract: an empty cache is REPAIRED, or the failure is REPORTED.
    // Booting on empty context in silence is the one outcome ruled out — and it
    // was exactly what happened before the session.rs fix.
    let repaired = fs::read_to_string(cache_file).unwrap_or_default();
    let rebuilt = !repaired.trim().is_empty();
    let reported = log.contains("session.context.error") || stdout.contains("context.error");
    assert!(
        rebuilt || reported,
        "empty cache was neither rebuilt nor reported — role would boot on nothing. \
         cache_bytes={}, log: {}, stdout: {}",
        repaired.len(), log, stdout
    );

    match cache_backup {
        Some(content) => { let _ = fs::write(cache_file, content); }
        None => { let _ = fs::remove_file(cache_file); }
    }
}

// === #1847: nudge acknowledgment — RETIRED ===
//
// #3721 — `role_state_drain_emits_nudge_acknowledged` is retired, not fixed.
// #2435 ("atomic cutover") deliberately retired the mechanism it tested: the
// /tmp/voice-inbox queue existed as the fallback channel for inject-failed
// nudges, and under the spine-tick-poller receiver there is no inject failure to
// queue for — the poller retries from the spine on its next 2s tick. role_state.rs
// says so in place: "Queue file + drain + count-payload nudge.acknowledged event
// all retire together." Nothing in src/ emits nudge.acknowledged any more.
//
// It survived that cutover because it was reading tail-5 of a DIFFERENT log file
// (see the module note above), where the string could still be found in unrelated
// history. Reading the correct window immediately showed the truth: role-state
// emits `role.state.changed` and nothing else. Restoring the event to satisfy
// this test would be re-adding a retired mechanism to serve its own test.
