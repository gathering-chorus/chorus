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

fn inject(role: &str, text: &str) -> Result<(), String> {
    let pattern = role_pattern(role)
        .ok_or_else(|| format!("unknown role: {}", role))?;

    // Escape for AppleScript double-quoted strings
    let escaped = text
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ")
        .replace('\u{2014}', "--")
        .replace('\u{2018}', "'")
        .replace('\u{2019}', "'")
        .replace('\u{201C}', "\\\"")
        .replace('\u{201D}', "\\\"");

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
