//! chorus-awake <role> — wake a role the same way every time (#4184).
//!
//! Jeff, 2026-09-16: "i feel like i need a standard script to start each of u
//! that initalizes u and makes sure i do the steps" and, an hour later, "it
//! must be rust". The hand sequence (exit, claude, the background-sessions
//! picker, "hi wren") put Kade's pane on the `claude agents` screen for 2.5
//! hours, spawned a second Wren, and left Kade's real conversation detached.
//!
//! What it does, in order:
//!   1. one tmux session per role (chorus-<role>), the role dir as cwd, role env
//!   2. the role's real conversation: claude's own rule is "the most recent
//!      conversation in this directory". If THAT one is detached in the
//!      background it is attached (claude attach <id>); otherwise it continues
//!      (claude -c). An older background helper is never the role's
//!      conversation (first live run: the 08:25 helper was attached instead of
//!      the 13:00 conversation). NEVER the picker.
//!   3. prove it: a live registry entry for the role with host=tmux and a pane
//!      (what nudge routing needs, ADR-039). One line, always.
//!   4. refuse a duplicate: two live sessions for one role → exit 1, both named
//!   5. background agents older than AWAKE_STALE_HOURS (24) are named; ended
//!      only with AWAKE_END_STALE=1. Never silently.
//!   6. idempotent: an awake role prints the same line and nothing is sent.
//!   7. a failed read of the background-session list REFUSES, never guesses.
//!   8. LOGIN (#4202) — Jeff, 2026-09-17: "to me chorus-awake must include
//!      authn for agents"; "agents must login to chorus"; "and then follow
//!      authz rules". Before the pane is started the role's token is obtained
//!      (chorus-identity-token), checked to name THAT role's WebID and to be
//!      unexpired, recorded as a Session row through the security API
//!      (/v1/security/sessions, owned by the principal), and announced on the
//!      spine as session.login. The pane inherits CHORUS_SESSION_TOKEN_FILE —
//!      the token file, never the token on a command line. No token, wrong
//!      principal, expired, or row refused → REFUSED, nothing started.
//!
//! Tests bring their own world (#3528): every outside thing is an env override.
//!   CLAUDE_BIN  TMUX_BIN  AWAKE_PS  CHORUS_SESSIONS_DIR  AWAKE_PROJECTS_DIR
//!   AWAKE_ROLE_DIR  AWAKE_WAIT  AWAKE_STALE_HOURS  AWAKE_END_STALE
//!   AWAKE_NO_ATTACH=1 (do not hand the terminal to the pane)
//!   CHORUS_TOKEN_BIN  AWAKE_CURL  CHORUS_LOG_BIN  CHORUS_IDENTITY_DIR  CHORUS_API_URL

use serde_json::Value;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const ROLES: [&str; 3] = ["wren", "silas", "kade"];

/// A live session from the registry (~/.chorus/sessions/<role>-<pid>.json).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Live {
    pub pid: u64,
    pub tty: String,
    pub host: String,
    pub pane: String,
}

/// What to launch, decided from claude's own session list + the newest
/// conversation file. Pure; unit-tested.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    /// `Some(id)` → `claude attach <id>`; `None` → `claude -c`.
    pub attach: Option<String>,
    /// background agent ids older than the stale window (never the attach target)
    pub stale: Vec<String>,
}

// ---------------------------------------------------------------- pure core

/// Parse one registry file's JSON into (pid, tty, host, pane). Missing pid → None.
pub fn parse_registry(json: &str) -> Option<Live> {
    let v: Value = serde_json::from_str(json).ok()?;
    let pid = v.get("pid")?.as_u64().or_else(|| v.get("pid")?.as_str()?.parse().ok())?;
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let pane = { let p = s("tmux"); if p.is_empty() { "-".to_string() } else { p } };
    Some(Live { pid, tty: s("tty"), host: s("host"), pane })
}

/// The one line Jeff reads. `how` says which path was taken.
pub fn proof_line(role: &str, l: &Live, how: &str) -> String {
    let reg = if l.host == "tmux" { "yes".to_string() } else { format!("yes-but-host={} (nudges need tmux)", l.host) };
    format!("awake: {}  pid {}  tty {}  pane {}  registered {}  via {}", role, l.pid, l.tty, l.pane, reg, how)
}

