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
//! All pure logic lives in the library crate (`chorus_inject`). This module
//! is argv parsing + osascript spawn + stdout handling.
//!
//! #2029 reverted #2245's "do script" approach — do script doesn't send a
//! Return that Claude Code recognizes, breaking auto-submit.
//! #2166 added CHORUS_INJECT_DRY_RUN — set to 1 in tests to skip osascript.
//! #2167 moved pure logic to lib.rs so tarpaulin measures real coverage.

use std::process::{Command, ExitCode};

use chorus_inject::{
    build_count_windows_script, build_inject_script, escape_for_applescript, role_pattern,
};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.len() == 2 && args[0] == "--count-windows" {
        return match count_windows(&args[1]) {
            Ok(s) => {
                println!("{}", s);
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{}", e);
                ExitCode::from(1)
            }
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

fn count_windows(pattern: &str) -> Result<String, String> {
    let script = build_count_windows_script(pattern);
    let output = Command::new("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| format!("osascript spawn failed: {}", e))?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn inject(role: &str, text: &str) -> Result<(), String> {
    let pattern = role_pattern(role).ok_or_else(|| format!("unknown role: {}", role))?;
    let escaped = escape_for_applescript(text);

    // Test seam: CHORUS_INJECT_DRY_RUN=1 skips the osascript call and prints what
    // would have been injected. Added #2166 so integration tests can exercise the
    // full binary path (argv parse, role validation, escape) without firing real
    // keystrokes into live terminals.
    if std::env::var("CHORUS_INJECT_DRY_RUN").is_ok() {
        println!(
            "DRY-RUN inject role={} pattern={} escaped={}",
            role, pattern, escaped
        );
        return Ok(());
    }

    let script = build_inject_script(pattern, &escaped, role);
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
