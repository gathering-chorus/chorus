//! Tests for #2283 — nudge consolidation: single drain point, dead code removed.
//!
//! What Jeff sees: a nudge arrives once, from one path.
//! PostToolUse no longer drains the queue — that was the duplicate delivery source.
//! UserPromptSubmit is the single drain point.
//! Dead flags (--level, --reply-to) are gone from nudge.rs.

/// PostToolUse drain is removed from shim.rs (#2283 AC-3)
use chorus_hooks::shared::state_paths::chorus_root;
#[test]
fn post_tool_use_does_not_drain_queue() {
    let source = std::fs::read_to_string(
        &format!("{}/platform/services/chorus-hooks/src/shim.rs", chorus_root())
    ).expect("shim.rs should exist");

    // The removed code drained the queue on PostToolUse:
    //   if endpoint == "post-tool-use" { if let Some(nudges) = drain_nudge_inbox() { ... } }
    // After #2283, this block must not exist.
    assert!(
        !source.contains("drain_nudge_inbox"),
        "PostToolUse drain (drain_nudge_inbox) must be removed from shim.rs (#2283). \
         Single drain point is UserPromptSubmit only."
    );
}

/// drain_nudge_inbox and is_ancestor helper functions are removed (#2283 AC-2)
#[test]
fn dead_drain_helpers_removed_from_shim() {
    let source = std::fs::read_to_string(
        &format!("{}/platform/services/chorus-hooks/src/shim.rs", chorus_root())
    ).expect("shim.rs should exist");

    assert!(
        !source.contains("fn drain_nudge_inbox"),
        "drain_nudge_inbox function must be removed from shim.rs (#2283)"
    );
    assert!(
        !source.contains("fn detect_drain_role"),
        "detect_drain_role function must be removed from shim.rs (#2283)"
    );
    assert!(
        !source.contains("fn is_ancestor"),
        "is_ancestor function must be removed from shim.rs (#2283)"
    );
}

/// --level flag is dead code and must be removed from nudge.rs (#2283 AC-2)
/// DEC-107: all nudges inject. The passive path was removed from design but not code.
#[test]
fn level_flag_removed_from_nudge() {
    let source = std::fs::read_to_string(
        &format!("{}/platform/services/chorus-hooks/src/nudge.rs", chorus_root())
    ).expect("nudge.rs should exist");

    assert!(
        !source.contains("\"--level\""),
        "--level flag must be removed from nudge.rs — DEC-107 removed the passive path, \
         the flag is dead code (#2283 AC-2)"
    );
}

/// --reply-to flag is removed from nudge.rs (#2283 AC-2)
/// Was a Clearing-specific hack. Clearing uses nudge directly now.
#[test]
fn reply_to_flag_removed_from_nudge() {
    let source = std::fs::read_to_string(
        &format!("{}/platform/services/chorus-hooks/src/nudge.rs", chorus_root())
    ).expect("nudge.rs should exist");

    assert!(
        !source.contains("\"--reply-to\""),
        "--reply-to flag must be removed from nudge.rs — Clearing hack, no longer needed (#2283 AC-2)"
    );
    assert!(
        !source.contains("fn deliver_to_url"),
        "deliver_to_url function must be removed from nudge.rs — only used by --reply-to (#2283 AC-2)"
    );
}

/// detect_sender does not call lsof when DEPLOY_ROLE is unset — falls back to "jeff" (#2283 AC-4)
#[test]
fn detect_sender_no_lsof_fallback() {
    let source = std::fs::read_to_string(
        &format!("{}/platform/services/chorus-hooks/src/nudge.rs", chorus_root())
    ).expect("nudge.rs should exist");

    // lsof must not be called from detect_sender — it takes 20s on this machine.
    // The only acceptable fallback when DEPLOY_ROLE is unset is "jeff".
    // process::get_cwd (which calls lsof) must not appear in the detect_sender function body.
    let detect_start = source.find("fn detect_sender").expect("detect_sender must exist");
    let detect_end = source[detect_start..].find("\nfn ").map(|i| detect_start + i)
        .unwrap_or(source.len());
    let detect_body = &source[detect_start..detect_end];

    assert!(
        !detect_body.contains("get_cwd"),
        "detect_sender must not call get_cwd (lsof) — 20s penalty. \
         Use DEPLOY_ROLE env var; fall back to 'jeff' if unset. (#2283 AC-4)"
    );
}

// queue_fallback_still_present_in_nudge — REMOVED 2026-04-25 (#2477 acp).
// The test asserted queue_message remains in nudge.rs as inject-fail fallback,
// but #2435 deliberately retired the queue_message function (see retirement
// comment block in nudge.rs around line 297-304). The test was asserting a
// function that was correctly removed; this assertion was stale, not the code.
