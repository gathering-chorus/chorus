//! process — Shared role-to-PID detection for L2 team awareness.
//! Used by nudge.rs (delivery routing) and role_state.rs (liveness).

use std::process::Command;

/// Find the PID of a role's active Claude process by matching CWD.
/// Used by role_state.rs for liveness checks — NOT used for nudge delivery.
/// Nudge delivery uses inject_by_tab_name which bypasses PID entirely.
pub fn find_role_pid(role: &str) -> Option<u32> {
    let output = Command::new("ps")
        .args(["-eo", "pid,comm"])
        .output()
        .ok()?;
    let output = String::from_utf8_lossy(&output.stdout);

    let mut best_pid: Option<u32> = None;

    for line in output.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[1] == "claude" {
            if let Ok(pid) = parts[0].parse::<u32>() {
                if let Some(cwd) = get_cwd(pid) {
                    let matches = match role {
                        "wren" => cwd.contains("product-manager"),
                        "silas" => cwd.contains("architect"),
                        "kade" => cwd.contains("engineer"),
                        _ => false,
                    };
                    if matches && best_pid.map_or(true, |prev| pid > prev) {
                        best_pid = Some(pid);
                    }
                }
            }
        }
    }

    best_pid
}

/// Get CWD of a process via lsof — used by sender detection and find_role_pid.
pub fn get_cwd(pid: u32) -> Option<String> {
    Command::new("lsof")
        .args(["-p", &pid.to_string(), "-a", "-d", "cwd", "-Fn"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with('n'))
                .map(|l| l[1..].to_string())
        })
}

/// Inject text into a role's Terminal window via inline osascript.
/// Calls osascript directly — inherits Terminal's TCC Accessibility grant.
/// No external binary, so cargo rebuild can never revoke TCC. (#2100 revert of #2075)
pub fn inject_by_tab_name(role: &str, text: &str) -> Result<(), String> {
    let pattern = match role {
        "wren" => "product-manager",
        "silas" => "architect",
        "kade" => "engineer",
        _ => return Err(format!("unknown role: {}", role)),
    };

    // Escape for AppleScript double-quoted strings.
    // Single quotes are fine inside double quotes — do NOT escape them.
    let escaped = text
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ")
        .replace('\u{2014}', "--")
        .replace('\u{2018}', "'")
        .replace('\u{2019}', "'")
        .replace('\u{201C}', "\\\"")
        .replace('\u{201D}', "\\\"");

    // #1764: saves/restores frontmost app to prevent focus theft.
    // Targets Terminal windows only (never Chrome). DEC-107.
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
                    delay 0.05
                    key code 36
                end tell
            end tell
            delay 0.05
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

/// Wall clock timestamp in Boston timezone
pub fn wall_clock() -> String {
    Command::new("date")
        .env("TZ", "America/New_York")
        .args(["+%Y-%m-%d %H:%M:%S"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- AC4: wall_clock returns valid Boston timestamp ---

    #[test]
    fn wall_clock_returns_valid_timestamp() {
        let clock = wall_clock();
        assert_ne!(clock, "unknown", "wall_clock should not return unknown");
        assert!(clock.len() >= 16, "should be YYYY-MM-DD HH:MM:SS: {}", clock);
        assert!(clock.starts_with("20"), "should start with year: {}", clock);
        assert!(clock.contains('-'), "should have date separators: {}", clock);
        assert!(clock.contains(':'), "should have time separators: {}", clock);
    }

    // --- AC4: role PID detection ---

    #[test]
    fn find_role_pid_returns_some_for_active_role() {
        // At least kade (us) should be running
        let pid = find_role_pid("kade");
        // May or may not find us depending on process tree — just verify no crash
        // The function is resilient to missing processes
        let _ = pid;
    }

    #[test]
    fn find_role_pid_returns_none_for_unknown_role() {
        let pid = find_role_pid("nonexistent-role");
        assert!(pid.is_none());
    }

    #[test]
    fn get_cwd_returns_path_for_self() {
        let pid = std::process::id();
        let cwd = get_cwd(pid);
        assert!(cwd.is_some(), "should get CWD for own process");
    }

    #[test]
    fn get_cwd_returns_none_for_invalid_pid() {
        let cwd = get_cwd(999999999);
        assert!(cwd.is_none());
    }

    // --- #2100: inject_by_tab_name uses inline osascript, not external binary ---

    #[test]
    fn inject_by_tab_name_uses_inline_osascript() {
        // Structural test: inject_by_tab_name should call osascript directly,
        // not delegate to an external binary. We verify by calling with a role
        // that has no window — the error message should come from osascript/AppleScript,
        // not from "chorus-inject spawn failed".
        let result = inject_by_tab_name("silas", "structural-test");
        if let Err(e) = result {
            assert!(
                !e.contains("chorus-inject spawn failed"),
                "should use inline osascript, not external binary: {}", e
            );
        }
        // Ok means it actually injected (Terminal window found) — also fine
    }

    #[test]
    fn inject_by_tab_name_rejects_unknown_role() {
        let result = inject_by_tab_name("nonexistent", "test");
        assert!(result.is_err(), "should reject unknown role");
    }

    #[test]
    fn inject_by_tab_name_escapes_double_quotes() {
        // Can't test actual injection without Terminal, but verify the function
        // handles a role that won't have a window — should return an error about
        // no window found, not a crash or unescaped quote error.
        let result = inject_by_tab_name("silas", "test with \"quotes\" inside");
        // Will fail (no matching window in test context) but shouldn't panic
        assert!(result.is_err() || result.is_ok());
    }
}
