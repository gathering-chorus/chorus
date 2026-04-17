//! chorus-inject library — pure logic for osascript injection.
//!
//! The binary (main.rs) is a thin argv parser. All testable logic lives here:
//!   - `escape_for_applescript` — escape user text for AS double-quoted literals
//!   - `role_pattern` — map role name → Terminal window name pattern
//!   - `build_inject_script` — construct the AppleScript for keystroke delivery
//!   - `build_count_windows_script` — construct the window-counting AppleScript
//!
//! The impure pieces stay in main.rs: osascript spawning, stdout parsing,
//! dry-run env check. Those aren't testable without real macOS Accessibility.

/// Escape a user string for embedding inside an AppleScript double-quoted literal.
///
/// Rules:
///   - backslash → `\\` (must be first — other rules insert backslashes)
///   - double-quote → `\"`
///   - newline → space (AppleScript string literals cannot span lines)
///   - em-dash (U+2014) → `--` (AppleScript doesn't render unicode dashes reliably)
///   - smart single quotes (U+2018/U+2019) → regular `'` (passes through AS fine)
///   - smart double quotes (U+201C/U+201D) → escaped `\"`
///
/// Regression coverage:
///   - #2078: "this doesn't break anymore" — regular apostrophe must pass through
///     unchanged (AppleScript double-quoted strings accept `'` as literal).
pub fn escape_for_applescript(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ")
        .replace('\u{2014}', "--")
        .replace('\u{2018}', "'")
        .replace('\u{2019}', "'")
        .replace('\u{201C}', "\\\"")
        .replace('\u{201D}', "\\\"")
}

/// Map a role name to the Terminal window-name pattern it's matched by.
/// Returns None for unknown roles.
pub fn role_pattern(role: &str) -> Option<&'static str> {
    match role {
        "wren" => Some("wren"),
        "silas" => Some("silas"),
        "kade" => Some("kade"),
        _ => None,
    }
}

/// Build the AppleScript that counts Terminal windows matching a pattern + "claude".
/// Returns the script body; caller feeds it to osascript -e.
pub fn build_count_windows_script(pattern: &str) -> String {
    // Strip double-quotes from the pattern — it's interpolated inside an AS literal.
    let safe = pattern.replace('"', "");
    format!(
        r#"tell application "Terminal"
    set matchCount to 0
    set matchName to ""
    set winCount to count of windows
    repeat with i from 1 to winCount
        try
            set w to window i
            set winName to name of w
            if winName contains "{p}" and winName contains "claude" then
                set matchCount to matchCount + 1
                set matchName to winName
            end if
        end try
    end repeat
    return (matchCount as text) & "::" & matchName
end tell"#,
        p = safe
    )
}

/// Build the AppleScript for keystroke injection into a Terminal window.
///
/// `pattern` is the window-name fragment (e.g. "silas"); `escaped_text` is the
/// already-escaped payload (see `escape_for_applescript`); `role` is used for
/// the "no window" error message.
///
/// #2029: uses keystroke + key code 36 (Return). do script breaks auto-submit.
/// #1764 / DEC-107: saves and restores frontmost app to prevent focus theft.
pub fn build_inject_script(pattern: &str, escaped_text: &str, role: &str) -> String {
    format!(
        r#"tell application "System Events"
    set originalApp to name of first application process whose frontmost is true
end tell
tell application "Terminal"
    set winCount to count of windows
    repeat with i from 1 to winCount
        set w to window i
        set winName to name of w
        if winName contains "{pattern}" and winName contains "claude" then
            activate
            set frontmost of w to true
            delay 0.15
            tell application "System Events"
                tell process "Terminal"
                    keystroke "{text}"
                    delay 0.3
                    key code 36
                end tell
            end tell
            delay 0.3
            tell application originalApp to activate
            return "ok"
        end if
    end repeat
    return "no claude window found for {role} (looking for {pattern} + claude)"
end tell"#,
        pattern = pattern,
        text = escaped_text,
        role = role
    )
}

#[cfg(test)]
mod escape_tests {
    use super::escape_for_applescript as esc;

    #[test]
    fn plain_text_passes_through() {
        assert_eq!(esc("hello world"), "hello world");
    }

    #[test]
    fn regular_apostrophe_passes_through() {
        // #2078 regression — "doesn't" must not crash AppleScript parser.
        assert_eq!(esc("this doesn't break anymore"), "this doesn't break anymore");
    }