/// Decide attach-vs-continue and name stale agents. `agents_json` is the raw
/// output of `claude agents --json --cwd <role dir>`; `latest_session` is the
/// id of the newest conversation file in the role's projects dir.
pub fn decide(agents_json: &str, latest_session: Option<&str>, stale_hours: f64, now_ms: u128) -> Decision {
    let d: Vec<Value> = serde_json::from_str::<Value>(agents_json).ok().and_then(|v| v.as_array().cloned()).unwrap_or_default();
    let is_bg = |s: &Value| s.get("kind").and_then(|k| k.as_str()) == Some("background");
    let live = |s: &Value| {
        let has_pid = s.get("pid").map(|p| !p.is_null() && p.as_str() != Some("")).unwrap_or(false);
        let state = s.get("state").and_then(|x| x.as_str()).unwrap_or("");
        has_pid && state != "exited" && state != "stopped"
    };
    let id = |s: &Value| s.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let attach = latest_session.and_then(|latest| {
        d.iter()
            .filter(|s| is_bg(s) && live(s))
            .find(|s| s.get("sessionId").and_then(|x| x.as_str()).map(|sid| sid.starts_with(latest) || latest.starts_with(sid)).unwrap_or(false))
            .map(|s| id(s))
    });
    let window = (stale_hours * 3600.0 * 1000.0) as u128;
    let stale = d
        .iter()
        .filter(|s| is_bg(s))
        .filter(|s| {
            let started: u128 = s.get("startedAt").map(|x| x.as_str().and_then(|t| t.parse().ok()).or_else(|| x.as_u64().map(|n| n as u128)).unwrap_or(now_ms)).unwrap_or(now_ms);
            now_ms.saturating_sub(started) > window
        })
        .map(|s| id(s))
        .filter(|i| !i.is_empty() && Some(i) != attach.as_ref())
        .collect();
    Decision { attach, stale }
}

/// The session id of the newest conversation file in a projects dir
/// (`<id>.jsonl` by mtime). None when the dir is empty or unreadable.
pub fn latest_session_in(projects_dir: &Path) -> Option<String> {
    let mut best: Option<(SystemTime, String)> = None;
    for e in fs::read_dir(projects_dir).ok()?.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("jsonl") { continue; }
        let m = e.metadata().ok()?.modified().ok()?;
        let id = p.file_stem()?.to_str()?.to_string();
        if best.as_ref().map(|(t, _)| m > *t).unwrap_or(true) { best = Some((m, id)); }
    }
    best.map(|(_, id)| id)
}

/// ~/.claude/projects/<role dir with '/' → '-'>
pub fn projects_dir_for(home: &str, role_dir: &str) -> PathBuf {
    PathBuf::from(home).join(".claude").join("projects").join(role_dir.replace('/', "-"))
}

// ------------------------------------------------------------ login (#4202)

/// What a login proved. Pure; built from the token's payload claims.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Login {
    pub webid: String,
    pub jti: String,
    pub iat: u64,
    pub exp: u64,
}

/// base64url without padding → bytes (std only; the crate is std + serde_json).
pub fn b64url_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits = 0u32;
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            b'=' => break,
            _ => return None,
        } as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 { bits -= 8; out.push(((buf >> bits) & 0xff) as u8); }
    }
    Some(out)
}

/// The payload claims of a JWT-shaped token. The signature is NOT checked here:
/// CSS signs and the security API verifies on the row write; this step only
/// asks WHO the token names, so a token for another role is refused before
/// anything is started.
pub fn token_claims(token: &str) -> Option<Value> {
    let payload = token.split('.').nth(1)?;
    serde_json::from_slice(&b64url_decode(payload)?).ok()
}

