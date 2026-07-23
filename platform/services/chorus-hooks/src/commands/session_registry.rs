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
        // #3668 — tmux sets TERM_PROGRAM=tmux for everything inside it. tmux
        // IS the transport-relevant host: delivery goes through the tmux
        // server (app-level, locked-screen-safe), whatever emulator displays it.
        Some("tmux") => "tmux",
        _ => "unknown",
    }
}

/// #3668 — the effective host + pane for registration. A session inside tmux
/// registers host="tmux" with its exact pane id (from $TMUX_PANE), regardless
/// of the displaying emulator — pulse routes on the pane, chorus-inject
/// delivers via `tmux load-buffer/paste-buffer` under osascript. Outside tmux,
/// unchanged. Pure so the rule is unit-tested without env mutation.
pub fn effective_host<'a>(
    term_program: Option<&str>,
    tmux_pane: Option<&'a str>,
) -> (&'static str, Option<&'a str>) {
    match tmux_pane {
        Some(p) if !p.trim().is_empty() => ("tmux", Some(p)),
        _ => (host_from_term_program(term_program), None),
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
    registration_json_tmux(role, pid, tty, host, None, epoch_secs)
}

/// #3668 — registration with an optional tmux pane id. When present, pulse's
/// planDelivery routes `--tmux <pane>` (app-level, locked-screen-safe) instead
/// of the vscode keystroke path. Pane ids are tmux-internal (`%N`) — sanitized
/// to the safe charset before embedding.
pub fn registration_json_tmux(
    role: &str,
    pid: u32,
    tty: &str,
    host: &str,
    tmux_pane: Option<&str>,
    epoch_secs: u64,
) -> String {
    let tmux_field = match tmux_pane {
        Some(p) if !p.trim().is_empty() => {
            let safe: String = p
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || matches!(*c, '%' | '.' | ':' | '-' | '_'))
                .collect();
            format!("\"tmux\":\"{}\",", safe)
        }
        _ => String::new(),
    };
    format!(
        "{{\"role\":\"{}\",\"pid\":{},\"tty\":\"{}\",\"host\":\"{}\",{}\"registered_at\":\"{}\"}}",
        role, pid, tty, host, tmux_field, epoch_secs
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

/// A parsed peer registration — filename + identity — for the eviction pass (#3439).
pub struct RegRow {
    pub file: String,
    pub role: String,
    pub pid: u32,
    pub tty: String,
}

/// Extract role/pid/tty from a registration JSON line (the `registration_json`
/// shape). None if any field is missing — a malformed file is skipped, never
/// evicted blindly. Pure (no fs), so the parse is unit-tested directly.
pub fn parse_reg(file: &str, json: &str) -> Option<RegRow> {
    let str_field = |key: &str| -> Option<String> {
        let pat = format!("\"{}\":\"", key);
        let start = json.find(&pat)? + pat.len();
        let rest = &json[start..];
        let end = rest.find('"')?;
        Some(rest[..end].to_string())
    };
    let role = str_field("role")?;
    let tty = str_field("tty")?;
    let pidpat = "\"pid\":";
    let pstart = json.find(pidpat)? + pidpat.len();
    let prest = &json[pstart..];
    let pend = prest.find(|c: char| !c.is_ascii_digit()).unwrap_or(prest.len());
    let pid: u32 = prest[..pend].parse().ok()?;
    Some(RegRow { file: file.to_string(), role, pid, tty })
}

/// #3439 AC1 — which existing registrations to evict when (me_role, me_pid, me_tty)
/// registers: (a) DEAD-pid files — GC of sessions that ended/rebooted without
/// deregistering; (b) live files on the SAME tty that aren't my own slot — a tty
/// has exactly one live occupant, so this kills the cross-role collision (one
/// pid/tty registered under two roles, the 2026-06-15 silas-on-kade's-tty bug).
/// Pure: liveness is injected so the rule is unit-tested without real pids.
pub fn evictable<F: Fn(u32) -> bool>(
    rows: &[RegRow],
    me_role: &str,
    me_pid: u32,
    me_tty: &str,
    is_alive: F,
) -> Vec<String> {
    rows.iter()
        .filter(|r| !(r.role == me_role && r.pid == me_pid)) // never evict my own slot
        .filter(|r| !is_alive(r.pid) || r.tty == me_tty) // dead, or claims my tty
        .map(|r| r.file.clone())
        .collect()
}

/// Liveness probe (macOS): a pid is alive if `ps -p <pid>` finds it. Probe
/// failure → assume alive (never evict on uncertainty — losing a live peer's
/// registration is worse than leaving a stale one).
fn pid_alive(pid: u32) -> bool {
    Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "pid="])
        .output()
        .map(|o| !String::from_utf8_lossy(&o.stdout).trim().is_empty())
        .unwrap_or(true)
}

