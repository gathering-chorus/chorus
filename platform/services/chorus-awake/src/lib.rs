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

pub mod lifecycle;

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
/// #4295 — the line carries the LOGIN, not "registered yes": on 2026-09-25
/// "registered yes" printed over two sessions that had no login at all.
pub fn proof_line(role: &str, l: &Live, login: &str, how: &str) -> String {
    let host = if l.host == "tmux" { String::new() } else { format!("  host={} (nudges need tmux)", l.host) };
    let login = if login.is_empty() { "NOT logged in" } else { login };
    format!("awake: {}  pid {}  tty {}  pane {}  {}{}  via {}", role, l.pid, l.tty, l.pane, login, host, how)
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

// ------------------------------------------------------ the verbs (#4295)

/// Everything a verb reads from the world, resolved once. Every field has an
/// env override so the tests bring their own world (#3528).
struct Ctx {
    home: String,
    root: String,
    sessions_dir: PathBuf,
    claude: String,
    tmux: String,
    ps: String,
    wait: u64,
    stale_hours: f64,
    identity_dir: String,
    token_bin: String,
    api: String,
    curl: String,
    log_bin: String,
    probe: String,
    services: Vec<(String, String)>,
    service_wait: u64,
}

impl Ctx {
    fn from_env() -> Ctx {
        let home = envd("HOME", "/tmp");
        let root = env::var("CHORUS_ROOT").ok().or_else(|| env::current_exe().ok().and_then(|p| p.ancestors().nth(6).map(|a| a.to_string_lossy().to_string()))).unwrap_or_else(|| format!("{}/CascadeProjects/chorus", home));
        Ctx {
            sessions_dir: PathBuf::from(envd("CHORUS_SESSIONS_DIR", &format!("{}/.chorus/sessions", home))),
            claude: envd("CLAUDE_BIN", &format!("{}/.local/bin/claude", home)),
            tmux: envd("TMUX_BIN", "tmux"),
            ps: envd("AWAKE_PS", "ps"),
            wait: envd("AWAKE_WAIT", "20").parse().unwrap_or(20),
            stale_hours: envd("AWAKE_STALE_HOURS", "24").parse().unwrap_or(24.0),
            identity_dir: envd("CHORUS_IDENTITY_DIR", &format!("{}/.chorus/identity", home)),
            token_bin: envd("CHORUS_TOKEN_BIN", &format!("{}/platform/scripts/chorus-identity-token", root)),
            api: envd("CHORUS_API_URL", "http://localhost:3360"),
            curl: envd("AWAKE_CURL", "curl"),
            log_bin: log_bin_for(&root),
            probe: envd("AWAKE_PROBE_BIN", "curl"),
            services: lifecycle::parse_services(&envd("AWAKE_SERVICES", lifecycle::DEFAULT_SERVICES)),
            service_wait: envd("AWAKE_SERVICE_WAIT", "180").parse().unwrap_or(180),
            home,
            root,
        }
    }
    /// AWAKE_ROLE_DIR is the single-role override the #4184/#4202 suites use;
    /// AWAKE_ROLES_BASE serves `up`, which starts all three.
    fn role_dir(&self, role: &str) -> String {
        if let Ok(d) = env::var("AWAKE_ROLE_DIR") { return d; }
        if let Ok(b) = env::var("AWAKE_ROLES_BASE") { return format!("{}/{}", b, role); }
        format!("{}/roles/{}", self.root, role)
    }
    fn state_path(&self, role: &str) -> PathBuf { PathBuf::from(&self.identity_dir).join(role).join("login.json") }
    fn read_state(&self, role: &str) -> LoginState {
        fs::read_to_string(self.state_path(role)).map(|t| lifecycle::parse_login_state(&t)).unwrap_or(LoginState::Unknown)
    }
    fn write_state(&self, role: &str, st: &LoginState) {
        let p = self.state_path(role);
        if let Some(d) = p.parent() { let _ = fs::create_dir_all(d); }
        let _ = write_private(&p, &lifecycle::login_state_json(st, &iso_utc(now_ms() as u64 / 1000)));
    }
    fn spine(&self, args: &[&str]) { let _ = sh(&self.log_bin, args); }
    fn tmux_session(role: &str) -> String { format!("chorus-{}", role) }
    fn has_tmux(&self, role: &str) -> bool { sh(&self.tmux, &["has-session", "-t", &Ctx::tmux_session(role)]).is_ok() }
    fn set_bar(&self, role: &str, word: &str) {
        let s = Ctx::tmux_session(role);
        if !self.has_tmux(role) { return; }
        let _ = sh(&self.tmux, &["set-option", "-t", &s, "status-right-length", "60"]);
        let _ = sh(&self.tmux, &["set-option", "-t", &s, "status-right", &lifecycle::tmux_status_right(word)]);
    }
    fn answered(&self, role: &str) -> bool {
        let mute_secs: u64 = envd("AWAKE_MUTE_SECS", "900").parse().unwrap_or(900);
        let spine = fs::read_to_string(envd("CHORUS_LOG_FILE", &format!("{}/.chorus/chorus.log", self.home))).unwrap_or_default();
        match sh("date", &["+%z"]).ok().as_deref().and_then(tz_offset_secs) {
            Some(o) => answered_recently(&spine, role, mute_secs, now_ms() as u64 / 1000, o),
            None => true,
        }
    }
}

use lifecycle::{Came, LoginState};

fn mmss(s: u64) -> String { format!("{}:{:02}", s / 60, s % 60) }

/// Wait for identity, chorus-api and athena-make to answer, with the countdown
/// on one line. Err names the first one still not answering at the bound.
fn wait_for_services(ctx: &Ctx, bound: u64, show: bool) -> Result<(), String> {
    let start = std::time::Instant::now();
    let mut drew = false;
    for (name, url) in &ctx.services {
        loop {
            let code = sh(&ctx.probe, &["-s", "-o", "/dev/null", "-w", "%{http_code}", "--max-time", "2", url]).unwrap_or_default();
            if lifecycle::is_answering(&code) { break; }
            let el = start.elapsed().as_secs();
            if el >= bound {
                if drew { eprintln!(); }
                return Err(format!("{} not answering after {}", name, mmss(bound)));
            }
            if show { eprint!("\r{}   ", lifecycle::countdown_line(name, url, el, bound)); drew = true; }
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    if drew { eprintln!(); }
    Ok(())
}

/// The login (#4202, #4215): token, principal check, Session row. Ok is
/// Recorded or Pending; Err is a refusal (wrong principal), already printed.
fn do_login(ctx: &Ctx, role: &str) -> Result<LoginState, ()> {
    let refuse_on_any = envd("AWAKE_REFUSE_ON_LOGIN_FAILURE", "0") == "1";
    let token = sh(&ctx.token_bin, &[role]).map(|t| t.trim().to_string());
    let login = match &token {
        Ok(t) => login_check(role, t, (now_ms() / 1000) as u64),
        Err(e) => Err(format!("no token: {}", e.trim())),
    };
    let login = match login {
        Ok(l) => l,
        Err(why) => match login_posture(Some(&why), refuse_on_any) {
            Start::Refuse(w) => {
                eprintln!("chorus-awake: REFUSED — {} for {}", w, role);
                if w.contains("wrong principal") {
                    eprintln!("  nothing was started; starting on another principal's credential would file this work as theirs.");
                } else {
                    eprintln!("  nothing was started (AWAKE_REFUSE_ON_LOGIN_FAILURE=1 — the pre-#4215 posture).");
                }
                return Err(());
            }
            _ => {
                ctx.spine(&["session.login.degraded", role, &format!("reason={}", why)]);
                return Ok(LoginState::Pending { why, pid: None });
            }
        },
    };
    let token = token.unwrap_or_default();
    let host_account = envd("USER", "unknown");
    let start_id = format!("{:x}", now_ms());
    let (session_name, body) = session_row(role, &login, &host_account, &start_id);
    // the row: POST through the security API with the token as a header FILE (0600), never an argv
    let role_id_dir = PathBuf::from(&ctx.identity_dir).join(role);
    let _ = fs::create_dir_all(&role_id_dir);
    let hdr = role_id_dir.join("session.hdr");
    let body_path = role_id_dir.join("session.body");
    if let Err(e) = write_private(&hdr, &format!("Authorization: Bearer {}\n", token)).and_then(|_| write_private(&body_path, &body.to_string())) {
        return Ok(LoginState::Pending { why: format!("cannot write {}: {}", hdr.display(), e), pid: None });
    }
    let url = format!("{}/v1/identity/sessions", ctx.api);
    let hdr_arg = format!("@{}", hdr.display());
    let body_arg = format!("@{}", body_path.display());
    let code = sh(&ctx.curl, &["-s", "-o", "/dev/null", "-w", "%{http_code}", "--max-time", "10", "-X", "POST", "-H", "Content-Type: application/json", "-H", &hdr_arg, "--data-binary", &body_arg, &url]).map(|c| c.trim().to_string()).unwrap_or_else(|e| format!("curl failed: {}", e.trim()));
    let _ = fs::remove_file(&hdr);
    // #4215 — a 409 on a per-start name means something real: ask WHOSE row it is.
    if code == "409" {
        let get = format!("{}/v1/identity/sessions/{}", ctx.api, session_name);
        let existing = sh(&ctx.curl, &["-s", "--max-time", "10", &get]).unwrap_or_default();
        let mine = format!("principal-{}", role);
        if existing.contains(&mine) {
            eprintln!("chorus-awake: session {} already open and owned by {} — reusing it", session_name, mine);
        } else {
            let owner = existing.split("ownedBy").nth(1).map(|t| t.chars().take(60).collect::<String>()).unwrap_or_else(|| "unknown".into());
            eprintln!("chorus-awake: REFUSED — session {} exists and is NOT yours (ownedBy{})", session_name, owner);
            eprintln!("  nothing was started; starting under someone else's session would log your work as theirs.");
            return Err(());
        }
    } else if !(code == "200" || code == "201") {
        // #4215 — degrade, do not block: the identity was verified above; what
        // failed is the bookkeeping write, and #4295 retries it on its own.
        ctx.spine(&["session.login.degraded", role, &format!("http={}", code), &format!("session={}", session_name)]);
        return Ok(LoginState::Pending { why: format!("the identity API answered HTTP {}", code), pid: None });
    }
    ctx.spine(&["session.login", role, &format!("webid={}", login.webid), &format!("jti={}", login.jti), &format!("session={}", session_name), &format!("host_account={}", host_account), &format!("expires_at={}", iso_utc(login.exp))]);
    println!("login: {}  webid {}  jti {}  session {}  recorded yes", role, login.webid, login.jti, session_name);
    Ok(LoginState::Recorded { session: session_name, pid: None })
}

/// Services first (bounded, counted down), then the login. A service still
/// down at the bound is a PENDING login, never a refusal to start.
fn login_after_services(ctx: &Ctx, role: &str, bound: u64) -> Result<LoginState, ()> {
    match wait_for_services(ctx, bound, true) {
        Ok(()) => do_login(ctx, role),
        Err(why) => { ctx.spine(&["session.login.degraded", role, &format!("reason={}", why)]); Ok(LoginState::Pending { why, pid: None }) }
    }
}

fn pending_line(role: &str, why: &str) -> String {
    format!("login: {}  pending — {}. {} is starting and logs itself in when that answers; nothing for you to do.", role, why, role)
}

/// A pending login keeps trying in the background, so Jeff never has to.
fn spawn_retry(role: &str) {
    if envd("AWAKE_NO_RETRY", "0") == "1" { return; }
    if let Ok(me) = env::current_exe() {
        let _ = Command::new(me).args(["relogin", role]).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
    }
}

fn came_of(st: &LoginState) -> Came {
    match st { LoginState::Recorded { .. } => Came::LoggedIn, LoginState::Pending { why, .. } => Came::Pending(why.clone()), _ => Came::Pending("no login".into()) }
}

fn attach_to(ctx: &Ctx, role: &str) {
    let s = Ctx::tmux_session(role);
    if env::var("TMUX").is_ok() { let _ = sh(&ctx.tmux, &["switch-client", "-t", &s]); }
    else { let _ = Command::new(&ctx.tmux).args(["attach", "-t", &s]).status(); }
}

/// `on <role>`: start it logged in, or, when it is already running, make sure
/// THIS session is logged in and go to its window. Never a second copy.
fn on(ctx: &Ctx, role: &str, attach: bool) -> Result<Came, (i32, String)> {
    let tmux_session = Ctx::tmux_session(role);
    let role_dir = ctx.role_dir(role);

    // 1/4/6 — registry first: one live → check it; two → refuse.
    let live = live_entries(&ctx.sessions_dir, role, &ctx.ps);
    if live.len() >= 2 {
        eprintln!("chorus-awake: REFUSED — {} live sessions for {}; one role, one session:", live.len(), role);
        for l in &live { eprintln!("  pid {}  tty {}  host {}  pane {}", l.pid, l.tty, l.host, l.pane); }
        eprintln!("  run `chorus-principal off {}` to end both, then `chorus-principal on {}`.", role, role);
        return Err((1, format!("{} live sessions", live.len())));
    }
    if let Some(l) = live.first() {
        let liveness_on = envd("AWAKE_LIVENESS", "1") != "0";
        if awake_verdict(true, ctx.answered(role), liveness_on) == Awake::AlreadyAwake {
            // #4295 — "already awake" is earned by a login too, not only a pid.
            let st = ctx.read_state(role);
            let st = match &st {
                LoginState::Recorded { .. } if lifecycle::login_belongs_to(&st, l.pid) => st.with_pid(l.pid),
                _ => {
                    println!("{} is running without a login; logging it in now", role);
                    match login_after_services(ctx, role, ctx.service_wait) { Ok(s) => s.with_pid(l.pid), Err(()) => return Err((1, "login refused".into())) }
                }
            };
            ctx.write_state(role, &st);
            if let LoginState::Pending { why, .. } = &st { println!("{}", pending_line(role, why)); spawn_retry(role); }
            let word = lifecycle::login_word(&st, Some(l.pid));
            ctx.set_bar(role, word);
            println!("{}", proof_line(role, l, word, "already awake"));
            if attach { attach_to(ctx, role); }
            return Ok(came_of(&st));
        }
        let mute_secs: u64 = envd("AWAKE_MUTE_SECS", "900").parse().unwrap_or(900);
        eprintln!("chorus-awake: {} has a live session (pid {}) that has not spoken in {}s — MUTE, replacing it", role, l.pid, mute_secs);
        eprintln!("  a pid and a registry entry are not an answer; ending it so a fresh conversation can start.");
        let _ = sh(&ctx.claude, &["stop", &l.pid.to_string()]);
        let _ = fs::remove_file(ctx.sessions_dir.join(format!("{}-{}.json", role, l.pid)));
    }

    // 2/7 — the real conversation (#4215: an unreadable list continues, never refuses).
    let agents = match sh(&ctx.claude, &["agents", "--json", "--cwd", &role_dir]) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("chorus-awake: could not list {}'s background sessions: {}", role, e.trim());
            eprintln!("  continuing with the last conversation (claude -c); a detached one may be left behind.");
            String::from("[]")
        }
    };
    let projects = PathBuf::from(env::var("AWAKE_PROJECTS_DIR").unwrap_or_else(|_| projects_dir_for(&ctx.home, &role_dir).to_string_lossy().to_string()));
    let latest = latest_session_in(&projects);
    let dec = decide(&agents, latest.as_deref(), ctx.stale_hours, now_ms());
    // #4219 — a conversation the API keeps refusing is not one to resume.
    let check_on = envd("AWAKE_TRANSCRIPT_CHECK", "1") != "0";
    let window: usize = envd("AWAKE_TRANSCRIPT_WINDOW", "40").parse().unwrap_or(40);
    let threshold: usize = envd("AWAKE_TRANSCRIPT_REFUSALS", "1").parse().unwrap_or(1);
    let poisoned = check_on && latest.as_deref().map(|id| {
        let tail = fs::read_to_string(projects.join(format!("{}.jsonl", id))).unwrap_or_default();
        transcript_is_poisoned(&tail, window, threshold)
    }).unwrap_or(false);
    let (cmd, how) = if poisoned {
        let id = latest.clone().unwrap_or_default();
        eprintln!("chorus-awake: the last conversation ({}) ends in API refusals — not resuming it", id);
        eprintln!("  starting a FRESH conversation instead; turning the key has to start the car.");
        (ctx.claude.clone(), format!("fresh conversation ({} ends in API refusals)", id))
    } else {
        match &dec.attach {
            Some(id) => (format!("{} attach {}", ctx.claude, id), format!("attach {} (the last conversation, detached in the background)", id)),
            None => (format!("{} -c", ctx.claude), "claude -c (last conversation)".to_string()),
        }
    };
    // 5 — stale agents: named; ended only on request.
    if !dec.stale.is_empty() {
        if envd("AWAKE_END_STALE", "0") == "1" {
            for id in &dec.stale { match sh(&ctx.claude, &["rm", id]) { Ok(_) => println!("ended stale agent {}", id), Err(e) => eprintln!("could not end {}: {}", id, e.trim()) } }
        } else {
            println!("stale: {} background agent(s) for {} older than {}h ({}); AWAKE_END_STALE=1 ends them", dec.stale.len(), role, ctx.stale_hours, dec.stale.join(","));
        }
    }

    // 8 — LOGIN before anything is started (#4202), after the services answer (#4295).
    let st = match login_after_services(ctx, role, ctx.service_wait) { Ok(s) => s, Err(()) => return Err((1, "login refused".into())) };
    ctx.write_state(role, &st);
    if let LoginState::Pending { why, .. } = &st { println!("{}", pending_line(role, why)); }
    let token_file = PathBuf::from(&ctx.identity_dir).join(role).join("token.cache");

    // 1 — the pane, then launch inside it.
    let launch = format!("cd '{}' && source '{}/platform/scripts/chorus-env-setup.sh' && export CHORUS_SESSION_TOKEN_FILE='{}' && {}", role_dir, ctx.root, token_file.display(), cmd);
    let in_own_pane = env::var("TMUX").is_ok() && sh(&ctx.tmux, &["display-message", "-p", "#S"]).map(|s| s.trim() == tmux_session).unwrap_or(false);
    if in_own_pane && attach {
        println!("awake: {}  starting here via {}", role, how);
        let _ = Command::new("bash").arg("-c").arg(&launch).status();
        return Ok(came_of(&st));
    }
    if !ctx.has_tmux(role) {
        if let Err(e) = sh(&ctx.tmux, &["new-session", "-d", "-s", &tmux_session, "-c", &role_dir]) { eprintln!("chorus-awake: tmux new-session failed: {}", e); return Err((1, format!("tmux new-session failed: {}", e.trim()))); }
    }
    if let Err(e) = sh(&ctx.tmux, &["send-keys", "-t", &tmux_session, &launch, "Enter"]) { eprintln!("chorus-awake: tmux send-keys failed: {}", e); return Err((1, format!("tmux send-keys failed: {}", e.trim()))); }

    // 3 — prove it: wait for a live tmux-hosted registry entry.
    let mut found: Option<Live> = None;
    for i in 0..=ctx.wait {
        found = live_entries(&ctx.sessions_dir, role, &ctx.ps).into_iter().next();
        if found.is_some() || i == ctx.wait { break; }
        std::thread::sleep(Duration::from_secs(1));
    }
    let Some(l) = found else {
        eprintln!("awake: {}  registered NO after {}s  via {} — look at the pane: {} attach -t {}", role, ctx.wait, how, ctx.tmux, tmux_session);
        return Err((1, format!("did not register in {}s", ctx.wait)));
    };
    let st = st.with_pid(l.pid);
    ctx.write_state(role, &st);
    if matches!(st, LoginState::Pending { .. }) { spawn_retry(role); }
    let word = lifecycle::login_word(&st, Some(l.pid));
    ctx.set_bar(role, word);
    println!("{}", proof_line(role, &l, word, &how));
    if attach { attach_to(ctx, role); }
    Ok(came_of(&st))
}

/// `relogin <role>` — the background retry a pending login starts. Ends when
/// the login is recorded, when the role is gone, or at AWAKE_RETRY_SECS.
fn relogin(ctx: &Ctx, role: &str) -> i32 {
    let total: u64 = envd("AWAKE_RETRY_SECS", "1800").parse().unwrap_or(1800);
    let every: u64 = envd("AWAKE_RETRY_EVERY", "10").parse().unwrap_or(10).max(1);
    let start = std::time::Instant::now();
    loop {
        let Some(l) = live_entries(&ctx.sessions_dir, role, &ctx.ps).into_iter().next() else { return 0 };
        let st = ctx.read_state(role);
        if matches!(st, LoginState::Recorded { .. }) && lifecycle::login_belongs_to(&st, l.pid) { return 0; }
        if matches!(st, LoginState::Closed) { return 0; }
        if wait_for_services(ctx, 0, false).is_ok() {
            match do_login(ctx, role) {
                Ok(LoginState::Recorded { session, .. }) => {
                    let st = LoginState::Recorded { session: session.clone(), pid: Some(l.pid) };
                    ctx.write_state(role, &st);
                    ctx.set_bar(role, "logged in");
                    ctx.spine(&["session.login.recovered", role, &format!("session={}", session), &format!("after_secs={}", start.elapsed().as_secs())]);
                    return 0;
                }
                Ok(_) => {}
                Err(()) => return 1,
            }
        }
        if start.elapsed().as_secs() >= total {
            ctx.spine(&["session.login.gave_up", role, &format!("after_secs={}", total)]);
            return 1;
        }
        std::thread::sleep(Duration::from_secs(every));
    }
}

/// Close the Session row: read it, write sessionState=closed + endedAt back.
fn close_row(ctx: &Ctx, role: &str, session: &str) -> Result<(), String> {
    let token = sh(&ctx.token_bin, &[role]).map_err(|e| format!("no token: {}", e.trim()))?;
    let role_id_dir = PathBuf::from(&ctx.identity_dir).join(role);
    let _ = fs::create_dir_all(&role_id_dir);
    let hdr = role_id_dir.join("session.hdr");
    let body_path = role_id_dir.join("session.body");
    write_private(&hdr, &format!("Authorization: Bearer {}\n", token.trim()))?;
    let url = format!("{}/v1/identity/sessions/{}", ctx.api, session);
    let hdr_arg = format!("@{}", hdr.display());
    let existing = sh(&ctx.curl, &["-s", "--max-time", "10", "-H", &hdr_arg, &url]).unwrap_or_default();
    let result = match lifecycle::closed_row(&existing, &iso_utc(now_ms() as u64 / 1000)) {
        None => Err(format!("session {} not found", session)),
        Some(row) => {
            write_private(&body_path, &row.to_string())?;
            let body_arg = format!("@{}", body_path.display());
            let code = sh(&ctx.curl, &["-s", "-o", "/dev/null", "-w", "%{http_code}", "--max-time", "10", "-X", "PUT", "-H", "Content-Type: application/json", "-H", &hdr_arg, "--data-binary", &body_arg, &url]).map(|c| c.trim().to_string()).unwrap_or_default();
            if code == "200" || code == "201" || code == "204" { Ok(()) } else { Err(format!("the identity API answered HTTP {}", code)) }
        }
    };
    let _ = fs::remove_file(&hdr);
    result
}

/// `off <role>` — end the role and close its login. `--from-exit` is the
/// SessionEnd hook after /exit: the session is already ending, so only the
/// login is closed, and /clear (a new conversation, same session) is ignored.
fn off(ctx: &Ctx, role: &str, from_exit: bool) -> i32 {
    if from_exit {
        let mut input = String::new();
        let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut input);
        let reason = serde_json::from_str::<Value>(&input).ok().and_then(|v| v.get("reason").and_then(|r| r.as_str()).map(String::from)).unwrap_or_default();
        if !lifecycle::exit_reason_logs_out(&reason) { return 0; }
    }
    let st = ctx.read_state(role);
    let closed = match &st {
        LoginState::Recorded { session, .. } => Some((session.clone(), close_row(ctx, role, session))),
        _ => None,
    };
    ctx.write_state(role, &LoginState::Closed);
    let how = if from_exit { "exit" } else { "off" };
    let session = closed.as_ref().map(|(s, _)| s.as_str()).unwrap_or("");
    ctx.spine(&["session.logout", role, &format!("session={}", session), &format!("how={}", how)]);
    if !from_exit {
        let live = live_entries(&ctx.sessions_dir, role, &ctx.ps);
        if ctx.has_tmux(role) { let _ = sh(&ctx.tmux, &["kill-session", "-t", &Ctx::tmux_session(role)]); }
        for l in &live { let _ = fs::remove_file(ctx.sessions_dir.join(format!("{}-{}.json", role, l.pid))); }
        if live.is_empty() && closed.is_none() { println!("{} off (it was not running)", role); return 0; }
    }
    match closed {
        Some((s, Ok(()))) => println!("{} off: login closed (session {})", role, s),
        Some((s, Err(why))) => println!("{} off: stopped; the login row {} could not be closed ({}), and it expires on its own", role, s, why),
        None => println!("{} off: stopped; it had no recorded login to close", role),
    }
    0
}