/// Is this token a login for `role`? Err names why not, in the words the
/// refusal prints: "no session", "wrong principal", "expired".
pub fn login_check(role: &str, token: &str, now_s: u64) -> Result<Login, String> {
    let c = token_claims(token).ok_or_else(|| "no session: the token has no readable claims".to_string())?;
    let webid = c.get("webid").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let want = format!("/{}/profile/card#me", role);
    if !webid.ends_with(&want) {
        return Err(format!("wrong principal: the token names {} not {}", if webid.is_empty() { "nobody" } else { webid.as_str() }, role));
    }
    let exp = c.get("exp").and_then(|v| v.as_u64()).unwrap_or(0);
    if exp <= now_s { return Err(format!("expired: the token's exp {} is not after now {}", exp, now_s)); }
    let iat = c.get("iat").and_then(|v| v.as_u64()).unwrap_or(0);
    let jti = c.get("jti").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if jti.is_empty() { return Err("no session: the token carries no jti".to_string()); }
    Ok(Login { webid, jti, iat, exp })
}

fn iso_utc(secs: u64) -> String {
    // civil-from-days (Howard Hinnant), std only
    let days = (secs / 86400) as i64; let rem = secs % 86400;
    let z = days + 719468; let era = z.div_euclid(146097); let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400; let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153; let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m, d, rem / 3600, (rem % 3600) / 60, rem % 60)
}

/// The Session row body for the generated security API (the shape in
/// session-4202.ttl). `name` is the row key: session-<role>-<jti tail>.
pub fn session_row(role: &str, l: &Login, host_account: &str) -> (String, Value) {
    let tail: String = l.jti.chars().rev().take(8).collect::<Vec<_>>().into_iter().rev().collect();
    let name = format!("session-{}-{}", role, tail.replace(|c: char| !c.is_ascii_alphanumeric(), "-"));
    let body = serde_json::json!({
        "name": name,
        "label": format!("{} logged in {} on {}", role, iso_utc(l.iat), host_account),
        "ownedBy": format!("principal-{}", role),
        "tokenId": l.jti,
        "issuedAt": iso_utc(l.iat),
        "expiresAt": iso_utc(l.exp),
        "sessionState": "open",
        "hostAccount": host_account,
    });
    (name, body)
}

// ------------------------------------------------------------ world access

fn envd(k: &str, d: &str) -> String { env::var(k).unwrap_or_else(|_| d.to_string()) }

/// Write a file readable by this account only (0600). The token and the row
/// body pass through files so they never appear in `ps` or a shell history.
fn write_private(path: &Path, content: &str) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    use std::io::Write;
    let mut f = fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path).map_err(|e| e.to_string())?;
    f.write_all(content.as_bytes()).map_err(|e| e.to_string())
}

fn alive(ps: &str, pid: u64) -> bool {
    Command::new(ps).arg("-p").arg(pid.to_string()).output().map(|o| o.status.success()).unwrap_or(false)
}

fn live_entries(sessions_dir: &Path, role: &str, ps: &str) -> Vec<Live> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(sessions_dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !(name.starts_with(&format!("{}-", role)) && name.ends_with(".json")) { continue; }
            if let Ok(txt) = fs::read_to_string(e.path()) {
                if let Some(l) = parse_registry(&txt) { if alive(ps, l.pid) { out.push(l); } }
            }
        }
    }
    out.sort_by_key(|l| l.pid);
    out
}

fn sh(bin: &str, args: &[&str]) -> Result<String, String> {
    let o = Command::new(bin).args(args).output().map_err(|e| format!("{} {}: {}", bin, args.join(" "), e))?;
    if o.status.success() { Ok(String::from_utf8_lossy(&o.stdout).to_string()) } else { Err(String::from_utf8_lossy(&o.stderr).chars().take(200).collect()) }
}

fn now_ms() -> u128 { SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0) }

