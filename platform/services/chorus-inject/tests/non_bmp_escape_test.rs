//! #3125 — non-BMP escaping (the 🪶 → "aa" mangle observed 2026-05-29).
//! AppleScript's `keystroke` interpreter cannot render codepoints ≥ U+10000;
//! they arrive as garbage. escape_for_applescript must strip them so a
//! decorative role prefix drops cleanly instead of corrupting the message.

use chorus_inject::escape_for_applescript;

#[test]
fn strips_non_bmp_emoji_prefix() {
    // 🪶 is U+1FAB6 (non-BMP). It must not survive into the keystroke text.
    let out = escape_for_applescript("🪶 Relay to silas");
    assert!(!out.contains('\u{1FAB6}'), "non-BMP emoji must be stripped, got: {out:?}");
    assert_eq!(out, " Relay to silas");
}

#[test]
fn strips_multiple_non_bmp_but_keeps_bmp() {
    // Mixed: non-BMP emoji removed, BMP content (incl. em-dash rule) preserved.
    let out = escape_for_applescript("⚠\u{FE0F}🎬 demo \u{2014} ready");
    // U+26A0 (⚠) is BMP and stays; U+FE0F is BMP and stays; 🎬 (U+1F3AC) drops.
    assert!(!out.contains('\u{1F3AC}'));
    assert!(out.contains('\u{26A0}'));
    assert!(out.contains("--")); // em-dash rule still applies
}

#[test]
fn bmp_text_and_existing_escapes_unchanged() {
    // Regression guard: the fix must not alter existing BMP behavior.
    assert_eq!(escape_for_applescript("plain text"), "plain text");
    assert_eq!(escape_for_applescript("this doesn't break"), "this doesn't break");
    assert_eq!(escape_for_applescript(r#"with "quotes""#), r#"with \"quotes\""#);
}