fn status(ctx: &Ctx) -> i32 {
    for role in ROLES {
        let live = live_entries(&ctx.sessions_dir, role, &ctx.ps);
        let pid = live.first().map(|l| l.pid);
        let answering = pid.is_some() && ctx.answered(role);
        println!("{}", lifecycle::status_line(role, pid, &ctx.read_state(role), answering));
        if live.len() >= 2 { println!("       {} live sessions: `chorus-principal off {}` ends both", live.len(), role); }
    }
    0
}

/// Open the windows from Jeff's layout, each on its role: Wren in VS Code's
/// terminal (a folder-open task), Silas and Kade in a Terminal window each.
/// A role that already has a window attached gets no second one.
fn open_windows(ctx: &Ctx) {
    let principal = envd("CHORUS_PRINCIPAL_BIN", &format!("{}/.chorus/bin/chorus-principal", ctx.home));
    let osa = envd("AWAKE_OSASCRIPT", "osascript");
    let attached = |role: &str| sh(&ctx.tmux, &["list-clients", "-t", &Ctx::tmux_session(role)]).map(|o| !o.trim().is_empty()).unwrap_or(false);
    // Wren: VS Code
    if !attached("wren") {
        let dir = PathBuf::from(envd("AWAKE_VSCODE_DIR", &format!("{}/.vscode", ctx.root)));
        let tasks = dir.join("tasks.json");
        match fs::read_to_string(&tasks) {
            Ok(t) if t.contains("on wren") => {}
            Ok(_) => eprintln!("chorus-awake: {} exists without the wren task; not changing it. The task is:\n{}", tasks.display(), lifecycle::vscode_tasks_json(&principal)),
            Err(_) => { let _ = fs::create_dir_all(&dir); let _ = fs::write(&tasks, lifecycle::vscode_tasks_json(&principal)); }
        }
        // VS Code asks once before running a folder-open task; this answers it
        // in the workspace so Jeff never sees the prompt.
        let settings = dir.join("settings.json");
        let current = fs::read_to_string(&settings).unwrap_or_else(|_| "{}".into());
        match serde_json::from_str::<Value>(&current) {
            Ok(Value::Object(mut m)) if !m.contains_key("task.allowAutomaticTasks") => {
                m.insert("task.allowAutomaticTasks".into(), Value::String("on".into()));
                let _ = fs::write(&settings, serde_json::to_string_pretty(&Value::Object(m)).unwrap_or_default() + "\n");
            }
            Ok(_) => {}
            Err(_) => eprintln!("chorus-awake: {} is not plain JSON; add \"task.allowAutomaticTasks\": \"on\" by hand", settings.display()),
        }
        let _ = sh(&envd("AWAKE_OPEN", "open"), &["-a", "Visual Studio Code", &ctx.root]);
    }
    for role in ["silas", "kade"] {
        if attached(role) { continue; }
        let script = format!("tell application \"Terminal\" to do script \"{} on {}\"", principal, role);
        let _ = sh(&osa, &["-e", &script]);
    }
}

