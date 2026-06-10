//! #3319 unit tests — loom-gemba pure core.
//!
//! AC under test (what Jeff sees):
//!   - Every poll's FIRST line is the banner — who's watching whom, since when,
//!     how many new turns. Empty polls included: silence becomes visible.
//!   - Invoking the verb IS the declaration: the role-state args it shells are
//!     `<role> observing gemba=<target>` — no separate step a model can skip.
//!   - Missing stream reads as "rebuilding", never as false quiet.
//!   - The gather mechanics are pulse-gather's, linked — cursor namespace is
//!     loom-gemba's own, so a /gemba poll never moves a raw pulse-gather cursor.

use loom_gemba::{banner, render_poll, role_state_args, spine_extras, CURSOR_NS};
use pulse_gather::{gather_since, parse_observation};

fn obs(ts: &str, tool: &str, digest: &str) -> String {
    format!(
        r#"{{"ts":"{}","role":"silas","tool":"{}","action":"x","digest":"{}"}}"#,
        ts, tool, digest
    )
}

// --- banner: the visibility contract (AC3) ---

#[test]
fn banner_normal_poll_names_watcher_target_cursor_count() {
    let b = banner("wren", "silas", "2026-06-10T08:37:53-0400", 2, true);
    assert_eq!(b, "[gemba] wren→silas | since 2026-06-10T08:37:53-0400 | 2 new turns");
}

#[test]
fn banner_empty_poll_is_visible_quiet_not_silence() {
    let b = banner("wren", "silas", "2026-06-10T08:37:53-0400", 0, true);
    assert_eq!(b, "[gemba] wren→silas | since 2026-06-10T08:37:53-0400 | 0 new turns (quiet)");
}

#[test]
fn banner_cold_start_says_cold_not_a_fake_since() {
    let b = banner("wren", "silas", "", 10, true);
    assert_eq!(b, "[gemba] wren→silas | cold start | last 10 turns");
}

#[test]
fn banner_missing_stream_is_rebuilding_never_quiet() {
    // The #3205 contract one level up: a missing stream is reboot-blindness,
    // not idleness — the banner must say so.
    let b = banner("wren", "silas", "whatever", 0, false);
    assert_eq!(b, "[gemba] wren→silas | observation stream unavailable — rebuilding (not idle)");
}

// --- render_poll: banner is ALWAYS the first line (AC3) ---

#[test]
fn render_poll_banner_first_then_turns() {
    let text = [obs("2026-06-10T08:40:00-0400", "Bash", "cargo test"),
                obs("2026-06-10T08:41:00-0400", "Edit", "lib.rs")].join("\n");
    let fresh: Vec<_> = text.lines().filter_map(parse_observation).collect();
    let out = render_poll("wren", "silas", "2026-06-10T08:39:00-0400", true, &fresh);
    let lines: Vec<&str> = out.lines().collect();
    assert!(lines[0].starts_with("[gemba] wren→silas | since 2026-06-10T08:39:00-0400 | 2 new turns"));
    assert_eq!(lines[1], "08:40:00 Bash — cargo test");
    assert_eq!(lines[2], "08:41:00 Edit — lib.rs");
}

#[test]
fn render_poll_empty_is_banner_only_never_empty_string() {
    // pulse-gather prints nothing on quiet; gemba must NEVER be invisible —
    // the empty poll is exactly the moment Jeff needs to see the watch exists.
    let out = render_poll("wren", "silas", "2026-06-10T08:39:00-0400", true, &[]);
    assert_eq!(out, "[gemba] wren→silas | since 2026-06-10T08:39:00-0400 | 0 new turns (quiet)");
}

#[test]
fn render_poll_missing_stream_is_banner_rebuilding() {
    let out = render_poll("wren", "silas", "", false, &[]);
    assert_eq!(out, "[gemba] wren→silas | observation stream unavailable — rebuilding (not idle)");
}

// --- invoke = declare (AC2): the exact role-state argv, nothing skippable ---

#[test]
fn role_state_args_declare_observing_with_gemba_target() {
    assert_eq!(
        role_state_args("wren", "silas"),
        vec!["wren".to_string(), "observing".to_string(), "gemba=silas".to_string()]
    );
}

// --- spine contract (AC5): gemba.observed with watcher/target/count/status ---

#[test]
fn spine_extras_carry_target_count_status() {
    let e = spine_extras("silas", 2, true);
    assert_eq!(e, vec![("target".to_string(), "silas".to_string()),
                       ("count".to_string(), "2".to_string()),
                       ("status".to_string(), "fresh".to_string())]);
    assert_eq!(spine_extras("silas", 0, true)[2].1, "quiet");
    assert_eq!(spine_extras("silas", 0, false)[2].1, "rebuilding");
}

// --- cursor namespace separation (AC1/AC7): gemba never moves pulse-gather's cursor ---

#[test]
fn cursor_namespace_is_loom_gemba_not_pulse_gather() {
    assert_eq!(CURSOR_NS, "loom-gemba");
}

#[test]
fn gather_mechanics_are_pulse_gathers_linked_not_reimplemented() {
    // Sanity that the linked core behaves: strictly-newer-than-cursor, cursor
    // advances to max ts. (The mechanics' own tests live in pulse-gather; this
    // pins that loom-gemba consumes THAT crate.)
    let text = [obs("2026-06-10T08:40:00-0400", "Bash", "a"),
                obs("2026-06-10T08:41:00-0400", "Edit", "b")].join("\n");
    let r = gather_since(&text, "2026-06-10T08:40:00-0400");
    assert_eq!(r.fresh.len(), 1);
    assert_eq!(r.cursor, "2026-06-10T08:41:00-0400");
}
