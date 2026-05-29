//! #3125 — session registry (write side).
//!
//! At SessionStart, capture `{role, pid, tty, host}` and write
//! `~/.chorus/sessions/<role>-<pid>.json`. Delivery (pulse) reads this to
//! route nudges by tty — an exact, host-agnostic key — instead of guessing by
//! window-title substring. This is the routing/transport split: the registry
//! is ROUTING ground-truth; chorus-inject is TRANSPORT.
//!
//! Best-effort by contract: a capture failure must NEVER block session boot,
//! and the registry being empty just means pulse falls back to the legacy
//! name-match path (as-is). Nothing here can strand delivery.

use std::path::PathBuf;
use std::process::Command;

/// Map `TERM_PROGRAM` → host class. Terminal.app, VS Code, and iTerm each set
/// this env var, and it propagates terminal → shell → claude → this hook.
/// The host decides transport: terminal/iterm → tty-matched osascript;
/// vscode → defer to the inbox/fold (osascript would leak into the focused app).
pub fn host_from_term_program(tp: Option<&str>) -> &'static str {
    match tp {
        Some("Apple_Terminal") => "terminal",
        Some("vscode") => "vscode",
        Some("iTerm.app") => "iterm",
        _ => "unknown",
    }
}

/// Normalize `ps -o tty=` output to a `/dev` path. `"ttys004"` → `/dev/ttys004`.
/// `"??"` / empty (a process with no controlling tty) → None.
pub fn parse_tty(ps_out: &str) -> Option<String> {
    let t = ps_out.trim();
    if t.is_empty() || t == "??" || t == "?" {
        return None;
    }
    if t.starts_with("/dev/") {
        Some(t.to_string())
    } else {
        Some(format!("/dev/{}", t))
    }
}

/// The session registration JSON line. Pure so it's unit-tested without a
/// filesystem. `registered_at` is epoch-seconds-as-string — the resolver sorts
/// it lexically to pick the most-recent of two sessions, and equal-width epoch
/// strings compare in numeric order.
pub fn registration_json(role: &str, pid: u32, tty: &str, host: &str, epoch_secs: u64) -> String {
    format!(
        "{{\"role\":\"{}\",\"pid\":{},\"tty\":\"{}\",\"host\":\"{}\",\"registered_at\":\"{}\"}}",
        role, pid, tty, host, epoch_secs
    )
}

/// Walk the parent-process chain from `start_pid` to find the `claude` process
/// and return its `(pid, tty)`. The agent's own command shell has no
/// controlling tty (it's piped), but the `claude` process does — that's the
/// session tty a nudge must reach. Best-effort via `ps`.
fn find_claude(start_pid: u32) -> Option<(u32, String)> {
    let mut pid = start_pid;
    for _ in 0..12 {
        let comm = Command::new("ps")
            .args(["-o", "comm=", "-p", &pid.to_string()])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default();
        let comm = comm.trim();
        if comm == "claude" || comm.ends_with("/claude") {
            let tty_out = Command::new("ps")
                .args(["-o", "tty=", "-p", &pid.to_string()])
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .unwrap_or_default();
            return parse_tty(&tty_out).map(|tty| (pid, tty));
        }
        let ppid_out = Command::new("ps")
            .args(["-o", "ppid=", "-p", &pid.to_string()])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default();
        let ppid: u32 = ppid_out.trim().parse().unwrap_or(0);
        if ppid <= 1 {
            break;
        }
        pid = ppid;
    }
    None
}

/// Capture + write the session registration. Best-effort: any failure returns
/// silently (registration is an optimization; name-match remains the fallback).
/// Writes NOTHING to stdout — the SessionStart envelope JSON must stay clean.
pub fn register(role: &str) {
    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(_) => return,
    };
    let (pid, tty) = match find_claude(std::process::id()) {
        Some(p) => p,
        None => return,
    };
    let host = host_from_term_program(std::env::var("TERM_PROGRAM").ok().as_deref());
    let dir = PathBuf::from(&home).join(".chorus").join("sessions");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let json = registration_json(role, pid, &tty, host, now);
    let _ = std::fs::write(dir.join(format!("{}-{}.json", role, pid)), json);
}

/// Remove this session's registration (best-effort, called at session close).
/// Liveness (pid-alive) already prevents a dead session from being resolved;
/// this just keeps the directory tidy.
pub fn deregister(role: &str) {
    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(_) => return,
    };
    if let Some((pid, _)) = find_claude(std::process::id()) {
        let path = PathBuf::from(&home)
            .join(".chorus")
            .join("sessions")
            .join(format!("{}-{}.json", role, pid));
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_mapping_covers_each_emulator() {
        assert_eq!(host_from_term_program(Some("Apple_Terminal")), "terminal");
        assert_eq!(host_from_term_program(Some("vscode")), "vscode");
        assert_eq!(host_from_term_program(Some("iTerm.app")), "iterm");
        assert_eq!(host_from_term_program(None), "unknown");
        assert_eq!(host_from_term_program(Some("Hyper")), "unknown");
    }

    #[test]
    fn tty_parse_normalizes_and_rejects_none() {
        assert_eq!(parse_tty("ttys004\n"), Some("/dev/ttys004".to_string()));
        assert_eq!(parse_tty("/dev/ttys001"), Some("/dev/ttys001".to_string()));
        assert_eq!(parse_tty("??\n"), None);
        assert_eq!(parse_tty("?"), None);
        assert_eq!(parse_tty("   "), None);
    }

    #[test]
    fn registration_json_is_well_formed_and_consumable() {
        let j = registration_json("silas", 12345, "/dev/ttys001", "terminal", 1716985200);
        // Matches the SessionReg shape pulse's resolver parses.
        assert!(j.contains(r#""role":"silas""#));
        assert!(j.contains(r#""pid":12345"#));
        assert!(j.contains(r#""tty":"/dev/ttys001""#));
        assert!(j.contains(r#""host":"terminal""#));
        assert!(j.contains(r#""registered_at":"1716985200""#));
    }
}