/// `up [--windows]` — after a reboot: all three roles, logged in, one line.
fn up(ctx: &mut Ctx, windows: bool) -> i32 {
    // services are waited for ONCE, with the countdown; each role after that probes once
    if let Err(why) = wait_for_services(ctx, ctx.service_wait, true) { eprintln!("chorus-awake: {}", why); }
    ctx.service_wait = 0;
    let mut results = Vec::new();
    for role in ["wren", "kade", "silas"] {
        let came = match on(ctx, role, false) { Ok(c) => c, Err((_, why)) => Came::NotStarted(why) };
        results.push((role.to_string(), came));
    }
    let line = lifecycle::summary_line(&results);
    println!("{}", line);
    ctx.spine(&["roles.up", "system", &format!("summary={}", line)]);
    let note = format!("display notification \"{}\" with title \"Chorus\"", line.replace('"', "'"));
    let _ = sh(&envd("AWAKE_OSASCRIPT", "osascript"), &["-e", &note]);
    if windows { open_windows(ctx); }
    if results.iter().any(|(_, c)| matches!(c, Came::NotStarted(_))) { 1 } else { 0 }
}

/// Refuse a verb run from an agent session for another role (#4295).
fn caller_refusal(role: &str, verb: &str) -> Option<String> {
    let in_agent = env::var("CLAUDECODE").map(|v| !v.is_empty()).unwrap_or(false);
    let caller = env::var("CHORUS_ROLE").ok().filter(|r| !r.is_empty());
    lifecycle::caller_may_act(in_agent, caller.as_deref(), role, verb).err()
}

