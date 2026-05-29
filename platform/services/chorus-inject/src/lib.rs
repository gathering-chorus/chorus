//! chorus-inject library — pure + testable logic for osascript injection.
//!
//! Script builders (pure):
//!   - `escape_for_applescript` — escape user text for AS double-quoted literals
//!   - `role_pattern` — map role name → Terminal window name pattern
//!   - `build_inject_script` — construct the AppleScript for keystroke delivery
//!   - `build_count_windows_script` — construct the window-counting AppleScript
//!
//! Behavior (testable via `OsaRunner`):
//!   - `inject` — role validation, escape, dry-run branch, runner dispatch, ok/err parse
//!   - `count_windows` — runner dispatch, stdout trim
//!   - `dispatch` — argv parsing, usage handling, outcome mapping
//!
//! main.rs is a thin shell over `dispatch` — it wires `std::env::args`,
//! `CHORUS_INJECT_DRY_RUN`, and `RealOsaRunner` to the library entry point.
//! #2167 retired the prior "bin is structurally uncoverable" framing by
//! routing every branch through the runner seam; tests use a FakeRunner.

use std::io;
use std::process::{Command, Output};

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
        .replace(['\u{2018}', '\u{2019}'], "'")
        .replace(['\u{201C}', '\u{201D}'], "\\\"")
        // #3125: non-BMP codepoints (emoji ≥ U+10000, e.g. the role prefix
        // 🪶 U+1FAB6) are mangled by AppleScript's `keystroke` interpreter —
        // they arrive as "aa…" garbage (observed 2026-05-29). The design names
        // this the non-BMP encoding boundary (nudge-service-design.md). Strip
        // them rather than emit garbage: decorative prefixes drop cleanly and
        // BMP message text is untouched. Refusing the whole nudge over a
        // decorative emoji would be worse than dropping the glyph.
        .chars()
        .filter(|c| (*c as u32) <= 0xFFFF)
        .collect()
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
/// #2029: uses keystroke + key code 36 (Return). do script breaks auto-submit.
/// #2277: no app-level activate — set frontmost on the window only, never steal focus.
pub fn build_inject_script(pattern: &str, escaped_text: &str, role: &str) -> String {
    format!(
        r#"tell application "Terminal"
    set winCount to count of windows
    repeat with i from 1 to winCount
        set w to window i
        set winName to name of w
        if winName contains "{pattern}" and winName contains "claude" then
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

/// #3125: build an inject script that targets the Terminal TAB whose `tty`
/// equals `tty` exactly — routing by tty, not by window-title substring.
///
/// Why: title-matching (`build_inject_script`) breaks two ways — a role in a
/// non-Terminal host is invisible, and a stale same-named shell tab
/// ("wren — -zsh") false-matches. The tty is an exact, unique key per session.
///
/// #3128: ALWAYS WAKE. No focus-gate. `System Events keystroke` lands in the
/// FOCUSED app, so to deliver into the matched tab we must make Terminal the
/// frontmost app — we `activate` it on a tty match before typing. This
/// OVERRIDES the #2277 no-focus-steal invariant by explicit Jeff decision:
/// the old gate (refuse-when-not-frontmost) didn't protect focus, it silently
/// dropped the nudge, leaving Jeff to chase roles by hand. A nudge that lands
/// and costs a focus-blip beats a nudge that dies. `activate` fires only inside
/// the tty match, so a no-match scan never steals focus.
pub fn build_inject_by_tty_script(tty: &str, escaped_text: &str) -> String {
    let safe_tty = tty.replace('"', "");
    format!(
        r#"tell application "Terminal"
    set winCount to count of windows
    repeat with i from 1 to winCount
        set w to window i
        repeat with t in tabs of w
            try
                if (tty of t) is "{tty}" then
                    set selected tab of w to t
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
                    return "ok"
                end if
            end try
        end repeat
    end repeat
end tell
return "no claude window found for tty {tty}""#,
        tty = safe_tty,
        text = escaped_text
    )
}

/// #3130 — build the inject script for a session hosted in VS Code's integrated
/// terminal. VS Code is an Electron app ("Code"), NOT Terminal.app: it exposes
/// no tabs/tty to AppleScript, so the Terminal `--tty` match (build_inject_by_tty_script)
/// returns "no claude window found" for a VS Code pseudo-tty — the no-window-found
/// failure. The only osascript delivery is: activate the Code app and keystroke
/// into its FOCUSED window.
///
/// KNOWN LIMITATION (by AppleScript constraint, not laziness): Electron exposes
/// no per-pane/tty handle, so this lands in whichever Code window/pane is focused
/// — it cannot select a specific terminal pane by pseudo-tty. Acceptable for the
/// Chorus model because Jeff lives in the Code window (he IS the focused presence);
/// a role per Code window means the focused pane is the intended target. If that
/// assumption breaks (multiple Claude panes in one Code window, editor focused),
/// the keystroke lands in the focused surface — the cost of Electron having no
/// addressable terminal API.
pub fn build_inject_vscode_script(escaped_text: &str) -> String {
    format!(
        r#"tell application "Code" to activate
delay 0.15
tell application "System Events"
    tell process "Code"
        keystroke "{text}"
        delay 0.3
        key code 36
    end tell
end tell
delay 0.3
return "ok""#,
        text = escaped_text
    )
}

/// Seam for osascript execution. `RealOsaRunner` shells out; tests use a fake.
pub trait OsaRunner {
    fn run(&self, script: &str) -> io::Result<Output>;
}

/// Production runner — invokes `osascript -e <script>`.
pub struct RealOsaRunner;

impl OsaRunner for RealOsaRunner {
    fn run(&self, script: &str) -> io::Result<Output> {
        Command::new("osascript").args(["-e", script]).output()
    }
}

/// Build the count-windows script, run it, trim stdout.
pub fn count_windows<R: OsaRunner>(runner: &R, pattern: &str) -> Result<String, String> {
    let script = build_count_windows_script(pattern);
    let output = runner
        .run(&script)
        .map_err(|e| format!("osascript spawn failed: {}", e))?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Inject `text` into `role`'s Terminal window via `runner`.
///
/// Behavior:
///   - unknown role → Err("unknown role: <role>"), runner not invoked
///   - `dry_run` = true → writes DRY-RUN line to `writer`, returns Ok(()) without invoking runner
///   - runner Ok + stdout "ok" → Ok(())
///   - runner Ok + stdout other → Err("<stdout> stderr: <stderr>")
///   - runner Err → Err("osascript spawn failed: ...")
pub fn inject<R: OsaRunner, W: io::Write>(
    runner: &R,
    writer: &mut W,
    role: &str,
    text: &str,
    dry_run: bool,
) -> Result<(), String> {
    let pattern = role_pattern(role).ok_or_else(|| format!("unknown role: {}", role))?;
    let escaped = escape_for_applescript(text);

    if dry_run {
        writeln!(
            writer,
            "DRY-RUN inject role={} pattern={} escaped={}",
            role, pattern, escaped
        )
        .map_err(|e| format!("write failed: {}", e))?;
        return Ok(());
    }

    let script = build_inject_script(pattern, &escaped, role);
    let output = runner
        .run(&script)
        .map_err(|e| format!("osascript spawn failed: {}", e))?;

    let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if result == "ok" {
        Ok(())
    } else {
        Err(format!("{} stderr: {}", result, stderr))
    }
}

/// #3125: inject `text` into the Terminal tab identified by `tty` (the routing
/// key resolved upstream from the session registry). dry_run writes a DRY-RUN
/// line naming the tty and returns Ok without invoking the runner. Mirrors
/// `inject`'s ok/err parse. This is the additive tty path — the legacy
/// `inject` (name-match) remains the default; callers opt in via `--tty`.
pub fn inject_by_tty<R: OsaRunner, W: io::Write>(
    runner: &R,
    writer: &mut W,
    tty: &str,
    text: &str,
    dry_run: bool,
) -> Result<(), String> {
    let escaped = escape_for_applescript(text);

    if dry_run {
        writeln!(writer, "DRY-RUN inject-by-tty tty={} escaped={}", tty, escaped)
            .map_err(|e| format!("write failed: {}", e))?;
        return Ok(());
    }

    let script = build_inject_by_tty_script(tty, &escaped);
    let output = runner
        .run(&script)
        .map_err(|e| format!("osascript spawn failed: {}", e))?;

    let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if result == "ok" {
        Ok(())
    } else {
        Err(format!("{} stderr: {}", result, stderr))
    }
}

/// #3130: inject `text` into the focused VS Code window. dry_run writes a
/// DRY-RUN line naming the vscode path. Mirrors `inject_by_tty`'s ok/err parse.
pub fn inject_vscode<R: OsaRunner, W: io::Write>(
    runner: &R,
    writer: &mut W,
    text: &str,
    dry_run: bool,
) -> Result<(), String> {
    let escaped = escape_for_applescript(text);

    if dry_run {
        writeln!(writer, "DRY-RUN inject-vscode (Code app, focused window) escaped={}", escaped)
            .map_err(|e| format!("write failed: {}", e))?;
        return Ok(());
    }

    let script = build_inject_vscode_script(&escaped);
    let output = runner
        .run(&script)
        .map_err(|e| format!("osascript spawn failed: {}", e))?;

    let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if result == "ok" {
        Ok(())
    } else {
        Err(format!("{} stderr: {}", result, stderr))
    }
}

/// Outcome of `dispatch` — main.rs maps this to ExitCode + stdout/stderr.
#[derive(Debug, PartialEq, Eq)]
pub enum Dispatch {
    /// Print the string to stdout, exit 0.
    PrintOut(String),
    /// Exit 0 (inject already wrote to `writer` if dry-run).
    Ok,
    /// Print the string to stderr, exit 1.
    Err(String),
}

/// Single entry point: parse `args` (post-argv[0]), run the right operation.
///
/// `writer` captures inject's dry-run output. In production this is
/// `io::stdout().lock()`; tests use a `Vec<u8>`.
pub fn dispatch<R: OsaRunner, W: io::Write>(
    runner: &R,
    writer: &mut W,
    args: &[String],
    dry_run: bool,
) -> Dispatch {
    if args.len() == 2 && args[0] == "--count-windows" {
        return match count_windows(runner, &args[1]) {
            Ok(s) => Dispatch::PrintOut(s),
            Err(e) => Dispatch::Err(e),
        };
    }

    // #3130: `--vscode <text...>` routes to the Code-app focused-window inject
    // (VS Code's pseudo-tty isn't a Terminal tab, so --tty can't reach it).
    if args.len() >= 2 && args[0] == "--vscode" {
        let text = args[1..].join(" ");
        return match inject_vscode(runner, writer, &text, dry_run) {
            Ok(()) => Dispatch::Ok,
            Err(e) => Dispatch::Err(e),
        };
    }

    // #3125: `--tty <tty> <text...>` routes by tty (exact tab match + focus
    // gate). Additive — the legacy `<role> <text...>` form below is unchanged.
    if args.len() >= 2 && args[0] == "--tty" {
        let tty = &args[1];
        let text = args[2..].join(" ");
        return match inject_by_tty(runner, writer, tty, &text, dry_run) {
            Ok(()) => Dispatch::Ok,
            Err(e) => Dispatch::Err(e),
        };
    }

    if args.len() < 2 {
        return Dispatch::Err(
            "Usage: chorus-inject <role> <text>\n       chorus-inject --count-windows <pattern>"
                .to_string(),
        );
    }

    let role = &args[0];
    let text = args[1..].join(" ");

    match inject(runner, writer, role, &text, dry_run) {
        Ok(()) => Dispatch::Ok,
        Err(e) => Dispatch::Err(e),
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
    fn no_app_level_activate() {
        // #2277 — never activate Terminal app-level; set frontmost on window only.
        let s = build("silas", "hello", "silas");
        assert!(!s.contains("activate"));
        assert!(s.contains("set frontmost of w to true"));
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
        let s = build("wren", r#"hi \"quoted\""#, "wren");
        assert!(s.contains(r#"keystroke "hi \"quoted\"""#));
    }
}

#[cfg(test)]
mod inject_by_tty_script_tests {
    use super::build_inject_by_tty_script as build;

    #[test]
    fn no_focus_gate() {
        // #3128 — always wake: the focus-gate refusal is gone. The script must
        // NOT bail out when another app is frontmost, and must not emit the
        // focus-gate-miss sentinel.
        let s = build("ttys003", "hello");
        assert!(!s.contains("focus-gate-miss"));
        assert!(!s.contains("frontApp"));
    }

    #[test]
    fn activates_terminal_to_land_keystroke() {
        // #3128 — keystroke lands in the frontmost app, so we must activate
        // Terminal on a tty match to deliver reliably (overrides #2277 by
        // explicit decision).
        let s = build("ttys003", "hello");
        assert!(s.contains("activate"));
    }

    #[test]
    fn still_routes_by_exact_tty() {
        let s = build("ttys042", "msg");
        assert!(s.contains(r#"(tty of t) is "ttys042""#));
        assert!(s.contains("no claude window found for tty ttys042"));
    }

    #[test]
    fn uses_keystroke_and_key_code_36() {
        let s = build("ttys003", "hi");
        assert!(s.contains("keystroke"));
        assert!(s.contains("key code 36"));
    }

    #[test]
    fn returns_ok_on_delivery() {
        let s = build("ttys003", "msg");
        assert!(s.contains(r#"return "ok""#));
    }
}