/// Prune stale + same-tty-colliding registrations before writing ours (#3439 AC1).
/// Best-effort: a read/parse failure on any file just skips it.
fn prune_registry(dir: &std::path::Path, me_role: &str, me_pid: u32, me_tty: &str) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    let mut rows = Vec::new();
    for ent in entries.flatten() {
        let p = ent.path();
        if p.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let fname = match p.file_name().and_then(|f| f.to_str()) {
            Some(f) => f.to_string(),
            None => continue,
        };
        if let Ok(json) = std::fs::read_to_string(&p) {
            if let Some(row) = parse_reg(&fname, &json) {
                rows.push(row);
            }
        }
    }
    for file in evictable(&rows, me_role, me_pid, me_tty, pid_alive) {
        let _ = std::fs::remove_file(dir.join(file));
    }
}

/// #3608 — is this process entitled to register as `role`? Only when its OWN
/// environment says it IS that role. This is what stops the poison-writer
/// class: session-start-orchestration-e2e.bats ran the real shim as
/// `session-start silas` from wren's/kade's session trees, find_claude walked
/// up to the HOST's interactive claude, and the registry got a silas entry at
/// another role's pid — then same-tty pruning EVICTED the host's true
/// registration. Pure so it's unit-tested without env mutation.
pub fn registration_permitted(argv_role: &str, env_role: Option<&str>) -> bool {
    env_role == Some(argv_role)
}