const USAGE: &str = "usage: chorus-awake on|off|status|up|relogin [role]   (wren | silas | kade)
  on <role>      start it logged in, or log in the running one and go to its window
  off <role>     stop it and close its login
  status         one line per role: running, logged in, answering
  up [--windows] all three after a reboot, one summary line; --windows opens VS Code and two Terminal windows
  <role>         same as on";

/// The whole command. Returns the exit code.
pub fn run(args: &[String]) -> i32 {
    let mut ctx = Ctx::from_env();
    let verb = args.first().map(String::as_str).unwrap_or("");
    let role_arg = |i: usize| -> Result<String, i32> {
        match args.get(i) {
            Some(r) if ROLES.contains(&r.as_str()) => Ok(r.clone()),
            Some(r) => { eprintln!("chorus-awake: unknown role '{}' (wren | silas | kade)", r); Err(2) }
            None => { eprintln!("{}", USAGE); Err(2) }
        }
    };
    match verb {
        "status" => status(&ctx),
        "up" => {
            if let Some(why) = caller_refusal("all roles", "start") { eprintln!("chorus-awake: REFUSED — {}", why); return 2; }
            up(&mut ctx, args.iter().any(|a| a == "--windows"))
        }
        "relogin" => match role_arg(1) { Ok(r) => relogin(&ctx, &r), Err(c) => c },
        "off" => match role_arg(1) {
            Ok(r) => {
                if let Some(why) = caller_refusal(&r, "stop") { eprintln!("chorus-awake: REFUSED — {}", why); return 2; }
                off(&ctx, &r, args.iter().any(|a| a == "--from-exit"))
            }
            Err(c) => c,
        },
        "on" | "wren" | "silas" | "kade" => {
            let r = if verb == "on" { match role_arg(1) { Ok(r) => r, Err(c) => return c } } else { verb.to_string() };
            if let Some(why) = caller_refusal(&r, "start") { eprintln!("chorus-awake: REFUSED — {}", why); eprintln!("  nothing was started."); return 2; }
            let attach = envd("AWAKE_NO_ATTACH", "0") != "1";
            match on(&ctx, &r, attach) { Ok(_) => 0, Err((c, _)) => c }
        }
        "" => { eprintln!("{}", USAGE); 2 }
        other => { eprintln!("chorus-awake: unknown role '{}' (wren | silas | kade)", other); 2 }
    }
}