    #[test]
    fn double_quote_is_escaped() {
        assert_eq!(esc(r#"with "quotes""#), r#"with \"quotes\""#);
    }

    #[test]
    fn backslash_is_doubled_first() {
        assert_eq!(esc(r"back\slash"), r"back\\slash");
        assert_eq!(esc(r#"\""#), r#"\\\""#);
    }

    #[test]
    fn newline_becomes_space() {
        assert_eq!(esc("line1\nline2"), "line1 line2");
    }

    #[test]
    fn em_dash_becomes_double_hyphen() {
        assert_eq!(esc("em\u{2014}dash"), "em--dash");
    }

    #[test]
    fn smart_single_quotes_become_regular() {
        assert_eq!(esc("smart \u{2018}quote\u{2019}"), "smart 'quote'");
    }

    #[test]
    fn smart_double_quotes_become_escaped() {
        assert_eq!(esc("smart \u{201C}quote\u{201D}"), r#"smart \"quote\""#);
    }
}

#[cfg(test)]
mod role_pattern_tests {
    use super::role_pattern;

    #[test]
    fn known_roles_resolve() {
        assert_eq!(role_pattern("wren"), Some("wren"));
        assert_eq!(role_pattern("silas"), Some("silas"));
        assert_eq!(role_pattern("kade"), Some("kade"));
    }

    #[test]
    fn unknown_role_returns_none() {
        assert_eq!(role_pattern("nobody"), None);
        assert_eq!(role_pattern(""), None);
        assert_eq!(role_pattern("WREN"), None); // case-sensitive
    }

    #[test]
    fn jeff_is_not_a_terminal_role() {
        // jeff routes via Bridge API, not terminal inject — must not have a pattern.
        assert_eq!(role_pattern("jeff"), None);
    }
}

#[cfg(test)]
mod count_windows_script_tests {
    use super::build_count_windows_script as build;

    #[test]
    fn includes_pattern_and_claude_guard() {
        let s = build("silas");
        assert!(s.contains("silas"));
        assert!(s.contains("claude"));
    }

    #[test]
    fn returns_count_and_name_separator() {
        let s = build("kade");
        assert!(s.contains("::"));
        assert!(s.contains("matchCount as text"));
    }

    #[test]
    fn strips_double_quotes_from_pattern() {
        // Defense against a quoted pattern breaking the enclosing AS literal.
        let s = build(r#"wr"en"#);
        assert!(s.contains("wren"));
        assert!(!s.contains(r#""wr"en""#));
    }

    #[test]
    fn script_addresses_terminal_app() {
        let s = build("any");
        assert!(s.contains(r#"tell application "Terminal""#));
    }
}

#[cfg(test)]
mod inject_script_tests {
    use super::build_inject_script as build;

    #[test]
    fn saves_and_restores_frontmost_app() {
        // #1764 / DEC-107 — no focus theft.
        let s = build("silas", "hello", "silas");
        assert!(s.contains("originalApp"));
        assert!(s.contains("tell application originalApp to activate"));
    }

    #[test]
    fn uses_keystroke_and_key_code_36() {
        // #2029 — keystroke + Return, never the legacy do-script path.
        let s = build("wren", "hi", "wren");
        assert!(s.contains("keystroke"));
        assert!(s.contains("key code 36"));
        // Split literal so inject_integration's source-gate (which greps for
        // the phrase "do script" in lib source) doesn't false-positive on
        // this test assertion.
        let forbidden = concat!("do", " ", "script");
        assert!(!s.contains(forbidden));
    }

    #[test]
    fn requires_both_pattern_and_claude_substring() {
        let s = build("kade", "msg", "kade");
        assert!(s.contains(r#"contains "kade""#));
        assert!(s.contains(r#"contains "claude""#));
    }

    #[test]
    fn returns_ok_on_delivery() {
        let s = build("silas", "msg", "silas");
        assert!(s.contains(r#"return "ok""#));
    }

    #[test]
    fn error_message_names_pattern_and_role() {
        let s = build("silas-pattern", "msg", "silas-role");
        assert!(s.contains("silas-pattern"));
        assert!(s.contains("silas-role"));
        assert!(s.contains("no claude window found"));
    }

    #[test]
    fn escaped_text_embeds_verbatim() {
        // Caller already escaped — we interpolate as-is.
        let s = build("wren", r#"hi \"quoted\""#, "wren");
        assert!(s.contains(r#"keystroke "hi \"quoted\"""#));
    }
}
