//! chorus-inject — Stable binary for osascript keystroke injection.
//!
//! This binary exists separately from chorus-hook-shim so that TCC
//! (Accessibility) permissions are not revoked when the shim is rebuilt.
//! It rarely changes — only the injection logic lives here.
//!
//! Usage: chorus-inject <role> <text>
//!   role: wren, silas, kade
//!   text: message to inject into the role's Terminal window
//!
//! Exit codes: 0 = injected, 1 = failed (role unknown, window not found, etc.)

use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.len() < 2 {
        eprintln!("Usage: chorus-inject <role> <text>");
        eprintln!("  role: wren, silas, kade");
        eprintln!("  Injects text into the role's Terminal window via osascript.");
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

fn role_pattern(role: &str) -> Option<&'static str> {
    match role {
        "wren" => Some("product-manager"),
        "silas" => Some("architect"),
        "kade" => Some("engineer"),
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

    // #2245: use "do script" instead of activate + keystroke.
    // Prior approach (#2100) used activate + keystroke which required:
    //   - TCC Accessibility grant (broke on every rebuild until #2075 split the crate)
    //   - Display awake (activate/frontmost fail when display sleeps)
    //   - Focus theft (steals window focus, disrupts Jeff)
    // "do script" is a Terminal.app command — targets a specific tab directly.
    // No TCC, no display, no focus theft. Works from LaunchAgent context overnight.
    let script = format!(
        r#"tell application "Terminal"
    repeat with w in windows
        set winName to name of w
        if winName contains "{pattern}" and winName contains "claude" then
            do script "{text}" in (selected tab of w)
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