/// Capture + write the session registration. Best-effort: any failure returns
/// silently (registration is an optimization; name-match remains the fallback).
/// Writes NOTHING to stdout — the SessionStart envelope JSON must stay clean.
pub fn register(role: &str) {
    // #3608 — refuse to register a role this process doesn't actually run as.
    // Emits a spine event so refusals are visible, not silent (ADR-046).
    let env_role = std::env::var("CHORUS_ROLE").ok();
    if !registration_permitted(role, env_role.as_deref()) {
        let _ = crate::chorus_log::run_silent(&[
            "session.registration.refused".to_string(),
            role.to_string(),
            format!("env_role={}", env_role.unwrap_or_else(|| "unset".to_string())),
        ]);
        return;
    }
    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(_) => return,
    };
    let (pid, tty) = match find_claude(std::process::id()) {
        Some(p) => p,
        None => return,
    };
    // #3668 — inside tmux, register host="tmux" + the exact pane id so pulse
    // routes the app-level tmux transport (locked-screen-safe) instead of
    // vscode keystrokes.
    let term_program = std::env::var("TERM_PROGRAM").ok();
    let tmux_pane_env = std::env::var("TMUX_PANE").ok();
    let (host, tmux_pane) = effective_host(term_program.as_deref(), tmux_pane_env.as_deref());
    // #3608 — test isolation seam: suites point this at their own tmpdir so a
    // test run can NEVER touch the live registry (test-brings-its-own-world).
    let dir = match std::env::var("CHORUS_SESSIONS_DIR") {
        Ok(d) if !d.is_empty() => PathBuf::from(d),
        _ => PathBuf::from(&home).join(".chorus").join("sessions"),
    };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    // #3439 AC1: GC dead-pid files + evict any sibling on this tty BEFORE writing
    // ours, so a reused tty / un-deregistered session can't leave a cross-role
    // collision behind (the 2026-06-15 routing bug).
    prune_registry(&dir, role, pid, &tty);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let json = registration_json_tmux(role, pid, &tty, host, tmux_pane, now);
    if std::fs::write(dir.join(format!("{}-{}.json", role, pid)), json).is_ok() {
        // #3608 — registrations are visible on the spine (legibility), so a
        // role that never registers (the kade #3605 gap) is diagnosable from
        // the absence of this event, not from a delivery failure later.
        let _ = crate::chorus_log::run_silent(&[
            "session.registered".to_string(),
            role.to_string(),
            format!(
                "pid={},tty={},host={}{}",
                pid,
                tty,
                host,
                tmux_pane.map(|p| format!(",tmux={}", p)).unwrap_or_default()
            ),
        ]);
    }
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
        assert_eq!(host_from_term_program(Some("tmux")), "tmux");
        assert_eq!(host_from_term_program(None), "unknown");
        assert_eq!(host_from_term_program(Some("Hyper")), "unknown");
    }

    #[test]
    fn tmux_pane_wins_the_host_and_carries_the_pane() {
        // #3668 — inside tmux (whatever emulator displays it), the transport-
        // relevant host is tmux + the exact pane id.
        assert_eq!(effective_host(Some("tmux"), Some("%3")), ("tmux", Some("%3")));
        assert_eq!(effective_host(Some("vscode"), Some("%12")), ("tmux", Some("%12")));
        assert_eq!(effective_host(Some("vscode"), None), ("vscode", None));
        assert_eq!(effective_host(Some("vscode"), Some("  ")), ("vscode", None));
        assert_eq!(effective_host(None, None), ("unknown", None));
    }

    #[test]
    fn registration_json_with_tmux_pane_is_consumable_and_sanitized() {
        // #3668 — pulse's readRegistry parses this shape; the pane field only
        // appears when present, and shell-meta chars never survive into it.
        let j = registration_json_tmux("wren", 54837, "/dev/ttys006", "tmux", Some("%3"), 1784816398);
        assert!(j.contains(r#""tmux":"%3""#), "{}", j);
        assert!(j.contains(r#""host":"tmux""#), "{}", j);
        let no_pane = registration_json_tmux("wren", 1, "/dev/ttys000", "vscode", None, 1);
        assert!(!no_pane.contains("tmux\":"), "{}", no_pane);
        // legacy wrapper unchanged shape
        assert_eq!(no_pane, registration_json("wren", 1, "/dev/ttys000", "vscode", 1));
        let hostile = registration_json_tmux("wren", 1, "/dev/ttys000", "tmux", Some("%3'; rm -rf /"), 1);
        assert!(!hostile.contains("rm -rf"), "{}", hostile);
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

    #[test]
    fn parse_reg_extracts_identity_from_a_registration_line() {
        let j = registration_json("silas", 62021, "/dev/ttys001", "terminal", 1716985200);
        let r = parse_reg("silas-62021.json", &j).unwrap();
        assert_eq!(r.role, "silas");
        assert_eq!(r.pid, 62021);
        assert_eq!(r.tty, "/dev/ttys001");
        assert_eq!(r.file, "silas-62021.json");
        // malformed → skipped, not panicked
        assert!(parse_reg("x.json", "{not json}").is_none());
    }

    #[test]
    fn evictable_gcs_dead_and_claims_the_tty() {
        // The exact 2026-06-15 live state: pid 62021/ttys001 registered as BOTH
        // kade and silas, plus a dead-pid file, plus a healthy peer on another tty.
        let rows = vec![
            RegRow { file: "kade-8547.json".into(), role: "kade".into(), pid: 8547, tty: "/dev/ttys001".into() },
            RegRow { file: "silas-62021.json".into(), role: "silas".into(), pid: 62021, tty: "/dev/ttys001".into() },
            RegRow { file: "kade-62021.json".into(), role: "kade".into(), pid: 62021, tty: "/dev/ttys001".into() },
            RegRow { file: "wren-37391.json".into(), role: "wren".into(), pid: 37391, tty: "/dev/ttys006".into() },
        ];
        let alive = |pid: u32| pid != 8547; // 8547 dead, rest alive
        // kade/62021 registers (re-anchoring its own ttys001 slot)
        let evict = evictable(&rows, "kade", 62021, "/dev/ttys001", alive);
        assert!(evict.contains(&"kade-8547.json".to_string()), "dead pid is GC'd");
        assert!(evict.contains(&"silas-62021.json".to_string()), "same-tty cross-role collision is evicted");
        assert!(!evict.contains(&"kade-62021.json".to_string()), "own slot is never evicted");
        assert!(!evict.contains(&"wren-37391.json".to_string()), "a live peer on another tty is kept");
        assert_eq!(evict.len(), 2);
    }

    #[test]
    fn evictable_keeps_everything_under_probe_uncertainty() {
        let rows = vec![
            RegRow { file: "wren-100.json".into(), role: "wren".into(), pid: 100, tty: "/dev/ttys000".into() },
        ];
        // alive=true for all, different tty, not me → nothing to evict
        let evict = evictable(&rows, "silas", 999, "/dev/ttys009", |_| true);
        assert!(evict.is_empty());
    }
}
