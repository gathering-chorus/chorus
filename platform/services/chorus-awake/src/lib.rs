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
//!      (/v1/identity/sessions, owned by the principal), and announced on the
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

/// #4215 — the spine binary, resolvable before the start block needs it.
fn log_bin_for(root: &str) -> String {
    envd("CHORUS_LOG_BIN", &format!("{}/platform/scripts/chorus-log", root))
}

/// #4215 — WHAT A LOGIN FAILURE COSTS. Jeff, 2026-09-19: "chorus-awake must be
/// a non issue day to day — highly reliable and resilient."
///
/// Every login failure used to return 1, so the API being down, a slow curl, a
/// shape complaint from the route, or an unwritable header file each meant the
/// role did not exist that morning. That is backwards: the work chorus-awake
/// protects is not the session ROW, it is Jeff having an engineer. On
/// 2026-09-18 it cost him Kade for an hour.
///
/// Exactly one failure still refuses: the credential names a different
/// principal. Starting on that would file Kade's work under Silas, and no
/// amount of loudness undoes a wrong author. Everything else — no token, an
/// unreadable or expired token, the API unreachable, a 5xx, a 422 — starts, and
/// says plainly that the session is unrecorded.
#[derive(Debug, PartialEq, Eq)]
pub enum Start { Go, Degraded(String), Refuse(String) }

pub fn login_posture(failure: Option<&str>, refuse_on_any: bool) -> Start {
    match failure {
        None => Start::Go,
        // "wrong principal: the token names X not Y" — login_check's own words.
        Some(w) if w.contains("wrong principal") => Start::Refuse(w.to_string()),
        // refuse_on_any is the OLD behaviour, kept only so the negative proof can
        // show the two postures differ on the same input.
        Some(w) if refuse_on_any => Start::Refuse(w.to_string()),
        Some(w) => Start::Degraded(w.to_string()),
    }
}

/// #4215 — the already-awake decision, pure so it can be watched failing.
/// Three inputs, three outcomes, and the point is that they are distinguishable:
/// a mute session must not read the same as a talking one.
#[derive(Debug, PartialEq, Eq)]
pub enum Awake { Fresh, AlreadyAwake, ReplaceMute }

pub fn awake_verdict(has_live_entry: bool, spoke_recently: bool, liveness_on: bool) -> Awake {
    if !has_live_entry { return Awake::Fresh; }
    if !liveness_on { return Awake::AlreadyAwake; }   // the OLD behaviour, kept only for the proof
    if spoke_recently { Awake::AlreadyAwake } else { Awake::ReplaceMute }
}

/// #4215 — DID IT ANSWER? A registry entry and a live pid say a process exists,
/// not that the session can respond. On 2026-09-18/19 Kade had both for over an
/// hour while every one of his turns came back an API refusal; `chorus-awake`
/// read the file, said "already awake", and did nothing. Jeff: "i cant
/// communicate with him reliable" — and he was right that the check was the
/// problem, not the role.
///
/// Liveness is the session's OWN most recent turn in the spine. A role that has
/// spoken inside the window is awake; one that has not is treated as gone,
/// whatever the file says. Reads the spine the same way pulse does — no new
/// surface, no second source of truth.
/// The machine's UTC offset in seconds, from `date +%z`. #4215 — returns None
/// when it cannot be read, and an unknown offset must never end a session: the
/// cost of guessing wrong is killing a role mid-turn.
pub fn tz_offset_secs(z: &str) -> Option<i64> {
    let z = z.trim();
    if z.len() < 5 { return None; }
    let sign = match z.as_bytes()[0] { b'+' => 1, b'-' => -1, _ => return None };
    let h: i64 = z[1..3].parse().ok()?;
    let m: i64 = z[3..5].parse().ok()?;
    Some(sign * (h * 3600 + m * 60))
}

pub fn answered_recently(spine: &str, role: &str, within_secs: u64, now_secs: u64, tz_offset_secs: i64) -> bool {
    // #4215 — the spine writes LOCAL time with no offset ("2026-09-19T07:33:01"),
    // and now_secs is UTC. Reading one as the other put every Boston timestamp
    // four hours in the past, so a role that had just answered read as mute and
    // would have been ended mid-turn. The offset is passed in, not read here, so
    // the test can prove both frames.
    // lines are the spine's json; we want this role's reply/turn events only
    for line in spine.lines().rev() {
        if !line.contains(&format!("\"role\":\"{}\"", role)) { continue; }
        if !(line.contains("reply.published") || line.contains("reply.emitted") || line.contains("agent.action")) { continue; }
        if let Some(i) = line.find("\"timestamp\":\"") {
            let t = &line[i + 13..];
            if let Some(j) = t.find('"') {
                if let Ok(when_local) = chrono_secs(&t[..j]) {
                    let when = (when_local as i64 - tz_offset_secs).max(0) as u64;
                    return now_secs.saturating_sub(when) <= within_secs;
                }
            }
        }
    }
    false
}

