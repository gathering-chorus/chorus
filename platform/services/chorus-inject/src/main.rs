//! chorus-inject — Stable binary for all osascript operations.
//!
//! This binary exists separately from chorus-hook-shim so that TCC
//! (Accessibility + Automation) permissions are not revoked when the shim
//! is rebuilt. ALL osascript goes through here — one binary, one grant.
//!
//! Usage:
//!   chorus-inject <role> <text>              Inject text into role's Terminal window
//!   chorus-inject --count-windows <pattern>  Count Terminal windows matching "<pattern> + claude"
//!                                            Stdout: <count>::<first-matching-window-name>
//!
//! Exit codes: 0 = success, 1 = failed

use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.len() == 2 && args[0] == "--count-windows" {
        return match count_windows(&args[1]) {
            Ok(s) => { println!("{}", s); ExitCode::SUCCESS }
            Err(e) => { eprintln!("{}", e); ExitCode::from(1) }
        };
    }

    if args.len() < 2 {
        eprintln!("Usage: chorus-inject <role> <text>");
        eprintln!("       chorus-inject --count-windows <pattern>");
        return ExitCode::from(1);
    }

    let role = &args[0];
    let text = args[1..].join(" ");

    match inject(role, &text) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{}", e);
            ExitCode::from(1)
        }
    }
}

/// Count Terminal windows whose name contains both `pattern` and "claude".
/// Returns "<count>::<first-matching-name>" on stdout.
fn count_windows(pattern: &str) -> Result<String, String> {
    let safe = pattern.replace('"', "");
    let script = format!(
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
    );
    let output = Command::new("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| format!("osascript spawn failed: {}", e))?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn role_pattern(role: &str) -> Option<&'static str> {
    match role {
        "wren" => Some("wren"),
        "silas" => Some("silas"),
        "kade" => Some("kade"),
        _ => None,
    }
}

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
fn escape_for_applescript(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ")
        .replace('\u{2014}', "--")
        .replace('\u{2018}', "'")
        .replace('\u{2019}', "'")
        .replace('\u{201C}', "\\\"")
        .replace('\u{201D}', "\\\"")
}

fn inject(role: &str, text: &str) -> Result<(), String> {
    let pattern = role_pattern(role)
        .ok_or_else(|| format!("unknown role: {}", role))?;

    let escaped = escape_for_applescript(text);

    // Test seam: CHORUS_INJECT_DRY_RUN=1 skips the osascript call and prints what
    // would have been injected. Added #2166 so integration tests can exercise the
    // full binary path (argv parse, role validation, escape) without firing real
    // keystrokes into live terminals. Because #2077 routes every osascript call
    // through chorus-inject, this env var also gates the shim's nudge path.
    if std::env::var("CHORUS_INJECT_DRY_RUN").is_ok() {
        println!("DRY-RUN inject role={} pattern={} escaped={}", role, pattern, escaped);
        return Ok(());
    }

    // #2029: revert to keystroke + key code 36 (Return).
    // #2245 switched to "do script" for overnight delivery but broke auto-submit —
    // do script doesn't send a return that Claude Code recognizes.
    // keystroke + key code 36 works. TCC survives rebuilds since #2077 (separate crate).
    // Saves/restores frontmost app to prevent focus theft (#1764, DEC-107).
    let script = format!(
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
        text = escaped,
        role = role
    );

    let output = Command::new("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| format!("osascript spawn failed: {}", e))?;

    let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if result == "ok" {
        Ok(())
    } else {
        Err(format!("{} stderr: {}", result, stderr))
    }
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
        // Inside AS double-quoted strings, `'` is a literal character.
        assert_eq!(esc("this doesn't break anymore"), "this doesn't break anymore");
    }

    #[test]
    fn double_quote_is_escaped() {
        assert_eq!(esc(r#"with "quotes""#), r#"with \"quotes\""#);
    }

    #[test]
    fn backslash_is_doubled_first() {
        // Backslash must be escaped before other rules, otherwise a `\"` in
        // input would become `\\\"` then `\\\\\"` etc.
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