/// The whole verb. Returns the exit code; prints the proof line or the refusal.
pub fn run(args: &[String]) -> i32 {
    let role = match args.first() { Some(r) if ROLES.contains(&r.as_str()) => r.clone(), Some(r) => { eprintln!("chorus-awake: unknown role '{}' (wren | silas | kade)", r); return 2; } None => { eprintln!("usage: chorus-awake <role>   (wren | silas | kade)"); return 2; } };
    let home = envd("HOME", "/tmp");
    let root = env::var("CHORUS_ROOT").ok().or_else(|| env::current_exe().ok().and_then(|p| p.ancestors().nth(6).map(|a| a.to_string_lossy().to_string()))).unwrap_or_else(|| format!("{}/CascadeProjects/chorus", home));
    let role_dir = envd("AWAKE_ROLE_DIR", &format!("{}/roles/{}", root, role));
    let sessions_dir = PathBuf::from(envd("CHORUS_SESSIONS_DIR", &format!("{}/.chorus/sessions", home)));
    let claude = envd("CLAUDE_BIN", &format!("{}/.local/bin/claude", home));
    let tmux = envd("TMUX_BIN", "tmux");
    let ps = envd("AWAKE_PS", "ps");
    let wait: u64 = envd("AWAKE_WAIT", "20").parse().unwrap_or(20);
    let stale_hours: f64 = envd("AWAKE_STALE_HOURS", "24").parse().unwrap_or(24.0);
    let tmux_session = format!("chorus-{}", role);

    // 1/4/6 — registry first: one live → say so and stop; two → refuse.
    let live = live_entries(&sessions_dir, &role, &ps);
    if live.len() >= 2 {
        eprintln!("chorus-awake: REFUSED — {} live sessions for {}; one role, one session:", live.len(), role);
        for l in &live { eprintln!("  pid {}  tty {}  host {}  pane {}", l.pid, l.tty, l.host, l.pane); }
        eprintln!("  end one of them (exit in its pane), then run again.");
        return 1;
    }
    if let Some(l) = live.first() { println!("{}", proof_line(&role, l, "already awake")); return 0; }

    // 2/7 — the real conversation. A failed read is not "none": refuse.
    let agents = match sh(&claude, &["agents", "--json", "--cwd", &role_dir]) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("chorus-awake: REFUSED — could not list {}'s background sessions (claude agents --json --cwd failed): {}", role, e.trim());
            eprintln!("  nothing was started; without that list a detached conversation cannot be told from none.");
            return 1;
        }
    };
    let projects = PathBuf::from(env::var("AWAKE_PROJECTS_DIR").unwrap_or_else(|_| projects_dir_for(&home, &role_dir).to_string_lossy().to_string()));
    let latest = latest_session_in(&projects);
    let dec = decide(&agents, latest.as_deref(), stale_hours, now_ms());
    let (cmd, how) = match &dec.attach {
        Some(id) => (format!("{} attach {}", claude, id), format!("attach {} (the last conversation, detached in the background)", id)),
        None => (format!("{} -c", claude), "claude -c (last conversation)".to_string()),
    };

    // 5 — stale agents: named; ended only on request.
    if !dec.stale.is_empty() {
        if envd("AWAKE_END_STALE", "0") == "1" {
            for id in &dec.stale { match sh(&claude, &["rm", id]) { Ok(_) => println!("ended stale agent {}", id), Err(e) => eprintln!("could not end {}: {}", id, e.trim()) } }
        } else {
            println!("stale: {} background agent(s) for {} older than {}h ({}); AWAKE_END_STALE=1 ends them", dec.stale.len(), role, stale_hours, dec.stale.join(","));
        }
    }

    // 8 — LOGIN before anything is started (#4202). No token → no session → nothing runs.
    let token_bin = envd("CHORUS_TOKEN_BIN", &format!("{}/platform/scripts/chorus-identity-token", root));
    let identity_dir = envd("CHORUS_IDENTITY_DIR", &format!("{}/.chorus/identity", home));
    let token = match sh(&token_bin, &[&role]) {
        Ok(t) => t.trim().to_string(),
        Err(e) => { eprintln!("chorus-awake: REFUSED — no session for {}: {}", role, e.trim()); eprintln!("  nothing was started; a role that cannot log in does not run."); return 1; }
    };
    let login = match login_check(&role, &token, (now_ms() / 1000) as u64) {
        Ok(l) => l,
        Err(why) => { eprintln!("chorus-awake: REFUSED — no session for {}: {}", role, why); eprintln!("  nothing was started; a role that cannot log in does not run."); return 1; }
    };
    let host_account = envd("USER", "unknown");
    let (session_name, body) = session_row(&role, &login, &host_account);
    // the row: POST through the security API with the token as a header FILE (0600), never an argv
    let role_id_dir = PathBuf::from(&identity_dir).join(&role);
    let _ = fs::create_dir_all(&role_id_dir);
    let hdr = role_id_dir.join("session.hdr");
    let body_path = role_id_dir.join("session.body");
    if let Err(e) = write_private(&hdr, &format!("Authorization: Bearer {}\n", token)).and_then(|_| write_private(&body_path, &body.to_string())) {
        eprintln!("chorus-awake: REFUSED — login not recorded for {}: cannot write {}: {}", role, hdr.display(), e); return 1;
    }
    let api = envd("CHORUS_API_URL", "http://localhost:3360");
    let curl = envd("AWAKE_CURL", "curl");
    let url = format!("{}/v1/security/sessions", api);
    let hdr_arg = format!("@{}", hdr.display());
    let body_arg = format!("@{}", body_path.display());
    let code = sh(&curl, &["-s", "-o", "/dev/null", "-w", "%{http_code}", "--max-time", "10", "-X", "POST", "-H", "Content-Type: application/json", "-H", &hdr_arg, "--data-binary", &body_arg, &url]).map(|c| c.trim().to_string()).unwrap_or_else(|e| format!("curl failed: {}", e.trim()));
    let _ = fs::remove_file(&hdr);
    if !(code == "200" || code == "201") {
        eprintln!("chorus-awake: REFUSED — login not recorded for {}: {} answered HTTP {}", role, url, code);
        eprintln!("  nothing was started; a login the security API did not accept is not a login.");
        return 1;
    }
    let token_file = role_id_dir.join("token.cache");
    let log_bin = envd("CHORUS_LOG_BIN", &format!("{}/platform/scripts/chorus-log", root));
    let _ = Command::new("bash").arg(&log_bin).args(["session.login", &role, &format!("webid={}", login.webid), &format!("jti={}", login.jti), &format!("session={}", session_name), &format!("host_account={}", host_account), &format!("expires_at={}", iso_utc(login.exp))]).output();
    println!("login: {}  webid {}  jti {}  session {}  recorded yes", role, login.webid, login.jti, session_name);

    // 1 — the pane, then launch inside it.
    let launch = format!("cd '{}' && source '{}/platform/scripts/chorus-env-setup.sh' && export CHORUS_SESSION_TOKEN_FILE='{}' && {}", role_dir, root, token_file.display(), cmd);
    let no_attach = envd("AWAKE_NO_ATTACH", "0") == "1";
    let in_own_pane = env::var("TMUX").is_ok() && sh(&tmux, &["display-message", "-p", "#S"]).map(|s| s.trim() == tmux_session).unwrap_or(false);
    if in_own_pane && !no_attach {
        // we ARE the pane: run it here in the foreground; the line prints when it exits
        println!("awake: {}  starting here via {}", role, how);
        let _ = Command::new("bash").arg("-c").arg(&launch).status();
        if let Some(l) = live_entries(&sessions_dir, &role, &ps).first() { println!("{}", proof_line(&role, l, &how)); }
        return 0;
    }
    if sh(&tmux, &["has-session", "-t", &tmux_session]).is_err() {
        if let Err(e) = sh(&tmux, &["new-session", "-d", "-s", &tmux_session, "-c", &role_dir]) { eprintln!("chorus-awake: tmux new-session failed: {}", e); return 1; }
    }
    if let Err(e) = sh(&tmux, &["send-keys", "-t", &tmux_session, &launch, "Enter"]) { eprintln!("chorus-awake: tmux send-keys failed: {}", e); return 1; }

    // 3 — prove it: wait for a live tmux-hosted registry entry.
    let found: Option<Live>;
    let mut i = 0u64;
    loop {
        let f = live_entries(&sessions_dir, &role, &ps).into_iter().next();
        if f.is_some() || i >= wait { found = f; break; }
        std::thread::sleep(Duration::from_secs(1));
        i += 1;
    }
    let Some(l) = found else {
        eprintln!("awake: {}  registered NO after {}s  via {} — look at the pane: {} attach -t {}", role, wait, how, tmux, tmux_session);
        return 1;
    };
    println!("{}", proof_line(&role, &l, &how));

    // hand the terminal to the pane unless told not to
    if !no_attach {
        if env::var("TMUX").is_ok() { let _ = sh(&tmux, &["switch-client", "-t", &tmux_session]); }
        else { let _ = Command::new(&tmux).args(["attach", "-t", &tmux_session]).status(); }
    }
    0
}