/// Seconds from an ISO-8601 local stamp the spine writes (2026-09-19T07:33:01...).
/// Self-contained: no chrono in this crate, and a wrong parse must read as "has
/// not spoken" rather than silently passing the liveness check.
pub fn chrono_secs(iso: &str) -> Result<u64, ()> {
    let b = iso.as_bytes();
    if b.len() < 19 { return Err(()); }
    let num = |a: usize, z: usize| -> Result<u64, ()> { iso[a..z].parse::<u64>().map_err(|_| ()) };
    let (y, mo, d) = (num(0,4)?, num(5,7)?, num(8,10)?);
    let (h, mi, se) = (num(11,13)?, num(14,16)?, num(17,19)?);
    // days since epoch, civil-from-days (Howard Hinnant's algorithm)
    let y2 = if mo <= 2 { y as i64 - 1 } else { y as i64 };
    let era = if y2 >= 0 { y2 } else { y2 - 399 } / 400;
    let yoe = y2 - era * 400;
    let mp = ((mo as i64 + 9) % 12) as i64;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Ok((days * 86400 + (h * 3600 + mi * 60 + se) as i64) as u64)
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

/// #4219 — IS THE LAST CONVERSATION STILL ALIVE? Jeff, 2026-09-19, after
/// `chorus-awake kade` came up and could not answer: "i dont want to have to do
/// this stuff as part of my jx"; "i just want it to work like turn a key to
/// start the car".
///
/// chorus-awake resumed the newest conversation and never asked whether the API
/// still accepts it. Kade's had 14 refusals in it (measured 08:52: 3,160 lines,
/// 14 flagged); every turn Jeff typed came back as an error until he cleared it
/// by hand. A start that registers, reports success and cannot say a word is
/// the same shape as #4215's silent-degrade, one layer out.
///
/// Pure over the transcript tail so the test can hold both a poisoned and a
/// healthy fixture. `window` is how many trailing lines count as "recent": one
/// refusal in a long history is noise, refusals at the END mean the
/// conversation is finished.
pub fn transcript_is_poisoned(tail: &str, window: usize, threshold: usize) -> bool {
    let lines: Vec<&str> = tail.lines().collect();
    let start = lines.len().saturating_sub(window);
    let hits = lines[start..].iter().filter(|l|
        l.contains("safeguards flagged") || l.contains("reasoning_extraction")
    ).count();
    hits >= threshold
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

/// The Session row body for the generated identity API (the shape in
/// session-4202.ttl).
///
/// `name` is the BARE key — <role>-<jti tail>-<start>, no `session-` prefix. The
/// mint adds the prefix itself (ADR-040) and refuses a name that already carries
/// it: "double-prefix ... pass the bare name". Sending session-<role>-<tail> got
/// a 422 on the first real login, after the route and the kind were both fixed.
///
/// #4215 — `start` exists because the name was the jti tail ALONE, and the token
/// is cached (~/.chorus/identity/<role>/token.cache). Two starts inside one
/// token's life therefore minted the SAME name, the second collided with the row
/// the first created, and the 409 stopped the role booting. 2026-09-18: Jeff ran
/// `chorus-awake kade` four times, I deleted a live session row by hand and then
/// cleared his token cache before it would start. A session is one START, not one
/// token; the name has to say so. Passed in rather than read from the clock here
/// so the unit test is deterministic.
pub fn session_row(role: &str, l: &Login, host_account: &str, start: &str) -> (String, Value) {
    let tail: String = l.jti.chars().rev().take(8).collect::<Vec<_>>().into_iter().rev().collect();
    let safe = |x: &str| x.replace(|c: char| !c.is_ascii_alphanumeric(), "-");
    let name = format!("{}-{}-{}", role, safe(&tail), safe(start));
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

/// Explicit profile selection opts a role into the runtime supervisor. Missing
/// config keeps the existing Claude launch path; invalid config never guesses.
pub fn configured_runtime_profile(role: &str, explicit: Option<&str>, config: Option<&str>) -> Result<Option<String>, String> {
    if let Some(profile) = explicit {
        if profile.trim().is_empty() { return Err("CHORUS_AGENT_PROFILE cannot be empty".into()); }
        return Ok(Some(profile.to_string()));
    }
    let config = match config { Some(text) => serde_json::from_str::<Value>(text).map_err(|e|format!("invalid agent profiles config: {e}"))?, None => return Ok(None) };
    match config.get("roles").and_then(|roles|roles.get(role)) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(profile)) if !profile.trim().is_empty() => Ok(Some(profile.clone())),
        _ => Err(format!("agent profile for {role} must be a nonempty string")),
    }
}

fn runtime_dispatch(role: &str, home: &str) -> Result<Option<i32>, String> {
    let state = env::var("CHORUS_AGENT_STATE_DIR").unwrap_or_else(|_|format!("{home}/.chorus"));
    let config_path = env::var("CHORUS_AGENT_CONFIG").unwrap_or_else(|_|format!("{state}/agent-profiles.json"));
    let explicit = env::var("CHORUS_AGENT_PROFILE").ok();
    let config = if explicit.is_some() { None } else {
        match fs::read_to_string(&config_path) {
            Ok(text) => Some(text),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(format!("cannot read {config_path}: {error}")),
        }
    };
    let profile = match configured_runtime_profile(role, explicit.as_deref(), config.as_deref())? { Some(profile) => profile, None => return Ok(None) };
    let bin = envd("CHORUS_AGENT_BIN", &format!("{home}/.chorus/bin/chorus-agent"));
    let mut command = Command::new(&bin);
    command.args(["launch", role, "--profile", &profile]);
    // Only an explicitly provided awake workspace overrides the configured
    // role anchor. Never pass the caller's incidental cwd or a latest session.
    if let Ok(cwd) = env::var("AWAKE_ROLE_DIR") { command.args(["--cwd", &cwd]); }
    let status = command.status().map_err(|e|format!("cannot launch runtime supervisor {bin}: {e}"))?;
    // Once a role opts in, a failed login/start remains a failure. Falling back
    // to Claude could start a second, differently authenticated conversation.
    Ok(Some(status.code().unwrap_or(1)))
}

/// The whole verb. Returns the exit code; prints the proof line or the refusal.
pub fn run(args: &[String]) -> i32 {
    let role = match args.first() { Some(r) if ROLES.contains(&r.as_str()) => r.clone(), Some(r) => { eprintln!("chorus-awake: unknown role '{}' (wren | silas | kade)", r); return 2; } None => { eprintln!("usage: chorus-awake <role>   (wren | silas | kade)"); return 2; } };
    let home = envd("HOME", "/tmp");
    match runtime_dispatch(&role, &home) {
        Ok(Some(code)) => return code,
        Ok(None) => {},
        Err(error) => { eprintln!("chorus-awake: REFUSED — {error}"); return 1; }
    }
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
    // #4215 — "already awake" now has to be EARNED. A registry entry plus a live
    // pid is what Kade had for over an hour while unable to answer anything, and
    // this branch blessed it and did nothing. Ask the spine whether the session
    // has spoken; if it has not, it is mute, and a mute session is ended and
    // replaced rather than reported as fine.
    //
    // AWAKE_MUTE_SECS is the window. AWAKE_LIVENESS=0 disables the check — and
    // exists so the negative proof can show the OLD behaviour (blessing a mute
    // session) rather than only asserting the new one.
    if let Some(l) = live.first() {
        let mute_secs: u64 = envd("AWAKE_MUTE_SECS", "900").parse().unwrap_or(900);
        let spine_path = envd("CHORUS_LOG_FILE", &format!("{}/.chorus/chorus.log", envd("HOME", "")));
        let spine = fs::read_to_string(&spine_path).unwrap_or_default();
        let now_secs = (now_ms() / 1000) as u64;
        let liveness_on = envd("AWAKE_LIVENESS", "1") != "0";
        let offset = sh("date", &["+%z"]).ok().as_deref().and_then(tz_offset_secs);
        let spoke = match offset {
            Some(o) => answered_recently(&spine, &role, mute_secs, now_secs, o),
            // unknown offset: every timestamp would read hours stale. Leave the
            // session alone and say why, rather than end a role on a guess.
            None => { eprintln!("chorus-awake: could not read the local UTC offset — skipping the mute check"); true }
        };
        if awake_verdict(true, spoke, liveness_on) == Awake::AlreadyAwake {
            println!("{}", proof_line(&role, l, "already awake"));
            return 0;
        }
        eprintln!("chorus-awake: {} has a live session (pid {}) that has not spoken in {}s — MUTE, replacing it", role, l.pid, mute_secs);
        eprintln!("  a pid and a registry entry are not an answer; ending it so a fresh conversation can start.");
        let _ = sh(&claude, &["stop", &l.pid.to_string()]);
        let _ = fs::remove_file(sessions_dir.join(format!("{}-{}.json", role, l.pid)));
    }

    // 2/7 — the real conversation.
    //
    // #4215 — this used to refuse. The reasoning was sound in isolation: a failed
    // read is not "none", and guessing "none" could start a second conversation
    // beside a live one. But the cost is not symmetric. The wrong guess costs a
    // duplicate pane Jeff can close; the refusal costs him the role entirely,
    // which is the thing this card exists to stop. So: carry on with an empty
    // list, which resolves to `claude -c` — the last conversation, the same thing
    // Jeff does by hand when a role goes quiet — and say plainly that the list
    // could not be read.
    let agents = match sh(&claude, &["agents", "--json", "--cwd", &role_dir]) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("chorus-awake: could not list {}'s background sessions: {}", role, e.trim());
            eprintln!("  continuing with the last conversation (claude -c); a detached one may be left behind.");
            String::from("[]")
        }
    };
    let projects = PathBuf::from(env::var("AWAKE_PROJECTS_DIR").unwrap_or_else(|_| projects_dir_for(&home, &role_dir).to_string_lossy().to_string()));
    let latest = latest_session_in(&projects);
    let dec = decide(&agents, latest.as_deref(), stale_hours, now_ms());
    // #4219 — a conversation the API keeps refusing is not a conversation to
    // resume. AWAKE_TRANSCRIPT_CHECK=0 disables this and exists for the
    // negative proof: the same poisoned fixture must be resumed without it.
    let check_on = envd("AWAKE_TRANSCRIPT_CHECK", "1") != "0";
    let window: usize = envd("AWAKE_TRANSCRIPT_WINDOW", "40").parse().unwrap_or(40);
    let threshold: usize = envd("AWAKE_TRANSCRIPT_REFUSALS", "1").parse().unwrap_or(1);
    let poisoned = check_on && latest.as_deref().map(|id| {
        let f = projects.join(format!("{}.jsonl", id));
        let tail = fs::read_to_string(&f).unwrap_or_default();
        transcript_is_poisoned(&tail, window, threshold)
    }).unwrap_or(false);

    let (cmd, how) = if poisoned {
        let id = latest.clone().unwrap_or_default();
        eprintln!("chorus-awake: the last conversation ({}) ends in API refusals — not resuming it", id);
        eprintln!("  starting a FRESH conversation instead; turning the key has to start the car.");
        (claude.clone(), format!("fresh conversation ({} ends in API refusals)", id))
    } else {
        match &dec.attach {
            Some(id) => (format!("{} attach {}", claude, id), format!("attach {} (the last conversation, detached in the background)", id)),
            None => (format!("{} -c", claude), "claude -c (last conversation)".to_string()),
        }
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
    let refuse_on_any = envd("AWAKE_REFUSE_ON_LOGIN_FAILURE", "0") == "1";
    let log_bin_early = log_bin_for(&root);
    let mut degraded: Option<String> = None;

    let token = sh(&token_bin, &[&role]).map(|t| t.trim().to_string());
    let login = match &token {
        Ok(t) => login_check(&role, t, (now_ms() / 1000) as u64),
        Err(e) => Err(format!("no token: {}", e.trim())),
    };
    let login = match login {
        Ok(l) => Some(l),
        Err(why) => match login_posture(Some(&why), refuse_on_any) {
            Start::Refuse(w) => {
                eprintln!("chorus-awake: REFUSED — {} for {}", w, role);
                if w.contains("wrong principal") {
                    eprintln!("  nothing was started; starting on another principal's credential would file this work as theirs.");
                } else {
                    eprintln!("  nothing was started (AWAKE_REFUSE_ON_LOGIN_FAILURE=1 — the pre-#4215 posture).");
                }
                return 1;
            }
            _ => {
                eprintln!("chorus-awake: session NOT recorded for {} — {}", role, why);
                eprintln!("  starting anyway: a login that cannot be written down must not decide whether you have an engineer.");
                let _ = sh(&log_bin_early, &["session.login.degraded", &role, &format!("reason={}", why)]);
                degraded = Some(why);
                None
            }
        },
    };
    let token = token.unwrap_or_default();
    // #4215 — only write a Session row when there are claims to write. Without
    // them the degrade was already announced above; the pane still starts.
    let mut row_written = true;
    if let Some(login) = login {
        let host_account = envd("USER", "unknown");
        let start_id = format!("{:x}", now_ms());
        let (session_name, body) = session_row(&role, &login, &host_account, &start_id);
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
        let url = format!("{}/v1/identity/sessions", api);
        let hdr_arg = format!("@{}", hdr.display());
        let body_arg = format!("@{}", body_path.display());
        let code = sh(&curl, &["-s", "-o", "/dev/null", "-w", "%{http_code}", "--max-time", "10", "-X", "POST", "-H", "Content-Type: application/json", "-H", &hdr_arg, "--data-binary", &body_arg, &url]).map(|c| c.trim().to_string()).unwrap_or_else(|e| format!("curl failed: {}", e.trim()));
        let _ = fs::remove_file(&hdr);
        // #4215 — a 409 says the session name already exists. With a per-start name
        // that should now be impossible, so a 409 here means something real: either a
        // clock/name collision, or a row that is not ours. Ask WHOSE it is before
        // refusing. "A login the API did not accept is not a login" is right; a login
        // it accepted a moment ago, on a row we own, IS one — and reading that as a
        // failure is what left Kade unreachable for an hour on 2026-09-18.
        if code == "409" {
            let get = format!("{}/v1/identity/sessions/{}", api, session_name);
            let existing = sh(&curl, &["-s", "--max-time", "10", &get]).unwrap_or_default();
            let mine = format!("principal-{}", role);
            let owned_by_me = existing.contains(&format!("\"ownedBy\":\"{}\"", mine))
                || existing.contains(&format!("\"ownedBy\": \"{}\"", mine))
                || existing.contains(&mine);
            if owned_by_me {
                eprintln!("chorus-awake: session {} already open and owned by {} — reusing it", session_name, mine);
            } else {
                let owner = existing.split("ownedBy").nth(1).map(|t| t.chars().take(60).collect::<String>()).unwrap_or_else(|| "unknown".into());
                eprintln!("chorus-awake: REFUSED — session {} exists and is NOT yours (ownedBy{})", session_name, owner);
                eprintln!("  nothing was started; starting under someone else's session would log your work as theirs.");
                return 1;
            }
        } else if !(code == "200" || code == "201") {
            // #4215 — DEGRADE, DO NOT BLOCK. Jeff, 2026-09-19: "chorus-awake must be a
            // non issue day to day — highly reliable and resilient."
            //
            // This branch used to refuse: "a login the security API did not accept is
            // not a login." True as a sentence about authentication, wrong as a rule
            // about starting: it made every hiccup in RECORDING the login stop the
            // role existing. The identity was already proven before this point —
            // login_check verified the token names this role and has not expired, and
            // that check still refuses. What fails here is the bookkeeping write.
            //
            // So: start, and make the gap loud rather than silent. A role that runs
            // with an unrecorded session is a visible gap; a role that never starts is
            // an hour of Jeff's morning.
            eprintln!("chorus-awake: session NOT recorded for {} — {} answered HTTP {}", role, url, code);
            eprintln!("  starting anyway: the identity was verified before this write, and a bookkeeping");
            eprintln!("  failure must not decide whether you have an engineer. Recorded as degraded.");
            let _ = sh(&log_bin_for(&root), &["session.login.degraded", &role, &format!("http={}", code), &format!("session={}", session_name)]);
            // #4215 — found in the live pair: the success line below printed
            // "recorded yes" two lines under "session NOT recorded". A start line
            // that contradicts the error above it is worse than no line at all.
            row_written = false;
        }
        let _ = Command::new("bash").arg(&log_bin_early).args(["session.login", &role, &format!("webid={}", login.webid), &format!("jti={}", login.jti), &format!("session={}", session_name), &format!("host_account={}", host_account), &format!("expires_at={}", iso_utc(login.exp))]).output();
        if row_written {
            println!("login: {}  webid {}  jti {}  session {}  recorded yes", role, login.webid, login.jti, session_name);
        } else {
            println!("login: {}  webid {}  jti {}  session {}  recorded NO — the API refused the row (started UNAUTHENTICATED: any write this pane attempts will be refused)", role, login.webid, login.jti, session_name);
        }
    }
    if let Some(why) = &degraded {
        println!("login: {}  recorded NO — {}  (started UNAUTHENTICATED: no session row, any write this pane attempts will be refused by the API)", role, why);
    }
    let token_file = PathBuf::from(&identity_dir).join(&role).join("token.cache");

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
