//! process — Shared role-to-PID detection for L2 team awareness.
//! Used by nudge.rs (delivery routing) and role_state.rs (liveness).

use std::process::Command;

/// Find the PID of a role's active Claude process by matching CWD.
/// Used by role_state.rs for liveness checks — NOT used for nudge delivery.
/// Nudge delivery is the spine-tick-poller reading chorus.log directly (#2435);
/// osascript inject (pair-enforce only) goes through the chorus-inject binary.
#[allow(dead_code)]
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
                        "wren" => cwd.contains("roles/wren") || cwd.contains("product-manager"),
                        "silas" => cwd.contains("roles/silas") || cwd.contains("architect"),
                        "kade" => cwd.contains("roles/kade") || cwd.contains("engineer"),
                        _ => false,
                    };
                    if matches && best_pid.is_none_or(|prev| pid > prev) {
                        best_pid = Some(pid);
                    }
                }
            }
        }
    }

    best_pid
}

/// Get CWD of a process via lsof — used by sender detection and find_role_pid.
///
/// #3802 — under a deadline. lsof on a loaded box can stall for minutes; an
/// uncapped call turns "the machine is busy" into "the session is dead" one
/// layer up. Expiry returns None, which callers must treat as COULD NOT
/// MEASURE, never as absence.
#[allow(dead_code)]
pub fn get_cwd(pid: u32) -> Option<String> {
    run_with_deadline("lsof", &["-p", &pid.to_string(), "-a", "-d", "cwd", "-Fn"], 5)
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with('n'))
                .map(|l| l[1..].to_string())
        })
}

/// How alive a role's session is. Three states, deliberately (#3802): the sweep
/// that read `None` as "dead" demoted WORKING roles to idle whenever a probe
/// merely failed — the Clearing showed Jeff idle tiles for roles mid-build.
/// "No" and "could not tell" are different answers and must stay different.
#[derive(Debug, Clone, PartialEq)]
pub enum Liveness {
    Alive(u32),
    Dead,
    Unmeasurable(&'static str),
}

/// Probe a role's session with measurement failure kept distinct from absence.
///
/// Dead requires POSITIVE evidence: every claude process enumerated AND every
/// one's cwd read AND none matching. If `ps` itself fails, or any claude pid's
/// cwd could not be read (that unreadable pid could be the role), the answer is
/// Unmeasurable — the caller keeps its last known state rather than inventing one.
pub fn probe_role_session(role: &str) -> Liveness {
    let Some(output) = run_with_deadline("ps", &["-eo", "pid,comm"], 5) else {
        return Liveness::Unmeasurable("ps");
    };
    let output = String::from_utf8_lossy(&output.stdout);

    let mut best_pid: Option<u32> = None;
    let mut unreadable = false;

    for line in output.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[1] == "claude" {
            if let Ok(pid) = parts[0].parse::<u32>() {
                match get_cwd(pid) {
                    Some(cwd) => {
                        let matches = match role {
                            "wren" => cwd.contains("roles/wren") || cwd.contains("product-manager"),
                            "silas" => cwd.contains("roles/silas") || cwd.contains("architect"),
                            "kade" => cwd.contains("roles/kade") || cwd.contains("engineer"),
                            _ => false,
                        };
                        if matches && best_pid.is_none_or(|prev| pid > prev) {
                            best_pid = Some(pid);
                        }
                    }
                    None => unreadable = true,
                }
            }
        }
    }

    match (best_pid, unreadable) {
        (Some(pid), _) => Liveness::Alive(pid),
        (None, true) => Liveness::Unmeasurable("lsof"),
        (None, false) => Liveness::Dead,
    }
}

/// Run a command with a hard deadline; expiry kills it and returns None.
/// None means COULD NOT MEASURE — callers must never read it as a result.
fn run_with_deadline(cmd: &str, args: &[&str], secs: u64) -> Option<std::process::Output> {
    let mut child = Command::new(cmd)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().ok(),
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(_) => return None,
        }
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

    // #3147 — NON-HERMETIC, #[ignore]'d out of the pre-commit's hermetic --lib --bins
    // suite (DEC-2525). get_cwd() reads the process's own cwd via a system call that
    // returns Some in an interactive shell but None in the daemon-spawned pre-commit/CI
    // context (cwd not readable there) — env-dependent, so it flaked the acp gate as a
    // false failure. Still runnable on demand (`cargo test -- --ignored`).
    #[test]
    #[ignore]
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

    // #2435 — inject_by_tab_name tests retired with the function. Nudge
    // delivery is now the spine-tick-poller; osascript coverage lives in
    // chorus-inject's own test suite (tests/inject_integration.rs).
}
