// @domain: identity — the product domain this service guards (#4538)
//! chorus-principal <verb> <role> — the session lifecycle (was chorus-awake, #4184; folded in by #4345).
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
pub mod rows;
pub mod msgs;
pub mod links;
pub mod onboarding;

use serde_json::Value;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// #4432 — the agent roles come from the roles door, not from a list here.
/// Refuses loudly when the door does not answer; never falls back to three.
fn agent_roles(ctx: &Ctx) -> Result<Vec<String>, String> {
    let url = format!("{}/v1/roles/roles?limit=500", ctx.api);
    let body = door(ctx, &["-s", "--max-time", "10", &url]).map_err(|e| format!("could not read {}: {}", url, e.trim()))?;
    rows::agent_roles(&body).map_err(|e| format!("{} ({})", e, url))
}

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

pub fn login_posture(failure: Option<&str>) -> Start {
    match failure {
        None => Start::Go,
        // "wrong principal: the token names X not Y" — login_check's own words.
        Some(w) if w.contains("wrong principal") => Start::Refuse(w.to_string()),
        // #4933 — one posture (Jeff 2026-11-08 17:24, "no": a failed sign-in does
        // not start the role). Degraded is a pending login; the start itself is
        // refused by the wrapper's `establish --run`, which fails on pending.
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

/// #4929 AC2 — a session younger than the silence window has not had the chance
/// to speak, so it is not mute. At the 2026-11-08 boot Kade's own-account session
/// (pid 44433) was replaced as "not spoken in 900s" when it was about 27s old, and
/// the replacement came up as jeffbridwell. `age_secs` None (unreadable) = judge it
/// as before (#4215's mute rule), never silently exempt.
pub fn too_young_to_judge(age_secs: Option<u64>, mute_secs: u64) -> bool {
    age_secs.map(|a| a < mute_secs).unwrap_or(false)
}

/// `ps -o etime=` → seconds: `[[dd-]hh:]mm:ss`.
pub fn parse_etime(s: &str) -> Option<u64> {
    let s = s.trim();
    let (days, rest) = match s.split_once('-') { Some((d, r)) => (d.parse::<u64>().ok()?, r), None => (0, s) };
    let parts: Vec<u64> = rest.split(':').map(|p| p.parse::<u64>().ok()).collect::<Option<_>>()?;
    let (h, m, sec) = match parts.as_slice() { [m, s] => (0, *m, *s), [h, m, s] => (*h, *m, *s), _ => return None };
    Some(days * 86400 + h * 3600 + m * 60 + sec)
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
/// lowercase, anything not a letter or digit becomes one dash, no dash at the ends
pub fn slug(x: &str) -> String {
    let mut out = String::new();
    for c in x.chars() {
        if c.is_ascii_alphanumeric() { out.push(c.to_ascii_lowercase()); }
        else if !out.ends_with('-') { out.push('-'); }
    }
    out.trim_matches('-').to_string()
}

pub fn session_row(role: &str, l: &Login, host_account: &str, start: &str) -> (String, Value) {
    let tail: String = l.jti.chars().rev().take(8).collect::<Vec<_>>().into_iter().rev().collect();
    // #4295 — the name is written the way the DAL stores it (lowercase, one dash,
    // none at the ends). It slugs what it is given, so the live login on
    // 2026-09-25 sent silas-3U01z2C--1a0d90f31b5, was stored as
    // silas-3u01z2c-1a0d90f31b5, and every read by the sent name answered 404:
    // `off` could never have closed it.
    let name = slug(&format!("{}-{}-{}", role, tail, start));
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

/// #4851 — the runtime is still running after AWAKE_SETTLE seconds (default 5),
/// checked once a second; Err names when it went away.
fn settled(ctx: &Ctx, pid: u64) -> Result<(), String> {
    let secs: u64 = envd("AWAKE_SETTLE", "5").parse().unwrap_or(5);
    for i in 0..=secs {
        if !alive(&ctx.ps, pid) { return Err(format!("exited within {}s", i.max(1))); }
        if i < secs { std::thread::sleep(Duration::from_secs(1)); }
    }
    Ok(())
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

/// #4845 AC3 — every door call a login makes carries its trace: the door logs
/// x-chorus-trace-id as the request's trace (athena-make effective_trace).
fn door(ctx: &Ctx, args: &[&str]) -> Result<String, String> {
    let hdr = env::var("CHORUS_TRACE_ID").ok().filter(|t| !t.is_empty()).map(|t| format!("x-chorus-trace-id: {}", t));
    let mut a: Vec<&str> = Vec::with_capacity(args.len() + 2);
    if let Some(h) = hdr.as_deref() { a.push("-H"); a.push(h); }
    a.extend_from_slice(args);
    sh(&ctx.curl, &a)
}

fn sh(bin: &str, args: &[&str]) -> Result<String, String> {
    let o = Command::new(bin).args(args).output().map_err(|e| format!("{} {}: {}", bin, args.join(" "), e))?;
    if o.status.success() { Ok(String::from_utf8_lossy(&o.stdout).to_string()) } else { Err(String::from_utf8_lossy(&o.stderr).chars().take(200).collect()) }
}

/// #4813 — the tmux argv that starts `launch` as the pane's own process, never as
/// keystrokes. On 10-30 a message typed into BQ's pane mid-launch left the shell at
/// `dquote>`, and every retry's send-keys line was swallowed as more of that
/// unfinished command. A fresh session runs it directly; an existing pane is
/// respawned (-k ends whatever stuck shell is there). remain-on-exit is set
/// BEFORE the launch runs, so a launch that dies at once still leaves its last
/// lines readable (AC3); setting it after lost that race. One tmux call, run in order.
pub fn start_in_pane_args(session: &str, dir: &str, launch: &str, exists: bool) -> Vec<String> {
    let s = |x: &str| x.to_string();
    let mut a: Vec<String> = Vec::new();
    if !exists {
        // a placeholder process holds the pane until the respawn below replaces it
        a.extend([s("new-session"), s("-d"), s("-s"), s(session), s("-c"), s(dir), s("sleep"), s("600"), s(";")]);
    }
    a.extend([s("set-option"), s("-t"), s(session), s("remain-on-exit"), s("on"), s(";")]);
    // #4943 — mouse off, so Jeff selects and copies a role pane's text with a plain drag
    a.extend([s("set-option"), s("-t"), s(session), s("mouse"), s("off"), s(";")]);
    a.extend([s("respawn-pane"), s("-k"), s("-t"), s(session), s("-c"), s(dir), s("bash"), s("-lc"), s(launch)]);
    a
}

/// #4934 AC3 — once the runtime has settled, its pane closes when it exits. The
/// remain-on-exit set before the launch (#4813 AC3) is only for a launch that dies
/// at once; left on, every role Jeff closed sat as a dead pane (11-09 08:53).
pub fn settled_pane_args(session: &str) -> Vec<String> {
    ["set-option", "-t", session, "remain-on-exit", "off"].iter().map(|s| s.to_string()).collect()
}

/// #4934 AC3 — every way a role is confirmed running (claude, agent, gemini, a model
/// runtime, or already awake) ends here: the bar shows its login word, and its pane is
/// settled so a quit closes it. On 11-09 10:31 only the model-runtime path settled, and
/// kade's pane (claude) died on every quit.
fn mark_running(ctx: &Ctx, role: &str, word: &str) {
    ctx.set_bar(role, word);
    let settle = settled_pane_args(&Ctx::tmux_session(role));
    let refs: Vec<&str> = settle.iter().map(|s| s.as_str()).collect();
    if let Err(e) = sh(&ctx.tmux, &refs) { ctx.spine(&["session.pane.unsettled", role, &format!("why={}", e.trim())]); }
}

/// Start the role's runtime in its pane (#4813). Refuses to respawn over a live
/// registry entry: a respawn kills the pane's process, and that must never be a
/// running role.
fn start_in_pane(ctx: &Ctx, role: &str, dir: &str, launch: &str) -> Result<(), (i32, String)> {
    let session = Ctx::tmux_session(role);
    let mut exists = ctx.has_tmux(role);
    if exists && !live_entries(&ctx.sessions_dir, role, &ctx.ps).is_empty() {
        return Err((1, format!("{} has a live session in {}; not respawning over it", role, session)));
    }
    // #4845 — a session with nothing live in it is left from an earlier run (10-31:
    // panes from 10-28 came back after the crash); end it and start a fresh one
    if exists {
        let _ = sh(&ctx.tmux, &["kill-session", "-t", &session]);
        exists = false;
    }
    let args = start_in_pane_args(&session, dir, launch, exists);
    let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    sh(&ctx.tmux, &refs).map(|_| ()).map_err(|e| (1, format!("tmux could not start {}: {}", role, e.trim())))
}

/// #4813 AC3 — the pane's last lines, for a launch that did not start.
fn pane_tail(ctx: &Ctx, role: &str) -> String {
    let out = sh(&ctx.tmux, &["capture-pane", "-p", "-S", "-20", "-t", &Ctx::tmux_session(role)]).unwrap_or_default();
    pane_tail_summary(&out)
}

/// The last non-empty lines of a pane capture, joined for one stderr line.
pub fn pane_tail_summary(capture: &str) -> String {
    let lines: Vec<&str> = capture.lines().map(str::trim_end).filter(|l| !l.trim().is_empty()).collect();
    let tail = &lines[lines.len().saturating_sub(5)..];
    if tail.is_empty() { "the pane printed nothing".into() } else { tail.join(" | ") }
}

/// #4845 AC3 — one login's trace: the verb, who, and when (ms + pid keep two
/// logins in the same millisecond apart). Pure.
pub fn login_trace(verb: &str, role: Option<&str>, ms: u128, pid: u32) -> String {
    let who = role.filter(|r| !r.starts_with('-')).unwrap_or("all");
    format!("login-{}-{}-{}-{}", verb, who, ms, pid)
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
    /// #4383 — sudo, for starting a principal as its own Mac account
    sudo: String,
    /// #4924 — launchctl, for session bootstrap isolation
    _launchctl: String,
    /// #4383 — the principal's own account, when it is not the current user
    run_as: Option<String>,
    log_bin: String,
    probe: String,
    services: Vec<(String, String)>,
    service_wait: u64,
    /// #4445 — the relay member-list projection; "none" switches it off
    relay: String,
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
            sudo: envd("AWAKE_SUDO", "sudo"),
            _launchctl: envd("AWAKE_LAUNCHCTL", "/bin/launchctl"),
            run_as: None,
            log_bin: log_bin_for(&root),
            probe: envd("AWAKE_PROBE_BIN", "curl"),
            services: lifecycle::parse_services(&envd("AWAKE_SERVICES", lifecycle::DEFAULT_SERVICES)),
            service_wait: envd("AWAKE_SERVICE_WAIT", "180").parse().unwrap_or(180),
            relay: envd("AWAKE_RELAY_PROJECT", &format!("{}/platform/scripts/buzz-allowlist-project", root)),
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

/// #4786 AC3 — the Mac account the role runs as: read from the principal's
/// own hostAccount at the identity door. Missing or unbound is refused loudly.
fn role_host_account(ctx: &Ctx, role: &str) -> Result<String, String> {
    let url = format!("{}/v1/identity/principals/{}", ctx.api, role);
    let ans = door(ctx, &["-s", "-w", "\n%{http_code}", "--max-time", "5", &url])
        .map_err(|e| format!("curl {url} failed: {e}"))?;
    let (body, code) = ans.trim_end().rsplit_once('\n').unwrap_or(("", ans.trim()));
    if code.trim() != "200" {
        return Err(format!("identity API answered HTTP {} for principal {role}", code.trim()));
    }
    let row: Value = serde_json::from_str(body)
        .map_err(|e| format!("invalid JSON from {url}: {e}"))?;
    let acct = row.get("data")
        .and_then(|d| d.get("hostAccount"))
        .and_then(|h| h.as_str())
        .filter(|h| !h.trim().is_empty())
        .ok_or_else(|| format!("principal {role} has no hostAccount bound in the identity door"))?;
    Ok(acct.to_string())
}

extern "C" { fn dup(fd: i32) -> i32; fn dup2(from: i32, to: i32) -> i32; }

/// This process's terminal, read with stdin inherited (sh() gives a child no stdin,
/// so `tty` there always says "not a tty"). None when there is no terminal.
fn own_tty() -> Option<String> {
    let out = Command::new("tty").stdin(std::process::Stdio::inherit()).output().ok()?;
    let t = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if out.status.success() && t.starts_with("/dev/") { Some(t) } else { None }
}

/// #4933 — while establishing for the wrapper, everything the login prints goes to
/// stderr: the wrapper captures stdout as CHORUS_RUN, so it must hold the run alone.
struct StdoutToStderr(i32);
impl StdoutToStderr {
    /// Err when stdout cannot be set aside: then nothing is printed for the wrapper at all
    fn new() -> Result<Self, String> {
        let _ = std::io::Write::flush(&mut std::io::stdout());
        let saved = unsafe { dup(1) };
        if saved < 0 { return Err("cannot set stdout aside (dup failed)".into()); }
        if unsafe { dup2(2, 1) } < 0 { return Err("cannot send login output to stderr (dup2 failed)".into()); }
        Ok(StdoutToStderr(saved))
    }
}
impl Drop for StdoutToStderr {
    fn drop(&mut self) { let _ = std::io::Write::flush(&mut std::io::stdout()); if self.0 >= 0 { unsafe { dup2(self.0, 1); } } }
}

/// #4933 AC3 — a role establishes its own session. Run by the role's start hook
/// (Claude's SessionStart, agy's hooks.json), as the role's own account: it signs
/// in with the role's own credential and writes the Session through the door, so
/// the door decides (AC2) and every row is the role's. The launcher writes nothing.
/// The account is the OS's answer (`id -un`), never the environment's.
/// With `--run` (the launch wrapper) it also writes the run for this pane and
/// prints only the run's name on stdout, for the runtime's CHORUS_RUN; a pending
/// login is a failure then, so the wrapper never starts a runtime with no session.
fn establish(ctx: &Ctx, role: &str, run_stamp: Option<String>, pane_arg: Option<String>, tty_arg: Option<String>) -> i32 {
    let with_run = run_stamp.is_some();
    // #4933 (review 20:26, Silas 21:27) — the pane and tty come from the pane's own
    // shell, outside sudo (which drops TMUX_PANE and, with use_pty, gives the inner
    // shell a tty of its own, so no equality check can hold). They are taken only with
    // --run: the launch line the launcher wrote passes them; anything else is refused.
    if (pane_arg.is_some() || tty_arg.is_some()) && !with_run {
        eprintln!("chorus-principal: REFUSED — establish {}: --pane/--tty come only from the launch line, with --run", role);
        return 2;
    }
    let pane = pane_arg.or_else(|| env::var("TMUX_PANE").ok()).unwrap_or_default();
    let tty = tty_arg.or_else(own_tty).unwrap_or_default();
    let quiet = if with_run {
        match StdoutToStderr::new() {
            Ok(q) => Some(q),
            Err(why) => { eprintln!("chorus-principal: establish {}: {}; not establishing", role, why); return 1; }
        }
    } else { None };
    let acct = match role_host_account(ctx, role) {
        Ok(a) => a,
        Err(why) => { eprintln!("chorus-principal: REFUSED — {}; no session established.", why); return 2; }
    };
    let me = sh("id", &["-un"]).map(|u| u.trim().to_string()).unwrap_or_default();
    if let Some(why) = lifecycle::establish_refusal(role, &acct, &me) {
        eprintln!("chorus-principal: REFUSED — {}", why);
        return 2;
    }
    // a login after /exit goes back to its own still-open session (#4406), as the role
    let resumed = match ctx.read_state(role) {
        LoginState::Recorded { session, .. } => read_row(ctx, role, "session")
            .filter(|r| row_name(r) == session && r.get("sessionState").and_then(|v| v.as_str()).unwrap_or("open") != "closed")
            .map(|_| { ctx.spine(&["session.resumed", role, &format!("session={}", session)]); LoginState::Recorded { session, pid: None } }),
        _ => None,
    };
    match resumed.map(Ok).unwrap_or_else(|| do_login(ctx, role)) {
        Ok(st) => {
            ctx.write_state(role, &st);
            // #4934 AC2 reopen — both ways in (a fresh login AND a resumed session) name the
            // domains; prod 09:48:40 borg-queen resumed and the event was never written
            if let LoginState::Recorded { session, .. } = &st { establish_domains(ctx, role, session); }
            match &st {
                LoginState::Recorded { session, .. } if with_run => {
                    record_run_stamped(ctx, role, session, &Live { pid: 0, tty: tty.clone(), host: "tmux".into(), pane: pane.clone() }, "pending", run_stamp.as_deref().unwrap_or_default());
                    let run = read_row(ctx, role, "run").filter(|r| r.get("runEndedAt").and_then(|e| e.as_str()).unwrap_or("").is_empty()).map(|r| row_name(&r)).unwrap_or_default();
                    // the run the launch named, or nothing: the runtime starts on that run only (review 20:26)
                    let named = run_name(role, run_stamp.as_deref().unwrap_or_default());
                    if run.is_empty() || !(run == named || run.ends_with(&format!("-{}", named))) {
                        eprintln!("chorus-principal: establish {}: session {} open, but the run {} the launch named was not written (found '{}')", role, session, named, run);
                        return 1;
                    }
                    eprintln!("establish: {}  session {}  run {}  (as {})", role, session, run, acct);
                    drop(quiet);
                    println!("{}", run);
                }
                LoginState::Recorded { session, .. } => {
                    // the run follows once the pane is registered (seen-write records it then)
                    if let Some(l) = live_entries(&ctx.sessions_dir, role, &ctx.ps).into_iter().next() {
                        record_run(ctx, role, session, &l, "pending");
                    }
                    eprintln!("establish: {}  session {}  (as {})", role, session, acct);
                }
                LoginState::Pending { why, .. } if with_run => { eprintln!("chorus-principal: establish {}: login pending — {}", role, why); return 1; }
                LoginState::Pending { why, .. } => eprintln!("establish: {}  pending — {}; the next turn retries", role, why),
                _ => {}
            }
            0
        }
        Err(()) => 1,
    }
}

/// #4934 AC2 — the domains this login establishes (the role's Principal's Permission
/// rows), read through the door on the login's trace and named in one event. To
/// stderr: establish --run keeps stdout for the run name alone.
fn establish_domains(ctx: &Ctx, role: &str, session: &str) {
    let perms = door(ctx, &["-sf", "--max-time", "10", &format!("{}/v1/security/permissions?agent=principal-{}&limit=500", ctx.api, role)])
        .map_err(|e| format!("the door did not answer: {}", e.trim()))
        .and_then(|body| rows::login_domains(&body, role));
    match perms {
        Ok(d) => {
            ctx.spine(&["session.domains.established", role, &format!("session={}", session), &format!("domains={}", d.join(","))]);
            eprintln!("login: {}  domains {}", role, if d.is_empty() { "none".to_string() } else { d.join(" ") });
        }
        Err(why) => ctx.spine(&["session.domains.unread", role, &format!("session={}", session), &format!("why={}", why)]),
    }
}

/// The login (#4202, #4215): token, principal check, Session row. Ok is
/// Recorded or Pending; Err is a refusal (wrong principal), already printed.
fn do_login(ctx: &Ctx, role: &str) -> Result<LoginState, ()> {
    let channel = match rows::session_channel(env::var("CHORUS_SESSION_CHANNEL").ok().as_deref()) {
        Ok(c) => c,
        Err(why) => { eprintln!("chorus-principal: REFUSED — {} for {}; nothing was started.", why, role); return Err(()); }
    };
    let token = sh(&ctx.token_bin, &[role]).map(|t| t.trim().to_string());
    let login = match &token {
        Ok(t) => login_check(role, t, (now_ms() / 1000) as u64),
        Err(e) => Err(format!("no token: {}", e.trim())),
    };
    let login = match login {
        Ok(l) => l,
        Err(why) => match login_posture(Some(&why)) {
            Start::Refuse(w) => {
                eprintln!("chorus-principal: REFUSED — {} for {}", w, role);
                eprintln!("  nothing was started; starting on another principal's credential would file this work as theirs.");
                return Err(());
            }
            _ => {
                ctx.spine(&["session.login.degraded", role, &format!("reason={}", why)]);
                return Ok(LoginState::Pending { why, pid: None });
            }
        },
    };
    let token = token.unwrap_or_default();
    let host_account = match role_host_account(ctx, role) {
        Ok(a) => a,
        Err(why) => {
            eprintln!("chorus-principal: REFUSED — {} for {}; nothing was started.", why, role);
            return Err(());
        }
    };
    let start_id = format!("{:x}", now_ms());
    let (session_name, body) = session_row(role, &login, &host_account, &start_id);
    // #4328 — the row says which role it acts as and when it started
    let body = rows::with_role_and_start(body, role, &iso_utc(now_ms() as u64 / 1000));
    // #4412 — who started it, and where it lives
    let mut body = body;
    body["channel"] = Value::String(channel.into());
    if let Ok(p) = env::var("CHORUS_STARTED_BY") { if !p.is_empty() { body["startedBy"] = Value::String(p); } }
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
    let answer = door(ctx, &["-s", "-w", "\n%{http_code}", "--max-time", "10", "-X", "POST", "-H", "Content-Type: application/json", "-H", &hdr_arg, "--data-binary", &body_arg, &url]).unwrap_or_else(|e| format!("curl failed: {}", e.trim()));
    let _ = fs::remove_file(&hdr);
    let (reply, code) = match answer.trim_end().rsplit_once('\n') { Some((b, c)) => (b.to_string(), c.trim().to_string()), None => (String::new(), answer.trim().to_string()) };
    // #4295 — the stored name is the one the API hands back, not the one sent
    let session_name = stored_name(&reply).unwrap_or(session_name);
    // #4215 — a 409 on a per-start name means something real: ask WHOSE row it is.
    if code == "409" {
        let get = format!("{}/v1/identity/sessions/{}", ctx.api, session_name);
        let existing = door(ctx, &["-s", "--max-time", "10", &get]).unwrap_or_default();
        let mine = format!("principal-{}", role);
        if existing.contains(&mine) {
            eprintln!("chorus-principal: session {} already open and owned by {} — reusing it", session_name, mine);
        } else {
            let owner = existing.split("ownedBy").nth(1).map(|t| t.chars().take(60).collect::<String>()).unwrap_or_else(|| "unknown".into());
            eprintln!("chorus-principal: REFUSED — session {} exists and is NOT yours (ownedBy{})", session_name, owner);
            eprintln!("  nothing was started; starting under someone else's session would log your work as theirs.");
            return Err(());
        }
    } else if !(code == "200" || code == "201") {
        // #4215 — degrade, do not block: the identity was verified above; what
        // failed is the bookkeeping write, and #4295 retries it on its own.
        ctx.spine(&["session.login.degraded", role, &format!("http={}", code), &format!("session={}", session_name)]);
        return Ok(LoginState::Pending { why: format!("the identity API answered HTTP {}", code), pid: None });
    }
    // #4295 — keep the full row: closing it is a whole-row PUT
    let mut saved = body.clone();
    saved["name"] = Value::String(session_name.clone());
    let _ = write_private(&role_id_dir.join("session.row.json"), &saved.to_string());
    ctx.spine(&["session.login", role, &format!("webid={}", login.webid), &format!("jti={}", login.jti), &format!("session={}", session_name), &format!("host_account={}", host_account), &format!("expires_at={}", iso_utc(login.exp))]);
    println!("login: {}  webid {}  jti {}  session {}  recorded yes", role, login.webid, login.jti, session_name);
    close_superseded(ctx, role, channel, &session_name);
    Ok(LoginState::Recorded { session: session_name, pid: None })
}

/// The row name the API stored, from its create reply (data.name).
fn stored_name(reply: &str) -> Option<String> {
    let v: Value = serde_json::from_str(reply).ok()?;
    v.get("data").and_then(|d| d.get("name")).and_then(|n| n.as_str()).filter(|n| !n.is_empty()).map(String::from)
}

/// Services first (bounded, counted down), then the login. A service still
/// down at the bound is a PENDING login, never a refusal to start.
/// #4406 — a session is login to logout (Jeff 2026-09-30). A login while the
/// role's session is still open (it was /exited or restarted, never logged
/// out) goes back to that session; the caller records a new run in it. Only
/// a role with no open session gets a new one.
/// #4412 — where a person signs in; the refusal hands Jeff this one step.
/// #4412 — the one command that restarts CSS (LaunchAgent com.security.css).
const CSS_RESTART: &str = "launchctl kickstart -k gui/$(id -u)/com.security.css";
const CLEARING_SIGN_IN: &str = "https://clearing.lightlifeurbangardens.com";

/// #4412 — Jeff 2026-09-30: "maybe we must bind chorus-principal to a human(s)"
/// / "the login and logout". A role's session starts or ends only while a
/// person is signed in: an open browser Session (his Clearing sign-in) owned by
/// a person principal. Returns that person's bare name. The API not answering
/// is a refusal too: nobody can be shown to be present.
fn person_signed_in(ctx: &Ctx) -> Result<String, String> {
    let url = format!("{}/v1/identity/sessions?channel=browser&limit=1000", ctx.api);
    let answer = door(ctx, &["-s", "-w", "\n%{http_code}", "--max-time", "5", &url]).unwrap_or_default();
    let (body, code) = answer.trim_end().rsplit_once('\n').unwrap_or(("", answer.trim()));
    if code.trim() != "200" { return Err(format!("could not read who is signed in (chorus-api answered {}).\n  fix: agent-state.sh restart chorus-api, then sign in at {}", code.trim(), CLEARING_SIGN_IN)); }
    for who in rows::signed_in_owners(body, &iso_utc(now_ms() as u64 / 1000)) {
        let purl = format!("{}/v1/identity/principals/{}", ctx.api, who);
        let pa = door(ctx, &["-s", "-w", "\n%{http_code}", "--max-time", "5", &purl]).unwrap_or_default();
        if pa.contains("\"principalKind\":\"person\"") { return Ok(who); }
    }
    // #4412 — Jeff 2026-10-01: "both use cases are valid". A person typing in
    // their own Mac account is present too: not an agent session, and the OS
    // user is a person principal's hostAccount.
    if let Some(p) = person_at_terminal(ctx) { return Ok(p); }
    // #4412 — with CSS down nobody CAN sign in; say that, with its one fix
    if let Some((_, url)) = ctx.services.iter().find(|(n, _)| n == "identity") {
        let code = sh(&ctx.probe, &["-s", "-o", "/dev/null", "-w", "%{http_code}", "--max-time", "2", url]).unwrap_or_default();
        if !lifecycle::is_answering(&code) {
            return Err(format!("nobody can sign in: CSS (identity, {}) is not answering.\n  fix: {}", url, CSS_RESTART));
        }
    }
    Err(format!("nobody is signed in: a role's session starts and ends only while a person is present — signed in to the Clearing, or at their own terminal.\n  fix: sign in at {}, or run this from your own terminal (not inside a role session)", CLEARING_SIGN_IN))
}

/// #4412 — the person at this terminal, if any: never from inside an agent
/// session (CLAUDECODE), and only when this Mac account is a person
/// principal's hostAccount.
fn person_at_terminal(ctx: &Ctx) -> Option<String> {
    if env::var("CLAUDECODE").map(|v| !v.is_empty()).unwrap_or(false) { return None; }
    let user = env::var("USER").ok().filter(|u| !u.is_empty())
        .or_else(|| sh("id", &["-un"]).ok().map(|u| u.trim().to_string()))?;
    let url = format!("{}/v1/identity/principals?limit=1000", ctx.api);
    let body = door(ctx, &["-s", "--max-time", "5", &url]).unwrap_or_default();
    rows::person_for_account(&body, &user)
}

/// #4445 — a principal's nostr key in the graph is not enough: the relay passes
/// on only its members' messages, and its member list is a projection that
/// nothing ran (Abby, 2026-10-07: 48 replies, none in the Clearing). Every login
/// runs it, so a role's key is a member by the time its session starts. Down is
/// loud, never a refusal, like the hooks check.
fn project_relay(ctx: &Ctx, role: &str) {
    if ctx.relay == "none" { return; }
    match sh(&ctx.relay, &[]) {
        Ok(out) => {
            let line = out.lines().filter(|l| !l.trim().is_empty()).last().unwrap_or("done").to_string();
            ctx.spine(&["session.relay.projected", role]);
            println!("relay: {}", line);
        }
        Err(why) => {
            ctx.spine(&["session.relay.failed", role, &format!("reason={}", why.trim())]);
            eprintln!("relay: the relay member list was not updated — {}\n  {}'s messages reach the Clearing only once its key is a member.\n  fix: bash {}", why.trim(), role, ctx.relay);
        }
    }
}

/// #4933 AC3 — what the launcher does in place of a login: wait for the services
/// and mint the launch's stamp. It signs in as nobody and writes no row; the
/// role's wrapper establishes the session and run (fail closed) as the role.
fn launch_ready(ctx: &Ctx, role: &str) -> (LoginState, String) {
    project_relay(ctx, role);
    if let Some(line) = hooks_down(ctx, role) { eprintln!("{}", line); }
    let why = match wait_for_services(ctx, ctx.service_wait, true) { Ok(()) => format!("{} establishes its own session at start", role), Err(w) => w };
    (LoginState::Pending { why, pid: None }, format!("{:x}", now_ms()))
}

/// #4933 AC3 — the launcher reads the role's session back through the door (a
/// GET, never a write): the run the launch named, open, and its session.
fn established(ctx: &Ctx, role: &str, run: &str, pid: Option<u64>) -> LoginState {
    let url = format!("{}/v1/identity/sessionruns/{}", ctx.api, run);
    let ans = door(ctx, &["-s", "-w", "\n%{http_code}", "--max-time", "5", &url]).unwrap_or_default();
    let (body, code) = ans.trim_end().rsplit_once('\n').unwrap_or(("", ans.trim()));
    let row: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let session = row.get("data").and_then(|d| d.get("runOf")).and_then(|s| s.as_str()).unwrap_or("").to_string();
    if code.trim() == "200" && !session.is_empty() { LoginState::Recorded { session, pid } }
    else { LoginState::Pending { why: format!("{}'s run {} is not established (HTTP {})", role, run, code.trim()), pid } }
}


/// #4409 — the hooks daemon answers on a unix socket, not a URL, so it is not
/// in AWAKE_SERVICES. Down is loud, never a refusal: the role starts and Jeff
/// decides. AWAKE_HOOKS_SOCKET overrides the path; "none" skips the check.
fn hooks_down(ctx: &Ctx, role: &str) -> Option<String> {
    let path = envd("AWAKE_HOOKS_SOCKET", &format!("{}/.chorus/run/chorus-hooks.sock", env::var("HOME").unwrap_or_default()));
    if path == "none" || std::os::unix::net::UnixStream::connect(&path).is_ok() { return None; }
    ctx.spine(&["session.login.degraded", role, "reason=hooks-daemon-down"]);
    Some(format!("hooks: the hooks daemon is not answering ({}); gates and nudges are off until it is.\n  fix: launchctl kickstart -k gui/$(id -u)/com.chorus.hooks", path))
}

fn pending_line(role: &str, why: &str) -> String {
    format!("login: {}  pending — {}. {} is starting and logs itself in when that answers; nothing for you to do.", role, why, role)
}


fn came_of(st: &LoginState) -> Came {
    match st { LoginState::Recorded { .. } => Came::LoggedIn, LoginState::Pending { why, .. } => Came::Pending(why.clone()), _ => Came::Pending("no login".into()) }
}

fn attach_to(ctx: &Ctx, role: &str) {
    let s = Ctx::tmux_session(role);
    if env::var("TMUX").is_ok() { let _ = sh(&ctx.tmux, &["switch-client", "-t", &s]); }
    else { let _ = Command::new(&ctx.tmux).args(["attach", "-t", &s]).status(); }
}

/// #4777 — the inner command run inside `sudo -n -u <acct> -H bash -c` when a
/// principal runs as its own Mac account (#4383). Exports `CHORUS_SESSIONS_DIR`
/// so `SessionStart` registers `<role>-<pid>.json` in the shared sessions
/// directory read by `chorus-principal` and `nudge-health-check.sh`. #4931: runs the
/// role's power-on self test (role-post) as the role before the session starts, so
/// every start writes the result the five-role table reads (in the background, so a slow
/// identity door or a refusal message to Jeff never holds up the start); and no longer exports a
/// token from ~/.chorus/secrets/claude.key, which overrode every fresh login.
pub fn own_account_inner_cmd(
    role_dir: &str,
    root: &str,
    own_dir: &str,
    own_token: &str,
    sessions_dir: &str,
    role: &str,
    principal_bin: &str,
    run_stamp: &str,
    cmd: &str,
) -> String {
    // #4933 AC3 — one wrapper, every runtime: the role's own shell establishes its
    // session and run (as the role, through the door) before the runtime starts,
    // and hands the run on as CHORUS_RUN. Fail closed: no session, no runtime.
    // #4933 AC4 — the login's trace crosses the account boundary (sudo drops the env),
    // so establish's door calls and the door's decision lines join this login's trace
    let clean = |v: &str| -> String { env::var(v).unwrap_or_default().chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')).collect() };
    let trace = clean("CHORUS_TRACE_ID");
    // #4412 — who started it crosses the account boundary too (sudo drops the env)
    let started_by = clean("CHORUS_STARTED_BY");
    format!(
        "umask 002 && cd '{}' && source '{}/platform/scripts/chorus-env-setup.sh' && export CHORUS_TRACE_ID='{}' CHORUS_STARTED_BY='{}' CHORUS_IDENTITY_DIR='{}' CHORUS_SESSION_TOKEN_FILE='{}' CHORUS_SESSIONS_DIR='{}' CHORUS_OUTBOX_DIR=\"$HOME/.chorus/outbox\" CHORUS_ROLE='{}' GIT_CONFIG_COUNT='1' GIT_CONFIG_KEY_0='safe.directory' GIT_CONFIG_VALUE_0='*' ENABLE_CLAUDEAI_MCP_SERVERS='false' CLAUDE_CODE_DISABLE_AGENT_VIEW=1 && {{ ( bash '{}/platform/scripts/role-post' >/dev/null 2>&1 & ) ; }} && {{ CHORUS_RUN=$('{}' establish '{}' --run '{}' --pane '__CHORUS_PANE__' --tty '__CHORUS_TTY__') || {{ echo \"chorus-principal: establish failed for {}; not starting {}\" >&2; exit 1; }} ; }} && export CHORUS_RUN && {}",
        role_dir, root, trace, started_by, own_dir, own_token, sessions_dir, role, root, principal_bin, role, run_stamp, role, role, cmd
    )
}

/// #4784 — Resolve a role's own project memory directory (`-roles-<role>/memory`),
/// refusing any path that does not end with `/roles/<role>` or contains `..`.
pub fn role_memory_source_dir(home: &str, role_dir: &str, role: &str) -> Option<PathBuf> {
    let trimmed = role_dir.trim_end_matches('/');
    if role.is_empty()
        || trimmed.contains("..")
        || !trimmed.ends_with(&format!("/roles/{role}"))
    {
        return None;
    }
    Some(projects_dir_for(home, trimmed).join("memory"))
}

/// #4784 — The memory directories handed to `<role>`'s macOS account on login:
/// (1) the shared chorus memory set (`<root>/memory`, e.g. `-Users-jeffbridwell-CascadeProjects-chorus/memory`)
/// and (2) the role's own project memory dir (`-roles-<role>/memory`), while
/// refusing any other role's directory (`-roles-<other>/memory`).
pub fn role_memory_handoff_dirs(
    home: &str,
    root: &str,
    role_dir: &str,
    role: &str,
) -> Option<Vec<(PathBuf, String)>> {
    let own_mem = role_memory_source_dir(home, role_dir, role)?;
    let clean_root = root.trim_end_matches('/');
    if clean_root.is_empty() || clean_root.contains("..") {
        return None;
    }
    let shared_mem = projects_dir_for(home, clean_root).join("memory");
    let shared_slug = clean_root.replace('/', "-");
    let own_slug = role_dir.trim_end_matches('/').replace('/', "-");
    Some(vec![(shared_mem, shared_slug), (own_mem, own_slug)])
}

fn account_uid(acct: &str) -> String {
    if let Ok(out) = Command::new("id").args(["-u", acct]).output() {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) {
                return s;
            }
        }
    }
    format!("$(id -u {})", acct)
}

/// #4928 (#4924) — pre-flight probe arguments for `launchctl asuser`.
/// Must match the pinned rule in /etc/sudoers.d/chorus-role-asuser:
/// `/bin/launchctl asuser <uid> /usr/bin/sudo -n -u <acct> -H bash -c *`
/// Silas pinned form: /bin/launchctl and /usr/bin/sudo are strictly required.
pub fn asuser_probe_args<'a>(uid: &'a str, acct: &'a str) -> [&'a str; 12] {
    ["-n", "/bin/launchctl", "asuser", uid, "/usr/bin/sudo", "-n", "-u", acct, "-H", "bash", "-c", "true"]
}

/// `on <role>`: start it logged in, or, when it is already running, make sure
/// THIS session is logged in and go to its window. Never a second copy.
/// The shell line that starts `cmd` in the role's pane, as the role's own
/// account when one is configured (#4383). Shared by the Claude and agent paths.
fn launch_line(ctx: &Ctx, role: &str, role_dir: &str, token_file: &Path, run_stamp: &str, cmd: &str) -> Result<String, (i32, String)> {
    let launch = match &ctx.run_as {
        // #4931 — fail closed: no account means no start, never a start as the launcher
        None => { let _ = (role_dir, token_file, run_stamp, cmd); return Err((2, format!("{} has no account to run as; refusing to start it as {} (#4931)", role, env::var("USER").unwrap_or_default()))); }
        // #4383 — the session runs as the principal's own account: its token is
        // copied into that account's home, readable by it alone, and files it
        // writes in the werk stay group-writable (umask 002).
        Some(acct) => {
            // the principal's identity lives in its own home: its credentials
            // (and current token) are handed over at login, readable by it alone,
            // and every mint and renewal from then on happens as that account
            let homes = envd("AWAKE_ACCOUNT_HOMES", "/Users");
            let own_dir = format!("{}/{}/.chorus/identity", homes, acct);
            let own = format!("{}/{}/token.cache", own_dir, role);
            for f in ["cred.json", "nostr.json", "token.cache"] {
                let Ok(bytes) = fs::read(PathBuf::from(&ctx.identity_dir).join(role).join(f)) else { continue };
                let dest = format!("{}/{}/{}", own_dir, role, f);
                let put = format!("umask 077 && mkdir -p '{}/{}' && cat > '{}'", own_dir, role, dest);
                let mut child = Command::new(&ctx.sudo).args(["-n", "-u", acct, "-H", "sh", "-c", &put])
                    .stdin(std::process::Stdio::piped()).spawn().map_err(|e| (1, format!("could not hand {} its {}: {}", acct, f, e)))?;
                if let Some(mut i) = child.stdin.take() { let _ = std::io::Write::write_all(&mut i, &bytes); }
                if !child.wait().map(|s| s.success()).unwrap_or(false) {
                    return Err((1, format!("could not hand {} its {}", acct, f)));
                }
            }
            // #4784 — hand the shared chorus memory set and the role's own memory/
            // folder into the role account's ~/.claude/projects/<slug>/memory/
            // (0700, non-clobbering via tar -k) so Claude auto-memory works.
            if let Some(mem_dirs) = role_memory_handoff_dirs(&ctx.home, &ctx.root, role_dir, role) {
                for (src_mem, slug) in mem_dirs {
                    if !src_mem.is_dir() {
                        continue;
                    }
                    let Ok(tar_out) = Command::new("tar")
                        .args(["-cf", "-", "-C", &src_mem.to_string_lossy(), "."])
                        .output()
                    else {
                        continue;
                    };
                    if !tar_out.status.success() || tar_out.stdout.is_empty() {
                        continue;
                    }
                    let dest_mem = format!("{}/{}/.claude/projects/{}/memory", homes, acct, slug);
                    let unpack = format!(
                        "umask 077 && mkdir -p '{}' && tar -xkf - -C '{}' 2>/dev/null || true",
                        dest_mem, dest_mem
                    );
                    if let Ok(mut child) = Command::new(&ctx.sudo)
                        .args(["-n", "-u", acct, "-H", "sh", "-c", &unpack])
                        .stdin(std::process::Stdio::piped())
                        .spawn()
                    {
                        if let Some(mut i) = child.stdin.take() {
                            let _ = std::io::Write::write_all(&mut i, &tar_out.stdout);
                        }
                        let _ = child.wait();
                    }
                }
            }
            // the installed path the role hooks run (never current_exe: a werk-slot copy can shadow it, #4933 defect 41)
            let principal_bin = envd("CHORUS_PRINCIPAL_BIN", &format!("{}/.chorus/bin/chorus-principal", ctx.home));
            let inner = own_account_inner_cmd(role_dir, &ctx.root, &own_dir, &own, &ctx.sessions_dir.to_string_lossy(), role, &principal_bin, run_stamp, cmd);
            let uid = account_uid(acct);
            // #4924 — launchctl asuser isolates the role session from Jeff's Aqua Mach bootstrap
            // namespace (gui/501). Silas pinned form: /bin/launchctl asuser <uid> /usr/bin/sudo -n -u <acct> -H bash -c ...
            // If the sudoers rule is not configured, fall back to today's start line with a warning.
            // #4933 — the pane's id and tty are filled in by the pane's own shell, before
            // sudo drops its environment (these two are the only unescaped expansions)
            let escaped_inner = inner.replace('\\', "\\\\").replace('"', "\\\"").replace('$', "\\$")
                .replace("__CHORUS_PANE__", "$TMUX_PANE").replace("__CHORUS_TTY__", "$(tty)");
            let probe_args = asuser_probe_args(&uid, acct);
            let asuser_check = sh(&ctx.sudo, &probe_args);
            // #4933 — no fallback (Jeff 11-08 11:05): a role that cannot be started
            // isolated in its own session is refused, never started unisolated
            if asuser_check.is_err() {
                return Err((2, format!("{} cannot be started isolated: sudo launchctl asuser {} is not permitted for {}; refusing (never an unisolated start). fix: the asuser rule in platform/config/chorus-role-asuser.sudoers", role, uid, acct)));
            }
            let launch_prefix = probe_args[1..11].join(" ");
            format!(
                "{} -n {} \"{}\"",
                ctx.sudo, launch_prefix, escaped_inner
            )
        }
    };
    Ok(launch)
}

fn on(ctx: &Ctx, role: &str, attach: bool) -> Result<Came, (i32, String)> {
    let tmux_session = Ctx::tmux_session(role);
    let role_dir = ctx.role_dir(role);
    // #4424 — a role on another runtime starts through the supervisor.
    match rows::session_channel(env::var("CHORUS_SESSION_CHANNEL").ok().as_deref()) {
        Ok("agent") => return on_agent(ctx, role, attach),
        Ok(_) => {}
        Err(e) => return Err((1, e)),
    }
    // #4444 reopen 2 — a principal the model assigns a runtime starts on it
    // (Abby on agy); the row says how, nothing here names the runtime
    if let Some((runtime, row)) = rows::runtime_for(&rows::runtime_table(), role) {
        return on_runtime(ctx, role, attach, &runtime, &row);
    }
    // #4444 — a Gemini role (Abby) opens in Gemini's own screen in her pane.
    if rows::pane_runtime(Path::new(&role_dir)) == rows::PaneRuntime::Gemini {
        return on_gemini(ctx, role, attach);
    }

    // 1/4/6 — registry first: one live → check it; two → refuse.
    let live = live_entries(&ctx.sessions_dir, role, &ctx.ps);
    if live.len() >= 2 {
        eprintln!("chorus-principal: REFUSED — {} live sessions for {}; one role, one session:", live.len(), role);
        for l in &live { eprintln!("  pid {}  tty {}  host {}  pane {}", l.pid, l.tty, l.host, l.pane); }
        eprintln!("  run `chorus-principal logout {}` to end both, then `chorus-principal login {}`.", role, role);
        return Err((1, format!("{} live sessions", live.len())));
    }
    if let Some(l) = live.first() {
        let liveness_on = envd("AWAKE_LIVENESS", "1") != "0";
        let mute_window: u64 = envd("AWAKE_MUTE_SECS", "900").parse().unwrap_or(900);
        let age = sh(&ctx.ps, &["-o", "etime=", "-p", &l.pid.to_string()]).ok().and_then(|e| parse_etime(&e));
        let young = too_young_to_judge(age, mute_window);
        if young || awake_verdict(true, ctx.answered(role), liveness_on) == Awake::AlreadyAwake {
            // #4295 — "already awake" is earned by a login too, not only a pid.
            let st = ctx.read_state(role);
            let st = match &st {
                LoginState::Recorded { .. } if lifecycle::login_belongs_to(&st, l.pid) => st.with_pid(l.pid),
                _ => match door_open_row(ctx, role, "identity/sessionruns", "run", "runEndedAt")
                    .map_err(|why| ctx.spine(&["session.row.failed", role, "kind=run", &format!("why={}", why)])).ok().flatten().and_then(|r| r.get("runOf").and_then(|s| s.as_str()).map(String::from)).filter(|s| !s.is_empty()) {
                    // #4933 — read back, never written: the role's own open run names its session
                    Some(session) => LoginState::Recorded { session, pid: Some(l.pid) },
                    None => {
                        // never logged in from here: the role establishes its own session on its next turn
                        println!("{} is running without a session; it establishes its own on its next turn", role);
                        LoginState::Pending { why: format!("{} establishes its own session on its next turn", role), pid: Some(l.pid) }
                    }
                }
            };
            ctx.write_state(role, &st);
            if let LoginState::Pending { why, .. } = &st { println!("{}", pending_line(role, why)); }
            let word = lifecycle::login_word(&st, Some(l.pid));
            mark_running(ctx, role, word);
            println!("{}", proof_line(role, l, word, "already awake"));
            if attach { attach_to(ctx, role); }
            return Ok(came_of(&st));
        }
        let mute_secs: u64 = envd("AWAKE_MUTE_SECS", "900").parse().unwrap_or(900);
        eprintln!("chorus-principal: {} has a live session (pid {}) that has not spoken in {}s — MUTE, replacing it", role, l.pid, mute_secs);
        eprintln!("  a pid and a registry entry are not an answer; ending it so a fresh conversation can start.");
        let _ = sh(&ctx.claude, &["stop", &l.pid.to_string()]);
        let _ = fs::remove_file(ctx.sessions_dir.join(format!("{}-{}.json", role, l.pid)));
    }

    // #4845 AC2 — a Claude role is launched with its MCP servers or not at all, named
    if let Err(why) = rows::mcp_config_ok(Path::new(&role_dir)) {
        eprintln!("chorus-principal: {} NOT started — {}", role, why);
        return Err((1, why));
    }

    // 2/7 — the real conversation (#4215: an unreadable list continues, never refuses).
    let agents = match sh(&ctx.claude, &["agents", "--json", "--cwd", &role_dir]) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("chorus-principal: could not list {}'s background sessions: {}", role, e.trim());
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
    // #4933 — the run is written by the role's own establish ("pending"); its first turn's
    // seen-write records the transcript it continues (#4328), so nothing is stamped here
    let (cmd, how) = if poisoned {
        let id = latest.clone().unwrap_or_default();
        eprintln!("chorus-principal: the last conversation ({}) ends in API refusals — not resuming it", id);
        eprintln!("  starting a FRESH conversation instead; turning the key has to start the car.");
        (ctx.claude.clone(), format!("fresh conversation ({} ends in API refusals)", id))
    } else {
        match &dec.attach {
            // #4337 — never attach. A background session runs in the daemon's
            // pre-warmed process, which carries whichever role's env started the
            // daemon (09-26: Kade ran as Wren). Stop it (the conversation is kept)
            // and resume it here, in a process started from this role's pane.
            Some(id) => {
                let _ = sh(&ctx.claude, &["stop", id]);
                (format!("{} --resume {}", ctx.claude, id), format!("resumed {} in its own pane (its background copy stopped)", id))
            }
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
    // #4933 AC3 — no login here: the role's wrapper establishes it, as the role
    let (st, stamp) = launch_ready(ctx, role);
    let token_file = PathBuf::from(&ctx.identity_dir).join(role).join("token.cache");

    // 1 — the pane, then launch inside it.
    // #4337 — CLAUDE_CODE_DISABLE_AGENT_VIEW=1: no on-demand daemon, no warm
    // spares, no exit handoff to the background (the role settings say the same)
    let launch = launch_line(ctx, role, &role_dir, &token_file, &stamp, &cmd)?;
    let in_own_pane = env::var("TMUX").is_ok() && sh(&ctx.tmux, &["display-message", "-p", "#S"]).map(|s| s.trim() == tmux_session).unwrap_or(false);
    if in_own_pane && attach {
        println!("awake: {}  starting here via {}", role, how);
        let _ = Command::new("bash").arg("-c").arg(&launch).status();
        return Ok(came_of(&st));
    }
    if let Err(e) = start_in_pane(ctx, role, &role_dir, &launch) { eprintln!("chorus-principal: {}", e.1); return Err(e); }

    // 3 — prove it: wait for a live tmux-hosted registry entry.
    let mut found: Option<Live> = None;
    for i in 0..=ctx.wait {
        found = live_entries(&ctx.sessions_dir, role, &ctx.ps).into_iter().next();
        if found.is_some() || i == ctx.wait { break; }
        std::thread::sleep(Duration::from_secs(1));
    }
    // #4328 — `claude attach <id>` joins a background session whose SessionStart
    // fired long ago, so it never registers; 09-26 08:55 Kade and Wren both came
    // up running and "registered NO", with no window. The pane itself is the
    // proof: its claude process, tty and pane id are written as the entry.
    let found = found.or_else(|| register_from_pane(ctx, role, rows::PaneRuntime::Claude));
    let Some(l) = found else {
        eprintln!("awake: {}  registered NO after {}s  via {} — the pane says: {} — look: {} attach -t {}", role, ctx.wait, how, pane_tail(ctx, role), ctx.tmux, tmux_session);
        // #4933 — a role whose sign-in could not happen does not start (Jeff 11-08): say why
        if let LoginState::Pending { why, .. } = &st { if !why.contains("establishes its own session") { eprintln!("awake: {}  not started: its sign-in could not happen — {}", role, why); } }
        return Err((1, format!("did not register in {}s", ctx.wait)));
    };
    // #4337 — a process carrying another role's env is not this role logged in
    if let Some(other) = process_role(ctx, l.pid).filter(|r| r != role) {
        ctx.spine(&["session.wrong_role", role, &format!("pid={}", l.pid), &format!("carries={}", other)]);
        // repair it ourselves once: end that pane and start again (Jeff: "i dont
        // want the 10 steps i need to run when the 1 step command fails")
        if envd("AWAKE_REPAIRING", "0") != "1" {
            eprintln!("awake: {}  pid {} runs as {}, not {} — ending that pane and starting {} again", role, l.pid, other, role, role);
            let _ = sh(&ctx.tmux, &["kill-session", "-t", &tmux_session]);
            let _ = fs::remove_file(ctx.sessions_dir.join(format!("{}-{}.json", role, l.pid)));
            env::set_var("AWAKE_REPAIRING", "1");
            let again = on(ctx, role, attach);
            env::remove_var("AWAKE_REPAIRING");
            return again;
        }
        eprintln!("awake: {}  WRONG ROLE — pid {} still runs as {} after one restart; not logged in. Next: chorus-principal login {}", role, l.pid, other, role);
        return Err((1, format!("pid {} carries {}", l.pid, other)));
    }
    // read back, never written: the run the launch named, established by the role
    // the launch's own reason (e.g. "identity not answering") is kept when the read-back
    // finds nothing, so a cold start still says WHY it is pending
    let st = lifecycle::keep_launch_reason(&st, established(ctx, role, &run_name(role, &stamp), Some(l.pid)));
    ctx.write_state(role, &st);
    let word = lifecycle::login_word(&st, Some(l.pid));
    mark_running(ctx, role, word);
    println!("{}", proof_line(role, &l, word, &how));
    if attach { attach_to(ctx, role); }
    Ok(came_of(&st))
}

/// #4424 — `on` for a role whose channel is agent: log in, write the run (the
/// supervisor's session id), start the runner in the role's pane bound to that
/// run, and prove it by the supervisor answering for exactly that run.
fn on_agent(ctx: &Ctx, role: &str, attach: bool) -> Result<Came, (i32, String)> {
    let tmux_session = Ctx::tmux_session(role);
    let role_dir = ctx.role_dir(role);
    let live = live_entries(&ctx.sessions_dir, role, &ctx.ps);
    if !live.is_empty() {
        eprintln!("chorus-principal: REFUSED — {} already has a Claude session (pid {}); one role, one session.", role, live[0].pid);
        eprintln!("  run `chorus-principal logout {}` first, then start it on its other runtime.", role);
        return Err((1, "a Claude session is live".into()));
    }
    // #4933 AC3 — the run is named here and written by the role's own wrapper
    // (establish --run, fail closed) before the runtime starts; nothing is written here
    let (st, stamp) = launch_ready(ctx, role);
    let run = run_name(role, &stamp);
    if !ctx.has_tmux(role) {
        if let Err(e) = sh(&ctx.tmux, &["new-session", "-d", "-s", &tmux_session, "-c", &role_dir]) { return Err((1, format!("tmux new-session failed: {}", e.trim()))); }
    }
    let agent_bin = envd("CHORUS_AGENT_BIN", &format!("{}/.chorus/bin/chorus-agent", ctx.home));
    let cmd = rows::agent_launch_cmd_checked(&agent_bin, role, &run).map_err(|e| (1, format!("refusing to launch on run {:?}: {}", run, e)))?;
    let token_file = PathBuf::from(&ctx.identity_dir).join(role).join("token.cache");
    let launch = launch_line(ctx, role, &role_dir, &token_file, &stamp, &cmd)?;
    start_in_pane(ctx, role, &role_dir, &launch)?;
    let mut enrolled = false;
    for i in 0..=ctx.wait {
        // #4432 — ask the supervisor of the account the agent runs as (Abby's
        // lives in her home), not the launcher's own socket.
        let status = match rows::agent_socket_for(&envd("AWAKE_ACCOUNT_HOMES", "/Users"), ctx.run_as.as_deref()) {
            Some(sock) => sh("env", &[&format!("CHORUS_AGENT_SOCKET={}", sock), &agent_bin, "status", &run]),
            None => sh(&agent_bin, &["status", &run]),
        };
        enrolled = status.map(|out| rows::agent_enrolled(&out, &run)).unwrap_or(false);
        if enrolled || i == ctx.wait { break; }
        std::thread::sleep(Duration::from_secs(1));
    }
    if !enrolled {
        end_run(ctx, role, "agent-not-enrolled");
        eprintln!("awake: {}  agent run {} NOT enrolled after {}s — the pane says: {} — look: {} attach -t {}", role, run, ctx.wait, pane_tail(ctx, role), ctx.tmux, tmux_session);
        return Err((1, format!("agent run {} did not enroll", run)));
    }
    ctx.spine(&["session.agent.enrolled", role, &format!("run={}", run)]);
    // #4933 — read back, never written: the run this launch named, established by the role
    let st = lifecycle::keep_launch_reason(&st, established(ctx, role, &run, None));
    ctx.write_state(role, &st);
    let word = lifecycle::login_word(&st, None);
    mark_running(ctx, role, word);
    println!("awake: {}  agent run {} enrolled  login {}", role, run, word);
    if attach { attach_to(ctx, role); }
    Ok(came_of(&st))
}

/// #4444 — `on` for a Gemini role: log in, start interactive Gemini in the
/// role's own pane as its account, and prove it by the Gemini process in that
/// pane. The pane is written as the role's live entry and Presence, so nudges
/// are typed into the same screen Jeff watches (pulse's tmux paste, measured
/// 10-06 18:40: a pasted nudge is answered in Gemini's screen).
fn on_gemini(ctx: &Ctx, role: &str, attach: bool) -> Result<Came, (i32, String)> {
    let tmux_session = Ctx::tmux_session(role);
    let role_dir = ctx.role_dir(role);
    let live = live_entries(&ctx.sessions_dir, role, &ctx.ps);
    if let Some(l) = live.first() {
        let st = ctx.read_state(role);
        let word = lifecycle::login_word(&st, Some(l.pid));
        println!("{}", proof_line(role, l, word, "already awake (gemini)"));
        if attach { attach_to(ctx, role); }
        return Ok(came_of(&st));
    }
    // #4933 AC3 — the run is named here and written by the role's own wrapper
    // (establish --run, fail closed) before the runtime starts; nothing is written here
    let (st, stamp) = launch_ready(ctx, role);
    let run = run_name(role, &stamp);
    if !ctx.has_tmux(role) {
        if let Err(e) = sh(&ctx.tmux, &["new-session", "-d", "-s", &tmux_session, "-c", &role_dir]) { return Err((1, format!("tmux new-session failed: {}", e.trim()))); }
    }
    let token_file = PathBuf::from(&ctx.identity_dir).join(role).join("token.cache");
    let agent_bin = envd("CHORUS_AGENT_BIN", &format!("{}/.chorus/bin/chorus-agent", ctx.home));
    let gemini_bin = envd("CHORUS_GEMINI_BIN", &format!("{}/.chorus/runtimes/bin/gemini", ctx.home));
    let allow_rules = envd("CHORUS_ALLOW_RULES_FILE", &format!("{}/.claude/settings.json", ctx.home));
    let cmd = rows::gemini_pane_cmd(&agent_bin, &gemini_bin, &allow_rules, &run).map_err(|e| (1, format!("refusing to launch on run {:?}: {}", run, e)))?;
    let launch = launch_line(ctx, role, &role_dir, &token_file, &stamp, &cmd)?;
    start_in_pane(ctx, role, &role_dir, &launch)?;
    let mut found: Option<Live> = None;
    for i in 0..=ctx.wait {
        found = register_from_pane(ctx, role, rows::PaneRuntime::Gemini);
        if found.is_some() || i == ctx.wait { break; }
        std::thread::sleep(Duration::from_secs(1));
    }
    let Some(l) = found else {
        eprintln!("awake: {}  gemini did not start in {}s — the pane says: {} — look: {} attach -t {}", role, ctx.wait, pane_tail(ctx, role), ctx.tmux, tmux_session);
        return Err((1, format!("gemini did not start in {}s", ctx.wait)));
    };
    ctx.spine(&["session.gemini.started", role, &format!("run={}", run), &format!("pid={}", l.pid)]);
    // #4933 — read back, never written: the run this launch named, established by the role
    let st = lifecycle::keep_launch_reason(&st, established(ctx, role, &run, Some(l.pid)));
    ctx.write_state(role, &st);
    let word = lifecycle::login_word(&st, Some(l.pid));
    mark_running(ctx, role, word);
    println!("{}", proof_line(role, &l, word, "gemini in its own pane"));
    if attach { attach_to(ctx, role); }
    Ok(came_of(&st))
}

/// #4444 reopen 2 — on_gemini generalised: the pane runs the principal's
/// AgentRuntime row (binary, sign-in, hooks file, tools file), read from the model.
fn on_runtime(ctx: &Ctx, role: &str, attach: bool, runtime: &str, row: &Value) -> Result<Came, (i32, String)> {
    let tmux_session = Ctx::tmux_session(role);
    let role_dir = ctx.role_dir(role);
    let live = live_entries(&ctx.sessions_dir, role, &ctx.ps);
    if let Some(l) = live.first() {
        let st = ctx.read_state(role);
        let word = lifecycle::login_word(&st, Some(l.pid));
        println!("{}", proof_line(role, l, word, &format!("already awake ({runtime})")));
        if attach { attach_to(ctx, role); }
        return Ok(came_of(&st));
    }
    // #4933 AC3 — the run is named here and written by the role's own wrapper
    // (establish --run, fail closed) before the runtime starts; nothing is written here
    let (st, stamp) = launch_ready(ctx, role);
    let run = run_name(role, &stamp);
    if !ctx.has_tmux(role) {
        if let Err(e) = sh(&ctx.tmux, &["new-session", "-d", "-s", &tmux_session, "-c", &role_dir]) { return Err((1, format!("tmux new-session failed: {}", e.trim()))); }
    }
    let token_file = PathBuf::from(&ctx.identity_dir).join(role).join("token.cache");
    let bin_dir = envd("CHORUS_BIN", &format!("{}/.chorus/bin", ctx.home));
    let hooks = rows::runtime_hooks_file(row, runtime, role, &format!("{bin_dir}/chorus-hook-shim"), &format!("{bin_dir}/chorus-principal")).map_err(|e| (1, e))?;
    let mcp = rows::runtime_mcp_file(&format!("{bin_dir}/chorus-runner"), role, &ctx.root);
    let hooks_run_dir = envd("CHORUS_HOOKS_RUN_DIR", &format!("{}/.chorus/run-agents", ctx.home));
    let cmd = rows::runtime_pane_cmd(row, runtime, &bin_dir, &hooks_run_dir, &run, &hooks, &mcp).map_err(|e| (1, e))?;
    let launch = launch_line(ctx, role, &role_dir, &token_file, &stamp, &cmd)?;
    start_in_pane(ctx, role, &role_dir, &launch)?;
    let binary = row.get("runtimeBinary").and_then(|b| b.as_str()).unwrap_or("").to_string();
    let mut found: Option<Live> = None;
    for i in 0..=ctx.wait {
        found = register_from_pane_matching(ctx, role, &|c: &str| !binary.is_empty() && c.contains(binary.as_str()));
        if found.is_some() || i == ctx.wait { break; }
        std::thread::sleep(Duration::from_secs(1));
    }
    let Some(l) = found else {
        eprintln!("awake: {}  {} did not start in {}s — the pane says: {} — look: {} attach -t {}", role, runtime, ctx.wait, pane_tail(ctx, role), ctx.tmux, tmux_session);
        return Err((1, format!("{} did not start in {}s", runtime, ctx.wait)));
    };
    // #4851 side fix (Jeff 11-01 18:17: BQ said "awake … logged in" then quit a
    // second later, and nothing said why) — a runtime that dies in its first
    // seconds is not a login: say so with the pane's last lines.
    if let Err(why) = settled(ctx, l.pid) {
        eprintln!("awake: {}  {} started (pid {}) then {} — the pane says: {} — look: {} attach -t {}", role, runtime, l.pid, why, pane_tail(ctx, role), ctx.tmux, tmux_session);
        return Err((1, format!("{} exited right after starting", runtime)));
    }
    ctx.spine(&["session.runtime.started", role, &format!("runtime={runtime}"), &format!("run={}", run), &format!("pid={}", l.pid)]);
    // #4933 — read back, never written: the run this launch named, established by the role
    let st = lifecycle::keep_launch_reason(&st, established(ctx, role, &run, Some(l.pid)));
    ctx.write_state(role, &st);
    let word = lifecycle::login_word(&st, Some(l.pid));
    mark_running(ctx, role, word);
    println!("{}", proof_line(role, &l, word, &format!("{runtime} in its own pane")));
    if attach { attach_to(ctx, role); }
    Ok(came_of(&st))
}

// ------------------------------------------------ the rows a login keeps (#4328)

/// POST (name None) or PUT (name Some) one row through the generated API as
/// the role, the token passed as a 0600 header FILE, never an argv. Returns
/// (http code, reply body).
fn api_send(ctx: &Ctx, role: &str, route: &str, name: Option<&str>, body: &Value, tag: &str) -> (String, String) {
    let (method, url) = match name { Some(n) => ("PUT", format!("{}/v1/{}/{}", ctx.api, route, n)), None => ("POST", format!("{}/v1/{}", ctx.api, route)) };
    api_call(ctx, role, method, &url, body, tag)
}

/// #4934 AC1 — change only `body`'s fields on the row (PATCH). The row the door holds
/// is the record; nothing is cached here. A refusal is named, as put_row names it.
fn patch_row(ctx: &Ctx, role: &str, route: &str, kind: &str, name: &str, body: &Value) -> bool {
    let (code, reply) = api_call(ctx, role, "PATCH", &format!("{}/v1/{}/{}", ctx.api, route, name), body, kind);
    if ok_code(&code) { return true; }
    ctx.spine(&["session.row.failed", role, &format!("kind={}", kind), &format!("row={}", name), &format!("http={}", code), &format!("why={}", rows::refusal_reason(&reply))]);
    false
}

fn api_call(ctx: &Ctx, role: &str, method: &str, url: &str, body: &Value, tag: &str) -> (String, String) {
    let Ok(token) = sh(&ctx.token_bin, &[role]) else { return ("no-token".into(), String::new()) };
    let dir = PathBuf::from(&ctx.identity_dir).join(role);
    let _ = fs::create_dir_all(&dir);
    let hdr = dir.join(format!("{}.hdr", tag));
    let body_path = dir.join(format!("{}.body", tag));
    if write_private(&hdr, &format!("Authorization: Bearer {}\n", token.trim())).and_then(|_| write_private(&body_path, &body.to_string())).is_err() {
        return ("no-file".into(), String::new());
    }
    let hdr_arg = format!("@{}", hdr.display());
    let body_arg = format!("@{}", body_path.display());
    let answer = door(ctx, &["-s", "-w", "\n%{http_code}", "--max-time", "10", "-X", method, "-H", "Content-Type: application/json", "-H", &hdr_arg, "--data-binary", &body_arg, &url]).unwrap_or_default();
    let _ = fs::remove_file(&hdr);
    match answer.trim_end().rsplit_once('\n') { Some((b, c)) => (c.trim().to_string(), b.to_string()), None => (answer.trim().to_string(), String::new()) }
}

fn ok_code(c: &str) -> bool { matches!(c, "200" | "201" | "204") }
fn row_path(ctx: &Ctx, role: &str, kind: &str) -> PathBuf { PathBuf::from(&ctx.identity_dir).join(role).join(format!("{}.row.json", kind)) }
/// #4895 (code review) — the Presence as the door holds it now, for a write. Two accounts
/// write it from their own cached copy (the role's turn as the role account; the sweep and
/// `undelivered` as jeffbridwell), and a PUT sends the whole row: a stale copy put back
/// would undo the other writer's pane or reachability. The fields another writer moves
/// are taken from the door; an unreadable door writes nothing (never a guess).
fn presence_now(ctx: &Ctx, role: &str) -> Option<Value> {
    let cached = read_row(ctx, role, "presence")?;
    let name = row_name(&cached);
    let url = format!("{}/v1/identity/presences/{}", ctx.api, name);
    let read = door(ctx, &["-sf", "--max-time", "3", &url])
        .and_then(|b| serde_json::from_str::<Value>(&b).map_err(|e| format!("not JSON: {e}")))
        .and_then(|v| v.get("data").cloned().ok_or_else(|| "no data".to_string()))
        .and_then(|d| rows::with_door_presence(cached, &d).ok_or_else(|| "data is not a row".to_string()));
    // code review (run 6): a write skipped because the door's row could not be read says so,
    // as the PUT it replaces did (session.row.failed), never silently
    read.map_err(|why| {
        eprintln!("chorus-principal: {role}'s Presence {name} not written: the door's current row could not be read ({why})");
        ctx.spine(&["presence.door.unread", role, &format!("presence={name}"), &format!("why={why}")]);
    }).ok()
}

fn read_row(ctx: &Ctx, role: &str, kind: &str) -> Option<Value> { fs::read_to_string(row_path(ctx, role, kind)).ok().and_then(|t| serde_json::from_str(&t).ok()) }
fn save_row(ctx: &Ctx, role: &str, kind: &str, v: &Value) { let _ = write_private(&row_path(ctx, role, kind), &v.to_string()); }
fn row_name(v: &Value) -> String { v.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string() }

/// Create one row and keep it (with the name the API stored) for later PUTs.
fn create_row(ctx: &Ctx, role: &str, route: &str, kind: &str, mut body: Value) -> Option<String> {
    let (code, reply) = api_send(ctx, role, route, None, &body, kind);
    if !ok_code(&code) {
        let why = rows::refusal_reason(&reply);
        ctx.spine(&["session.row.failed", role, &format!("kind={}", kind), &format!("http={}", code), &format!("why={}", why)]);
        eprintln!("chorus-principal: the {} row for {} was not written (HTTP {}: {})", kind, role, code, why);
        return None;
    }
    let name = stored_name(&reply).unwrap_or_else(|| row_name(&body));
    body["name"] = Value::String(name.clone());
    save_row(ctx, role, kind, &body);
    Some(name)
}

/// #4944 — write only the fields our code changed on a saved row (PATCH), then keep the
/// new copy. The cache is never sent back whole: once the door's row moves on, a stale
/// whole row is refused (the Session 422s of 11-09/10). Nothing changed is no write.
fn patch_changed(ctx: &Ctx, role: &str, route: &str, kind: &str, before: &Value, after: &Value) -> bool {
    let Some((name, body)) = rows::changed_fields(before, after) else { return true };
    if !patch_row(ctx, role, route, kind, &name, &body) { return false; }
    save_row(ctx, role, kind, after);
    true
}

/// Replace one saved row and keep the new copy.
fn put_row(ctx: &Ctx, role: &str, route: &str, kind: &str, row: &Value) -> bool {
    let name = row_name(row);
    let (code, reply) = api_send(ctx, role, route, Some(&name), row, kind);
    if ok_code(&code) { save_row(ctx, role, kind, row); true } else {
        ctx.spine(&["session.row.failed", role, &format!("kind={}", kind), &format!("row={}", name), &format!("http={}", code), &format!("why={}", rows::refusal_reason(&reply))]);
        false
    }
}

/// #4933 — the role's open run or presence, read from the door: the one source for
/// ending a run (the rows live in the role's own account, written by establish, so a
/// local copy in whoever runs this is not the record). The newest open row of `route`
/// whose name is `<role>-<kind>-…` (the names establish mints).
fn door_open_row(ctx: &Ctx, role: &str, route: &str, kind: &str, open_field: &str) -> Result<Option<Value>, String> {
    let url = if open_field.is_empty() { format!("{}/v1/{}?limit=5000", ctx.api, route) } else { format!("{}/v1/{}?{}.absent=true&limit=5000", ctx.api, route, open_field) };
    let body = door(ctx, &["-sf", "--max-time", "10", &url]).map_err(|e| format!("the door did not answer: {}", e.trim()))?;
    serde_json::from_str::<Value>(&body).map_err(|e| format!("not JSON: {e}"))?;
    // open, checked here too, not only by the door's filter; fit to PUT back (#4934 AC1)
    Ok(rows::open_row_from_listing(&body, role, route, kind, open_field))
}

/// End the role's open run (and retire its presence), read from the door.
fn end_run(ctx: &Ctx, role: &str, reason: &str) {
    // the door is the only source (Jeff 11-08: no fallbacks): never a local copy first
    // a door that cannot be read is named, never a silent skip (#4895)
    let run = door_open_row(ctx, role, "identity/sessionruns", "run", "runEndedAt")
        .map_err(|why| ctx.spine(&["session.row.failed", role, "kind=run", &format!("why={}", why)])).ok().flatten();
    if let Some(run) = run {
        if run.get("runEndedAt").and_then(|e| e.as_str()).unwrap_or("").is_empty() {
            // #4934 AC1 — only what ending changes; the listing row put back whole was 422
            if let Some((name, body)) = rows::ended_run(run, &iso_utc(now_ms() as u64 / 1000), reason).and_then(|e| rows::patch_of(&e, &["runEndedAt", "endReason"])) {
                if patch_row(ctx, role, "identity/sessionruns", "run", &name, &body) {
                    ctx.spine(&["session.run.ended", role, &format!("run={}", name), &format!("reason={}", reason)]);
                }
            }
        }
    }
    let presence = door_open_row(ctx, role, "identity/presences", "presence", "")
        .map_err(|why| ctx.spine(&["presence.door.unread", role, &format!("why={}", why)])).ok().flatten();
    if let Some(p) = presence {
        if p.get("reachability").and_then(|r| r.as_str()) != Some("unreachable") {
            if let Some((name, body)) = rows::gone_presence(p).and_then(|g| rows::patch_of(&g, &["reachability"])) {
                if patch_row(ctx, role, "identity/presences", "presence", &name, &body) {
                    ctx.spine(&["session.presence.retired", role, &format!("presence={}", name), &format!("reason={}", reason)]);
                }
            }
        }
    }
}

/// #4344 — the role's credential files as Credential rows. #4447: written by
/// the identity owner (`writer`) with `chorus-principal credentials <role>`,
/// never by a login as the role — the security graph is closed to role grants
/// (#4204), and a login rewrote these every time because the token rotates.
/// #4455 — who writes a role's Credential rows: the caller's own CHORUS_ROLE.
/// No role set is refused (exit 2), never defaulted to a teammate (#3959).
pub fn credentials_writer(role: Option<String>) -> Result<String, i32> {
    match role.filter(|w| !w.trim().is_empty()) {
        Some(w) => Ok(w),
        None => { eprintln!("chorus-principal: REFUSED — credentials needs CHORUS_ROLE (the writer); none is set"); Err(2) }
    }
}

fn record_credentials(ctx: &Ctx, role: &str, writer: &str) -> i32 {
    let dir = PathBuf::from(&ctx.identity_dir).join(role);
    let mtimes: Vec<(String, String)> = ["cred.json", "nostr.json", "token.cache"].iter().filter_map(|f| {
        let secs = fs::metadata(dir.join(f)).ok()?.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs();
        Some((f.to_string(), iso_utc(secs)))
    }).collect();
    let shown = ctx.identity_dir.replacen(&ctx.home, "~", 1);
    let mut failed = 0;
    for row in rows::credential_rows(role, &shown, &mtimes) {
        let kind = format!("cred-{}", row["credentialKind"].as_str().unwrap_or(""));
        if read_row(ctx, role, &kind).as_ref() == Some(&row) { continue; }
        let (code, reply) = api_send(ctx, writer, "security/credentials", None, &row, &kind);
        let ok = if code == "409" { put_row(ctx, writer, "security/credentials", &kind, &row) } else { ok_code(&code) };
        if ok { save_row(ctx, role, &kind, &row); println!("credential {} written", row_name(&row)); }
        else { ctx.spine(&["session.row.failed", role, "kind=credential", &format!("http={}", code), &format!("why={}", rows::refusal_reason(&reply))]); eprintln!("credential {} refused: {}", row_name(&row), rows::refusal_reason(&reply)); failed += 1; }
    }
    if failed > 0 { 1 } else { 0 }
}

/// #4342 — one Conversation row per conversation id: written once, never twice.
fn ensure_conversation(ctx: &Ctx, role: &str, run: &str, conversation: &str) {
    let Some(body) = rows::conversation_row(role, run, conversation) else { return };
    let have = read_row(ctx, role, "conversation");
    if have.as_ref().and_then(|h| h.get("conversationId")).and_then(|c| c.as_str()) == Some(conversation)
        && have.as_ref().and_then(|h| h.get("conversationOf")).and_then(|c| c.as_str()) == Some(run) { return; }
    // a resumed conversation (claude -c, --resume) is the same transcript in a new run:
    // the row already exists, so it moves to the run that speaks in it now
    let (code, reply) = api_send(ctx, role, "memory/conversations", None, &body, "conversation");
    let name = if code == "409" {
        let n = row_name(&body);
        if !put_row(ctx, role, "memory/conversations", "conversation", &body) { return; }
        n
    } else if ok_code(&code) {
        let n = stored_name(&reply).unwrap_or_else(|| row_name(&body));
        let mut saved = body.clone(); saved["name"] = Value::String(n.clone());
        save_row(ctx, role, "conversation", &saved);
        n
    } else {
        ctx.spine(&["session.row.failed", role, "kind=conversation", &format!("http={}", code), &format!("why={}", rows::refusal_reason(&reply))]);
        return;
    };
    ctx.spine(&["session.conversation.recorded", role, &format!("run={}", run), &format!("conversation={}", name)]);
}

/// #4745 — one role's run is recorded by one process at a time. Three hooks at
/// one start each read "no live run" before any had saved one, and wrote three
/// live runs in 600 ms (wren, 2026-10-25 15:21:51). A held lock older than
/// `stale` is a dead holder's and is taken over. Each holder signs the lock
/// with its own token and releases only a lock that still carries it, so a slow
/// holder that was taken over never removes its successor's lock (Kade, #4745 review).
pub struct RunLock { dir: PathBuf, token: String }
impl Drop for RunLock {
    fn drop(&mut self) {
        let owner = self.dir.join("owner");
        if fs::read_to_string(&owner).ok().as_deref() == Some(self.token.as_str()) {
            let _ = fs::remove_file(&owner);
            let _ = fs::remove_dir(&self.dir);
        }
    }
}
fn clear_lock(lock: &Path) { let _ = fs::remove_file(lock.join("owner")); let _ = fs::remove_dir(lock); }
pub fn run_lock(dir: &Path, wait: Duration, stale: Duration) -> Option<RunLock> {
    let _ = fs::create_dir_all(dir);
    let lock = dir.join("run.lock");
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let token = format!("{}-{:x}-{}", std::process::id(), now_ms(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    let until = std::time::Instant::now() + wait;
    loop {
        if fs::create_dir(&lock).is_ok() {
            if fs::write(lock.join("owner"), &token).is_ok() { return Some(RunLock { dir: lock, token }); }
            clear_lock(&lock);
        }
        let age = fs::metadata(&lock).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok());
        if age.map(|a| a > stale).unwrap_or(false) { clear_lock(&lock); continue; }
        if std::time::Instant::now() >= until { return None; }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A recorded login's run: the previous live run ends as a restart, then this
/// run, its presence and its boot context are written.
fn record_run(ctx: &Ctx, role: &str, session: &str, l: &Live, conversation: &str) {
    record_run_stamped(ctx, role, session, l, conversation, &format!("{:x}", now_ms()))
}

/// #4933 AC3 — the run a launch will carry: the launcher mints only this NAME (no
/// write); the role's own `establish --run <stamp>` writes the row under it.
pub fn run_name(role: &str, stamp: &str) -> String { slug(&format!("{}-run-{}", role, stamp)) }

fn record_run_stamped(ctx: &Ctx, role: &str, session: &str, l: &Live, conversation: &str, stamp: &str) {
    let dir = PathBuf::from(&ctx.identity_dir).join(role);
    let Some(_held) = run_lock(&dir, Duration::from_secs(20), Duration::from_secs(60)) else {
        ctx.spine(&["session.row.failed", role, "kind=run", "why=another process is recording this role's run"]);
        return;
    };
    let previous = read_row(ctx, role, "run").filter(|r| r.get("runEndedAt").and_then(|e| e.as_str()).unwrap_or("").is_empty()).map(|r| row_name(&r));
    if previous.is_some() { end_run(ctx, role, "restart"); }
    let started = iso_utc(now_ms() as u64 / 1000);
    let run_body = rows::run_row(role, &run_name(role, stamp), session, conversation, &started, previous.as_deref());
    let Some(run) = create_row(ctx, role, "identity/sessionruns", "run", run_body) else { return };
    ensure_conversation(ctx, role, &run, conversation);
    let host_account = match role_host_account(ctx, role) {
        Ok(a) => a,
        Err(why) => {
            eprintln!("chorus-principal: the presence row for {} was not written ({})", role, why);
            ctx.spine(&["session.row.failed", role, "kind=presence", &format!("why={}", why)]);
            return;
        }
    };
    let presence = create_row(ctx, role, "identity/presences", "presence", rows::presence_row(role, &slug(&format!("{}-presence-{}", role, stamp)), &run, &l.pane, &l.tty, &host_account, rows::session_channel(env::var("CHORUS_SESSION_CHANNEL").ok().as_deref()).unwrap_or("pane")));
    let context = create_row(ctx, role, "memory/contexts", "context", rows::boot_context_row(role, &slug(&format!("{}-boot-{}", role, stamp)), &run, &started));
    ctx.spine(&["session.run.recorded", role, &format!("session={}", session), &format!("run={}", run),
        &format!("previous={}", previous.unwrap_or_default()), &format!("presence={}", presence.unwrap_or_default()), &format!("context={}", context.unwrap_or_default())]);
}

/// `seen <role>` — the UserPromptSubmit hook. Reads the turn from stdin and,
/// when a write is due, hands it to a detached `seen-write` so the prompt is
/// never held for a network call.
/// #4385 — a revoked principal's session ends on its next turn. Revoked means
/// the identity API no longer lets it log in (its Principal row is gone, or is
/// no longer an agent). An API that does not answer revokes nothing.
fn revoked_now(ctx: &Ctx, role: &str) -> Option<String> {
    let url = format!("{}/v1/identity/principals/{}", ctx.api, role);
    let answer = door(ctx, &["-s", "-w", "\n%{http_code}", "--max-time", "3", &url]).unwrap_or_default();
    let (body, code) = answer.trim_end().rsplit_once('\n').unwrap_or(("", answer.trim()));
    let code = code.trim();
    if code != "200" && code != "404" { return None; }
    rows::login_verdict(role, code, body, &ctx.api).err()
}

fn end_revoked(ctx: &Ctx, role: &str, why: &str) {
    let st = ctx.read_state(role);
    end_run(ctx, role, "revoked");
    let session = match &st {
        LoginState::Recorded { session, .. } => { let _ = close_row(ctx, role, session); session.clone() }
        _ => String::new(),
    };
    ctx.write_state(role, &LoginState::Closed);
    let _ = write_private(&PathBuf::from(&ctx.identity_dir).join(role).join("revoked"), why);
    ctx.spine(&["session.revoked", role, &format!("session={}", session), &format!("why={}", rows::refusal_reason(why))]);
}

fn seen(ctx: &Ctx, role: &str) -> i32 {
    let mut input = String::new();
    let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut input);
    // #4895 (code review) — at the turn's end only the time is stamped: no revocation
    // check (exit 2 on a Stop hook means "keep going", and its curl would sit on every
    // turn end) and no second read of a prompt already handled when it arrived
    if rows::is_turn_end(&input) { return stamp_seen(ctx, role, &rows::turn_facts(&input).0, "turn", false); }
    // #4385 — revocation bites on the next turn, not at the next login: the
    // turn is refused (exit 2 stops the prompt), and it says why once
    let marker = PathBuf::from(&ctx.identity_dir).join(role).join("revoked");
    if marker.exists() {
        eprintln!("chorus-principal: {}'s session was ended: its principal is revoked. Nothing more runs here until it is restored and it logs in again.", role);
        return 2;
    }
    if !matches!(ctx.read_state(role), LoginState::Closed) {
        if let Some(why) = revoked_now(ctx, role) {
            end_revoked(ctx, role, &why);
            eprintln!("chorus-principal: {}'s principal is revoked ({}). This session is ended as revoked; the turn did not run.", role, why);
            return 2;
        }
    }
    let (conv, delivered) = rows::turn_facts(&input);
    // #4339 — Jeff spoke: this turn always writes, so the session says when
    let jeff = rows::speaker(&input) == rows::Speaker::Jeff;
    // #4340 — keep what actually arrived, so the projector can tell a message
    // that landed cut from one that landed whole (pulse cannot see that)
    log_received(ctx, role, &input);
    let flag = if delivered { "delivered" } else if jeff { "jeff" } else { "turn" };
    stamp_seen(ctx, role, &conv, flag, delivered || jeff)
}

/// The rate-limited stamp both turn edges share: due → a detached `seen-write`.
fn stamp_seen(ctx: &Ctx, role: &str, conv: &str, flag: &str, force: bool) -> i32 {
    let conv = conv.to_string();
    let at = PathBuf::from(&ctx.identity_dir).join(role).join("seen.at");
    let now = now_ms() as u64 / 1000;
    let last = fs::read_to_string(&at).ok().and_then(|t| t.trim().parse::<u64>().ok());
    let every: u64 = envd("AWAKE_SEEN_EVERY", "60").parse().unwrap_or(60);
    if !rows::seen_due(last, now, every, force) { return 0; }
    let _ = fs::write(&at, now.to_string());
    if envd("AWAKE_SEEN_SYNC", "0") == "1" { return seen_write(ctx, role, &conv, flag); }
    if let Ok(me) = env::current_exe() {
        // #4520 reopen 2 — its own process group: Gemini's hook runner ends the
        // hook's group when the hook returns, and the write died with it
        use std::os::unix::process::CommandExt;
        let _ = Command::new(me).args(["seen-write", role, &conv, flag]).process_group(0).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
    }
    let _ = flag;
    0
}

fn seen_write(ctx: &Ctx, role: &str, conv: &str, flag: &str) -> i32 {
    let delivered = flag == "delivered";
    // after a logout the saved row is closed; a late turn must not reopen it
    if matches!(ctx.read_state(role), LoginState::Closed) { return 0; }
    // #4933 AC3 — no session yet (the start's establish failed, or a role running from
    // before): the role establishes its own here, as itself; establish refuses any other account
    if !matches!(ctx.read_state(role), LoginState::Recorded { .. }) && read_row(ctx, role, "session").is_none() {
        // the per-turn hook stays fast (review 20:26): at most one establish a minute
        let tried = PathBuf::from(&ctx.identity_dir).join(role).join("establish.tried");
        let recent = fs::metadata(&tried).and_then(|m| m.modified()).ok()
            .and_then(|t| t.elapsed().ok()).map(|age| age.as_secs() < envd("AWAKE_ESTABLISH_EVERY", "60").parse().unwrap_or(60)).unwrap_or(false);
        if recent { return 0; }
        let _ = fs::create_dir_all(tried.parent().unwrap_or(Path::new("/")));
        let _ = fs::write(&tried, "");
        let _ = establish(ctx, role, None, None, None);
        if !matches!(ctx.read_state(role), LoginState::Recorded { .. }) { return 0; }
    }
    let now = iso_utc(now_ms() as u64 / 1000);
    let cached = read_row(ctx, role, "session");
    // a login from before #4328 carries no role or start: add them on its first turn
    let session = cached.clone().map(|s| {
        if s.get("actsAs").and_then(|a| a.as_str()).unwrap_or("").is_empty() {
            let started = s.get("issuedAt").and_then(|a| a.as_str()).unwrap_or(&now).to_string();
            rows::with_role_and_start(s, role, &started)
        } else { s }
    });
    // #4339 — a prompt from Jeff: the session records who attended it and when
    let session = if flag == "jeff" { session.and_then(|s| rows::attended_session(s, "jeff", &now)) } else { session };
    // #4377 — the login renewed since the row was written: its expiry follows,
    // so a live session never reads as expired (one PUT with the turn's, not two)
    let session = session.map(|s| { let exp = current_login_expiry(ctx, role); rows::renewed_session(s.clone(), &exp).unwrap_or(s) });
    if let (Some(before), Some(s)) = (cached.as_ref(), session.clone().and_then(|s| rows::seen_session(s, &now))) { patch_changed(ctx, role, "identity/sessions", "session", before, &s); }
    // ... and no run: this running session gets its run, presence and boot context now
    let run_live = read_row(ctx, role, "run").map(|r| r.get("runEndedAt").and_then(|e| e.as_str()).unwrap_or("").is_empty()).unwrap_or(false);
    if !run_live {
        if let (LoginState::Recorded { session: sname, .. }, Some(l)) = (ctx.read_state(role), live_entries(&ctx.sessions_dir, role, &ctx.ps).into_iter().next()) {
            record_run(ctx, role, &sname, &l, if conv.is_empty() { "pending" } else { conv });
            return 0;
        }
    }
    if delivered {
        if let Some(p) = presence_now(ctx, role).and_then(|p| rows::delivered_presence(p, &now)) { put_row(ctx, role, "identity/presences", "presence", &p); }
    }
    // #4339 — is his terminal on this role's pane right now
    if let Some(p) = presence_now(ctx, role) {
        let pane = p.get("pane").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let clients = Command::new(&ctx.tmux).args(["list-clients", "-F", "#{client_flags}|#{pane_id}"]).output().ok().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
        if let Some(p) = rows::focused_presence(p, rows::pane_shown(&clients, &pane), &now) { put_row(ctx, role, "identity/presences", "presence", &p); }
    }
    if let Some(r) = read_row(ctx, role, "run").and_then(|r| rows::with_conversation(r, conv)) { put_row(ctx, role, "identity/sessionruns", "run", &r); }
    if let Some(run) = read_row(ctx, role, "run").filter(|r| r.get("runEndedAt").and_then(|e| e.as_str()).unwrap_or("").is_empty()) {
        ensure_conversation(ctx, role, &row_name(&run), conv);
    }
    0
}

/// #4377 — the expiry of the role's current login: the same token the row write
/// will carry (chorus-identity-token renews it when the cached one lapsed),
/// only if it names this role. "" when there is none. Reading token.cache
/// directly saw the lapsed token from before the renewal (demo, 09-27 09:07).
fn current_login_expiry(ctx: &Ctx, role: &str) -> String {
    let Ok(tok) = sh(&ctx.token_bin, &[role]) else { return String::new() };
    match login_check(role, tok.trim(), now_ms() as u64 / 1000) { Ok(l) => iso_utc(l.exp), Err(_) => String::new() }
}

/// "store: session <name> open, acts as role-x, since <t>, last seen <t>".
fn store_line(listing: &str, session: Option<&str>) -> Option<String> {
    let session = session?;
    let v: Value = serde_json::from_str(listing).ok()?;
    let row = v.get("data")?.as_array()?.iter().find(|r| r.get("name").and_then(|n| n.as_str()) == Some(session))?;
    let f = |k: &str| row.get(k).and_then(|x| x.as_str()).filter(|x| !x.is_empty()).unwrap_or("-").to_string();
    Some(format!("store: session {} {}, acts as {}, since {}, last seen {}", session, f("sessionState"), f("actsAs"), f("startedAt"), f("lastSeenAt")))
}

/// Append this turn's prompt to identity/<role>/received.jsonl (last 200 kept).
fn log_received(ctx: &Ctx, role: &str, hook_input: &str) {
    let v: Value = serde_json::from_str(hook_input).unwrap_or(Value::Null);
    let Some(prompt) = v.get("prompt").and_then(|p| p.as_str()) else { return };
    let path = PathBuf::from(&ctx.identity_dir).join(role).join("received.jsonl");
    let mut lines: Vec<String> = fs::read_to_string(&path).unwrap_or_default().lines().map(String::from).collect();
    lines.push(serde_json::json!({"at": iso_utc(now_ms() as u64 / 1000), "text": prompt}).to_string());
    let keep = lines.len().saturating_sub(200);
    let _ = write_private(&path, &(lines[keep..].join("\n") + "\n"));
}

/// (at, text) of every prompt the role received, from the seen hook's log.
fn received_log(ctx: &Ctx, role: &str) -> Vec<(String, String)> {
    let path = PathBuf::from(&ctx.identity_dir).join(role).join("received.jsonl");
    fs::read_to_string(path).unwrap_or_default().lines()
        .filter_map(|l| { let v: Value = serde_json::from_str(l).ok()?; Some((v.get("at")?.as_str()?.to_string(), v.get("text")?.as_str()?.to_string())) }).collect()
}

fn api_list(ctx: &Ctx, route: &str) -> Value {
    serde_json::from_str(&door(ctx, &["-s", "--max-time", "20", &format!("{}/v1/{}?limit=5000", ctx.api, route)]).unwrap_or_default()).unwrap_or(Value::Null)
}

/// `project-messages` — one pass of messages.db into the store: new messages
/// (id past the watermark) and deliveries whose outcome changed since last time.
/// Idempotent: the rows are named by the messages.db id.
fn project_messages(ctx: &Ctx) -> i32 {
    let roles = match agent_roles(ctx) { Ok(r) => r, Err(why) => { eprintln!("chorus-principal: REFUSED — {}", why); return 2; } };
    let db = envd("CHORUS_MESSAGES_DB", &format!("{}/platform/pulse/messages.db", ctx.root));
    let sqlite = envd("AWAKE_SQLITE", "sqlite3");
    let state_path = PathBuf::from(&ctx.identity_dir).join("silas").join("messages-projection.json");
    let mut state: Value = fs::read_to_string(&state_path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(serde_json::json!({}));
    let since = envd("AWAKE_PROJECT_SINCE", "2026-09-26 16:30:00");
    let watermark = state.get("watermark").and_then(|w| w.as_u64()).unwrap_or(0);
    let open: Vec<u64> = state.get("open").and_then(|o| o.as_object()).map(|o| o.keys().filter_map(|k| k.parse().ok()).collect()).unwrap_or_default();
    let open_list = if open.is_empty() { "0".to_string() } else { open.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",") };
    let q = format!("select id,type,\"from\",\"to\",content,created_at,delivery_status,delivered_at,last_delivery_error,trace_id from messages where (id > {} and created_at >= '{}') or id in ({}) order by id limit 500", watermark, since, open_list);
    let out = sh(&sqlite, &["-json", &db, &q]).unwrap_or_default();
    let srcs = msgs::parse_rows(if out.trim().is_empty() { "[]" } else { &out });
    if srcs.is_empty() { println!("project-messages: nothing new"); return 0; }

    let principals: Vec<String> = api_list(ctx, "identity/principals").get("data").and_then(|d| d.as_array()).map(|a| a.iter().filter_map(|p| p.get("name")?.as_str().map(String::from)).collect()).unwrap_or_default();
    let sessions = api_list(ctx, "identity/sessions");
    let runs = api_list(ctx, "identity/sessionruns");
    let presences = api_list(ctx, "identity/presences");
    ensure_channels(ctx);

    let (mut made, mut updated, mut failed) = (0, 0, 0);
    let mut new_mark = watermark;
    // a message that failed must be tried again: the watermark never passes it
    let mut first_failed: Option<u64> = None;
    for src in &srcs {
        // already projected and settled: the query should not return it; never write it twice
        if src.id <= watermark && !open.contains(&src.id) { continue; }
        let key = src.id.to_string();
        let known = state.get("open").and_then(|o| o.get(&key)).cloned();
        let at = msgs::iso(if src.delivered_at.is_empty() { &src.created_at } else { &src.delivered_at });
        let to_p = msgs::principal_of(&src.to, &principals);
        let presence = to_p.as_deref().and_then(|p| msgs::presence_at(&format!("principal-{}", p), &at, &runs, &presences));
        let recv_role = src.to.trim_start_matches("principal-");
        // Kade's review: only prompts that arrived after this message was sent, within 10 minutes
        let received = if roles.iter().any(|r| r == recv_role) { msgs::received_for(&received_log(ctx, recv_role), &msgs::iso(&src.created_at), 600) } else { vec![] };
        let outcome = msgs::outcome(src, &received);
        let message_name = match &known {
            Some(k) => k.get("message").and_then(|m| m.as_str()).unwrap_or("").to_string(),
            None => {
                let from_p = msgs::principal_of(&src.from, &principals);
                let session = from_p.as_deref().and_then(|p| msgs::session_at(&format!("principal-{}", p), &msgs::iso(&src.created_at), &sessions));
                let body = msgs::message_row(src, &principals, session.as_deref());
                let (mut code, reply) = api_send(ctx, "silas", "messages/messages", None, &body, "msg");
                // already there (a retry after a lost reply): idempotent, not a failure
                if code == "409" { code = "200".into(); }
                if !ok_code(&code) { failed += 1; first_failed = Some(first_failed.map_or(src.id, |f| f.min(src.id))); eprintln!("  message {} not written (HTTP {})", src.id, code); continue; }
                made += 1;
                stored_name(&reply).unwrap_or_else(|| row_name(&body))
            }
        };
        let row = msgs::delivery_row(src, &message_name, presence.as_deref(), &outcome);
        let prev_outcome = known.as_ref().and_then(|k| k.get("outcome")).and_then(|o| o.as_str()).map(String::from);
        let delivery_name = if let Some(k) = &known {
            let n = k.get("delivery").and_then(|d| d.as_str()).unwrap_or("").to_string();
            if prev_outcome.as_deref() != Some(outcome.as_str()) {
                let mut r = row.clone(); r["name"] = Value::String(n.clone());
                let (code, _) = api_send(ctx, "silas", "messages/deliveries", Some(&n), &r, "del");
                if ok_code(&code) { updated += 1 } else { failed += 1 }
            }
            n
        } else {
            let (mut code, reply) = api_send(ctx, "silas", "messages/deliveries", None, &row, "del");
            if code == "409" { code = "200".into(); }
            if !ok_code(&code) { failed += 1; eprintln!("  delivery {} not written (HTTP {})", src.id, code); String::new() } else { stored_name(&reply).unwrap_or_else(|| row_name(&row)) }
        };
        if src.id > new_mark { new_mark = src.id; }
        // keep it open while the outcome can still change
        if let Some(o) = state.get_mut("open").and_then(|o| o.as_object_mut()) { o.remove(&key); }
        let young = msgs::age_secs(&msgs::iso(&src.created_at), &iso_utc(now_ms() as u64 / 1000)).map(|a| a < 600).unwrap_or(false);
        if (matches!(outcome.as_str(), "pending" | "queued") || (outcome == "delivered" && young)) && !delivery_name.is_empty() {
            if state.get("open").is_none() { state["open"] = serde_json::json!({}); }
            state["open"][&key] = serde_json::json!({"message": message_name, "delivery": delivery_name, "outcome": outcome});
        }
        if outcome == "truncated" { ctx.spine(&["message.delivery.truncated", &format!("id={}", src.id), &format!("to={}", src.to)]); }
    }
    if let Some(f) = first_failed { new_mark = new_mark.min(f.saturating_sub(1)); }
    state["watermark"] = serde_json::json!(new_mark);
    let _ = fs::create_dir_all(state_path.parent().unwrap_or(Path::new(".")));
    let _ = write_private(&state_path, &state.to_string());
    ctx.spine(&["messages.projected", &format!("made={}", made), &format!("updated={}", updated), &format!("failed={}", failed), &format!("watermark={}", new_mark)]);
    println!("project-messages: {} message(s) written, {} delivery update(s), {} failed, watermark {}", made, updated, failed, new_mark);
    if failed > 0 { 1 } else { 0 }
}

/// The channels that exist today, created once.
fn ensure_channels(ctx: &Ctx) {
    let have: Vec<String> = api_list(ctx, "messages/channels").get("data").and_then(|d| d.as_array()).map(|a| a.iter().filter_map(|c| c.get("channelKind")?.as_str().map(String::from)).collect()).unwrap_or_default();
    for (kind, what) in [("terminal", "typed into a role's tmux pane: Jeff at the keyboard"), ("nudge", "the messages API, delivered into a role's pane by pulse"), ("clearing", "the group chat"),
        // #4424 — a role on a non-Claude runtime, reached through its supervisor
        ("agent", "the agent supervisor, delivered into a supervised runtime's session")] {
        if have.iter().any(|h| h == kind) { continue; }
        let body = serde_json::json!({"name": kind, "label": kind, "comment": format!("{} — {}. #4340.", kind, what), "ownedBy": "principal-silas", "channelKind": kind});
        let (code, _) = api_send(ctx, "silas", "messages/channels", None, &body, "chan");
        if !ok_code(&code) { eprintln!("  channel {} not written (HTTP {})", kind, code); }
    }
}

/// `sweep` — close the sessions still reading open long after their token died.
fn sweep(ctx: &Ctx) -> i32 {
    let listing = door(ctx, &["-s", "--max-time", "20", &format!("{}/v1/identity/sessions?endedAt.absent=true&limit=5000", ctx.api)]).unwrap_or_default(); // #4838: open rows, at the door
    let roles = match agent_roles(ctx) { Ok(r) => r, Err(why) => { eprintln!("chorus-principal: REFUSED — {}", why); return 2; } };
    let mut keep: Vec<String> = roles.iter().filter_map(|r| read_row(ctx, r, "session").map(|v| row_name(&v))).collect();
    let now = iso_utc(now_ms() as u64 / 1000);
    let (kept, mut failed) = keep_live_sessions(ctx, &roles);
    let (mut closed, mut skipped) = (0, 0);
    // #4944 — a session a live run points at is never closed; unreadable runs close nothing
    let runs = door(ctx, &["-s", "--max-time", "20", &format!("{}/v1/identity/sessionruns?runEndedAt.absent=true&limit=5000", ctx.api)]).unwrap_or_default();
    let expired = match rows::live_run_sessions(&runs) {
        Ok(live) => { keep.extend(live); rows::expired_open(&listing, &now, &keep) }
        Err(why) => { failed += 1; eprintln!("  closing nothing this pass: {}", why); vec![] }
    };
    for row in expired {
        let Some((owner, name, body)) = rows::session_close(&row, &now, None) else { failed += 1; eprintln!("  {} not closed: the row could not be read", row_name(&row)); continue };
        // a person's sign-in (Jeff's browser) is closed by its own door, never by an agent's credential
        if !roles.iter().any(|r| *r == owner) { skipped += 1; continue; }
        // #4944 — a PATCH of the two fields; a refusal is named on the spine by patch_row
        if patch_row(ctx, &owner, "identity/sessions", "session", &name, &body) { closed += 1 } else { failed += 1; eprintln!("  {} not closed (the door refused the PATCH; see session.row.failed)", name) }
    }
    ctx.spine(&["session.sweep", &format!("kept={}", kept), &format!("closed={}", closed), &format!("skipped={}", skipped), &format!("failed={}", failed)]);
    println!("sweep: {} live session(s) kept open, {} expired session(s) closed, {} not an agent's (left to their own door), {} failed", kept, closed, skipped, failed);

    // #4924 AC2: scan for 401 expired Claude logins and alert Jeff within 60s
    let scan_script = format!("{}/platform/scripts/role-401-scan", envd("CHORUS_ROOT", "/Users/jeffbridwell/CascadeProjects/chorus"));
    if Path::new(&scan_script).exists() {
        let _ = Command::new("bash").args([&scan_script, "--all", "--notify"]).status();
    }

    if failed > 0 { 1 } else { 0 }
}

/// #4377 reopen 1 — a session whose process is running stays open, idle or
/// not (Jeff 2026-10-14: "see no sessions for wren and abby" — both running,
/// both dropped 10 minutes after their last turn). Its login is renewed and
/// the row's expiry follows, the same renewal a turn makes. A role whose
/// process has exited is not renewed: its run already ended at exit, and its
/// row drops from view at its expiry as before (#4500).
fn keep_live_sessions(ctx: &Ctx, roles: &[String]) -> (u32, u32) {
    let (mut kept, mut failed) = (0, 0);
    for role in roles {
        if !matches!(ctx.read_state(role), LoginState::Recorded { .. }) { continue; }
        // #4889 — an idle role is never woken by a nudge to a dead pane, so the sweep
        // keeps its address current too, not only its turns
        refresh_pane(ctx, role);
        if live_entries(&ctx.sessions_dir, role, &ctx.ps).is_empty() { continue; }
        let Some(session) = read_row(ctx, role, "session") else { continue };
        let exp = current_login_expiry(ctx, role);
        match rows::renewed_session(session.clone(), &exp) {
            None if exp.is_empty() => { failed += 1; eprintln!("  {} not kept: no current login", role) }
            None => kept += 1,
            Some(s) => if patch_changed(ctx, role, "identity/sessions", "session", &session, &s) { kept += 1 } else { failed += 1; eprintln!("  {} not kept: the row write was refused", role) },
        }
    }
    (kept, failed)
}

/// #4889 (Jeff 07:48: a role's address follows the role, like a phone number) — the
/// Presence takes the pane the role's tmux session shows now, the same read a login
/// makes. Run by the sweep (every ~60s, as jeffbridwell, whose tmux socket holds every
/// role's session): a role's own account cannot read that socket (mode 700), so a
/// turn could not do it (Kade 07:57). A sweep that cannot read the pane says so once
/// per change (presence.pane.unreadable), never silently. Pulse never writes a
/// Presence and never guesses a pane: a pane that is gone is a loud not-delivered there.
fn refresh_pane(ctx: &Ctx, role: &str) {
    let Some(p) = presence_now(ctx, role) else { return };
    let answer = if ctx.has_tmux(role) {
        sh(&ctx.tmux, &["list-panes", "-t", &Ctx::tmux_session(role), "-F", "#{pane_id}|#{pane_tty}"])
    } else {
        Err(format!("no tmux session {} readable from account {}", Ctx::tmux_session(role), env::var("USER").unwrap_or_default()))
    };
    let marker = PathBuf::from(&ctx.identity_dir).join(role).join("pane.unreadable");
    let (pane, tty) = match rows::pane_read(answer) {
        Ok(pt) => { let _ = fs::remove_file(&marker); pt }
        Err(why) => {
            if !marker.exists() {
                // the marker makes the event once per change; without its dir it would fire every sweep
                if let Err(e) = marker.parent().map_or(Ok(()), fs::create_dir_all).and_then(|_| fs::write(&marker, &why)) {
                    eprintln!("chorus-principal: pane.unreadable marker for {role} not written: {e}");
                }
                ctx.spine(&["presence.pane.unreadable", role, &format!("why={}", why)]);
            }
            return;
        }
    };
    repane(ctx, role, p, &pane, &tty);
}

/// #4889 — write the pane the role really runs in onto its Presence, when it moved.
fn repane(ctx: &Ctx, role: &str, presence: Value, pane: &str, tty: &str) {
    let was = presence.get("pane").and_then(|x| x.as_str()).unwrap_or("").to_string();
    if let Some(np) = rows::repaned_presence(presence, pane, tty) {
        let ok = put_row(ctx, role, "identity/presences", "presence", &np);
        ctx.spine(&["presence.pane.refreshed", role, &format!("was={}", was), &format!("now={}", pane), &format!("written={}", ok)]);
    }
}

/// #4895 — a delivery to the role's pane failed for good: its Presence reads unreachable
/// now, not at the next sweep, and says so on the spine.
fn undelivered(ctx: &Ctx, role: &str, pane: &str) -> i32 {
    let Some(p) = presence_now(ctx, role) else { return 1 };
    match rows::undelivered_presence(p, pane) {
        None => 0,
        Some(np) => {
            let ok = put_row(ctx, role, "identity/presences", "presence", &np);
            ctx.spine(&["presence.unreachable", role, &format!("pane={}", pane), "reason=delivery-failed", &format!("written={}", ok)]);
            if ok { 0 } else { 1 }
        }
    }
}

/// Ids of the role's background sessions (`claude agents --json --cwd <role dir>`).
fn background_ids(ctx: &Ctx, role: &str) -> Vec<String> {
    let out = sh(&ctx.claude, &["agents", "--json", "--cwd", &ctx.role_dir(role)]).unwrap_or_default();
    let v: Value = serde_json::from_str(&out).unwrap_or(Value::Null);
    v.as_array().map(|a| a.iter().filter(|s| s.get("kind").and_then(|k| k.as_str()) == Some("background"))
        .filter_map(|s| s.get("id").or_else(|| s.get("sessionId")).and_then(|x| x.as_str()).map(String::from)).collect()).unwrap_or_default()
}

/// The CHORUS_ROLE a running process carries, read from its environment.
fn process_role(ctx: &Ctx, pid: u64) -> Option<String> {
    let out = sh(&ctx.ps, &["eww", "-o", "command=", "-p", &pid.to_string()]).ok()?;
    rows::env_value(&out, "CHORUS_ROLE")
}

/// The claude process running in the role's pane, written into the registry
/// the way the SessionStart hook writes it. None when the pane runs no claude.
fn register_from_pane(ctx: &Ctx, role: &str, runtime: rows::PaneRuntime) -> Option<Live> {
    register_from_pane_matching(ctx, role, &|c: &str| rows::pane_process_is(c, runtime))
}

/// The pane's process that runs `is`: registered as the role's live session.
fn register_from_pane_matching(ctx: &Ctx, role: &str, is: &dyn Fn(&str) -> bool) -> Option<Live> {
    let out = sh(&ctx.tmux, &["list-panes", "-t", &Ctx::tmux_session(role), "-F", "#{pane_id} #{pane_tty} #{pane_pid}"]).ok()?;
    let mut it = out.lines().next()?.split_whitespace();
    let (pane, tty, shell_pid) = (it.next()?.to_string(), it.next()?.to_string(), it.next()?.to_string());
    let pgrep = envd("AWAKE_PGREP", "pgrep");
    // #4929 — an own-account start puts sudo, launchctl, sudo and bash between the
    // pane and claude, so search the pane's descendants (a few levels), not only
    // its direct children.
    let mut level = vec![shell_pid.clone()];
    let mut found = None;
    for _ in 0..6 {
        let mut next = Vec::new();
        for p in &level {
            for k in sh(&pgrep, &["-P", p]).unwrap_or_default().lines().filter_map(|k| k.trim().parse::<u64>().ok()) {
                let cmd = sh(&ctx.ps, &["-o", "command=", "-p", &k.to_string()]).unwrap_or_default();
                // the wrappers an own-account start puts in between are never the session,
                // whatever their arguments say (Gemini and agy roles match by substring)
                if !rows::is_launch_wrapper(&cmd) && is(&cmd) { found = Some(k); break; }
                next.push(k.to_string());
            }
            if found.is_some() { break; }
        }
        if found.is_some() || next.is_empty() { break; }
        level = next;
    }
    let pid = found?;
    let entry = serde_json::json!({"role": role, "pid": pid, "tty": tty, "host": "tmux", "tmux": pane, "source": "chorus-principal (attach, #4328)"});
    let _ = fs::create_dir_all(&ctx.sessions_dir);
    fs::write(ctx.sessions_dir.join(format!("{}-{}.json", role, pid)), entry.to_string()).ok()?;
    ctx.spine(&["session.registered.from_pane", role, &format!("pid={}", pid), &format!("pane={}", pane)]);
    // #4889 (measured 11-05 11:20: Abby %16→%17, BQ %14→%15) — the run's Presence was
    // written before start_in_pane replaced the session (#4845), so it names a pane that
    // no longer exists; the pane the runtime is found in is its address
    if let Some(p) = presence_now(ctx, role) { repane(ctx, role, p, &pane, &tty); }
    parse_registry(&entry.to_string())
}

/// `relogin <role>` — the background retry a pending login starts. Ends when
/// the login is recorded, when the role is gone, or at AWAKE_RETRY_SECS.
fn relogin(_ctx: &Ctx, role: &str) -> i32 {
    // #4933 AC3 — the launcher no longer signs a role in or retries it: the role
    // establishes its own session at start, and its next turn retries a failed one.
    eprintln!("chorus-principal: relogin is retired (#4933): {} establishes its own session; its next turn retries", role);
    2
}

/// #4445 — close the role's other open sessions on this channel, left by a
/// login that wrote its row and was interrupted before recording it. Loud, never
/// a refusal: the new session is already recorded.
fn close_superseded(ctx: &Ctx, role: &str, channel: &str, keep: &str) {
    let Ok(token) = sh(&ctx.token_bin, &[role]) else { return };
    let role_id_dir = PathBuf::from(&ctx.identity_dir).join(role);
    let hdr = role_id_dir.join("session.hdr");
    if write_private(&hdr, &format!("Authorization: Bearer {}\n", token.trim())).is_err() { return; }
    let url = format!("{}/v1/identity/sessions?channel={}&limit=1000", ctx.api, channel);
    let list = door(ctx, &["-s", "--max-time", "10", "-H", &format!("@{}", hdr.display()), &url]).unwrap_or_default();
    let _ = fs::remove_file(&hdr);
    let stale = rows::superseded_sessions(&list, role, channel, keep);
    if stale.is_empty() { return; }
    let mut closed = Vec::new();
    for name in &stale {
        match close_row(ctx, role, name) {
            Ok(()) => closed.push(name.clone()),
            Err(why) => eprintln!("login: could not close older session {} — {}", name, why),
        }
    }
    if !closed.is_empty() {
        ctx.spine(&["session.superseded", role, &format!("closed={}", closed.join(",")), &format!("by={}", keep)]);
        println!("login: closed {} older open session(s) an interrupted login left: {}", closed.len(), closed.join(", "));
    }
}

/// Close the Session row (#4944): a PATCH of sessionState, endedAt and, when set,
/// endedBy (#4412: who ended it). The row is never read back and sent whole; a refusal
/// is named on the spine by patch_row.
fn close_row(ctx: &Ctx, role: &str, session: &str) -> Result<(), String> {
    let ended = iso_utc(now_ms() as u64 / 1000);
    let by = env::var("CHORUS_ENDED_BY").ok();
    let row = serde_json::json!({"name": session, "ownedBy": format!("principal-{}", role)});
    let (_, name, body) = rows::session_close(&row, &ended, by.as_deref()).ok_or_else(|| format!("session {} has no name", session))?;
    if patch_row(ctx, role, "identity/sessions", "session", &name, &body) { Ok(()) } else { Err("the identity API refused the close (see session.row.failed)".into()) }
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
    // #4406 — Jeff 2026-09-30: "session starts when i do chorus-principal login
    // and ends when i do logout". /exit ends this Claude run, not the session:
    // the next login goes back to the same open session with a new run.
    if from_exit {
        end_run(ctx, role, "exit");
        ctx.spine(&["session.run.exited", role]);
        return 0;
    }
    let st = ctx.read_state(role);
    // #4328 — the run ends (and its presence goes unreachable) before the session closes
    end_run(ctx, role, "logout");
    let closed = match &st {
        LoginState::Recorded { session, .. } => Some((session.clone(), close_row(ctx, role, session))),
        _ => None,
    };
    ctx.write_state(role, &LoginState::Closed);
    let how = if from_exit { "exit" } else { "off" };
    let session = closed.as_ref().map(|(s, _)| s.as_str()).unwrap_or("");
    ctx.spine(&["session.logout", role, &format!("session={}", session), &format!("how={}", how)]);
    if !from_exit {
        // #4337 — a background copy of the conversation outlives the pane and is
        // what the next `on` tripped over (09-26: `claude -c` refused, "running in
        // the background"); off ends it too, the conversation is kept
        for id in background_ids(ctx, role) { let _ = sh(&ctx.claude, &["stop", &id]); }
        let live = live_entries(&ctx.sessions_dir, role, &ctx.ps);
        if ctx.has_tmux(role) { let _ = sh(&ctx.tmux, &["kill-session", "-t", &Ctx::tmux_session(role)]); }
        for l in &live { let _ = fs::remove_file(ctx.sessions_dir.join(format!("{}-{}.json", role, l.pid))); }
        if live.is_empty() && closed.is_none() { println!("{} logged out (it was not running)", role); return 0; }
    }
    // #4385 — the pane Jeff typed into is gone; what shows behind it can be
    // days old (09-28 05:23: a Sep 25 "chorus-awake silas … starting" read as
    // his logout turning into a login). Clear the screen, then say logged out.
    if !from_exit && std::io::IsTerminal::is_terminal(&std::io::stdout()) { print!("\x1b[2J\x1b[3J\x1b[H"); }
    match closed {
        Some((s, Ok(()))) => println!("{} logged out: login closed (session {})", role, s),
        Some((s, Err(why))) => println!("{} logged out: stopped; the login row {} could not be closed ({}), and it expires on its own", role, s, why),
        None => println!("{} logged out: stopped; it had no recorded login to close", role),
    }
    0
}

fn status(ctx: &Ctx) -> i32 {
    let listing = door(ctx, &["-s", "--max-time", "5", &format!("{}/v1/identity/sessions?endedAt.absent=true&limit=5000", ctx.api)]).unwrap_or_default(); // #4838
    let presences = door(ctx, &["-s", "--max-time", "5", &format!("{}/v1/identity/presences?limit=5000", ctx.api)]).unwrap_or_default();
    let roles = match agent_roles(ctx) { Ok(r) => r, Err(why) => { eprintln!("chorus-principal: REFUSED — {}", why); return 2; } };
    for role in roles.iter().map(String::as_str) {
        let live = live_entries(&ctx.sessions_dir, role, &ctx.ps);
        let pid = live.first().map(|l| l.pid);
        let answering = pid.is_some() && ctx.answered(role);
        println!("{}", lifecycle::status_line(role, pid, &ctx.read_state(role), answering));
        // #4328 — and what the store says, which is what everyone else reads
        if let Some(line) = store_line(&listing, read_row(ctx, role, "session").map(|v| row_name(&v)).as_deref()) { println!("       {}", line); }
        // #4339 — who is in the room, as the store has it
        let find = |l: &str, name: Option<String>| -> Option<Value> {
            let v: Value = serde_json::from_str(l).ok()?;
            let name = name?;
            v.get("data")?.as_array()?.iter().find(|r| r.get("name").and_then(|n| n.as_str()) == Some(&name)).cloned()
        };
        let s = find(&listing, read_row(ctx, role, "session").map(|v| row_name(&v)));
        let p = find(&presences, read_row(ctx, role, "presence").map(|v| row_name(&v)));
        println!("       {}", rows::room_line(s.as_ref(), p.as_ref()));
        if live.len() >= 2 { println!("       {} live sessions: `chorus-principal logout {}` ends both", live.len(), role); }
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
            Ok(t) if t.contains("login wren") || t.contains("on wren") => {}   // #4345: login is the verb; an older on-task still counts
            Ok(_) => eprintln!("chorus-principal: {} exists without the wren task; not changing it. The task is:\n{}", tasks.display(), lifecycle::vscode_tasks_json(&principal)),
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
            Err(_) => eprintln!("chorus-principal: {} is not plain JSON; add \"task.allowAutomaticTasks\": \"on\" by hand", settings.display()),
        }
        let _ = sh(&envd("AWAKE_OPEN", "open"), &["-a", "Visual Studio Code", &ctx.root]);
    }
    for role in ["silas", "kade"] {
        if attached(role) { continue; }
        let script = format!("tell application \"Terminal\" to do script \"{} login {}\"", principal, role);
        let _ = sh(&osa, &["-e", &script]);
    }
}

/// #4929 — the ONE way a role starts, used by `on`/`login` and by `up`. The
/// principal gate sets the role's own account before anything launches, and a
/// person must be signed in. Jeff 2026-11-08: "i expect the boot up to be the same".
fn start_role(ctx: &mut Ctx, role: &str, attach: bool, by_person: bool) -> Result<Came, (i32, String)> {
    match principal_gate(ctx, role) {
        Ok(acct) => { ctx.run_as = acct; let _ = fs::remove_file(PathBuf::from(&ctx.identity_dir).join(role).join("revoked")); }
        Err(why) => return Err((2, why)),
    }
    // #4931 — fail closed (Jeff 2026-11-08 11:05: "i dont want fall backs to the thing we
    // are trying to remove"). No account of its own means no start: a missing hostAccount
    // is refused by role_host_account's own message; one equal to the launcher's account
    // is refused here. Only starting is gated — logout/off never are.
    if ctx.run_as.is_none() {
        let me = env::var("USER").unwrap_or_default();
        let acct = role_host_account(ctx, role).map_err(|e| (2, e))?;
        return Err((2, format!("{} would run as the launcher's own account {}; refusing (each agent runs as its own account, #4931)", role, if acct.is_empty() { me } else { acct })));
    }
    // #4412 — a person starts it. `up` runs from the roles-up LaunchAgent at
    // login, before anyone is signed in, and never required a person; it keeps
    // that, and only the account choice is shared (#4929 review, run 3).
    if by_person {
        match person_signed_in(ctx) {
            Ok(p) => std::env::set_var("CHORUS_STARTED_BY", p),
            Err(why) => return Err((2, why)),
        }
    }
    on(ctx, role, attach)
}

/// `up [--windows]` — after a reboot: all roles, each through start_role, one line.
fn up(ctx: &mut Ctx, windows: bool) -> i32 {
    // services are waited for ONCE, with the countdown; each role after that probes once
    if let Err(why) = wait_for_services(ctx, ctx.service_wait, true) { eprintln!("chorus-principal: {}", why); }
    ctx.service_wait = 0;
    // #4845 — every agent role the roles door lists (Abby and Borg Queen too), never
    // a list of three; a door that does not answer starts nobody and says so
    let roles = match agent_roles(ctx) {
        Ok(r) => r,
        Err(why) => { eprintln!("chorus-principal up: started nobody — the roles door did not name the roles: {}", why); return 1; }
    };
    let mut results = Vec::new();
    for role in roles.iter().map(String::as_str) {
        // #4929 — `up` starts each role through the same path as `on`: the
        // principal gate picks the role's own account. Before this, `up` called
        // on() with run_as unset, so Kade and Borg Queen booted as jeffbridwell.
        let came = match start_role(ctx, role, false, false) { Ok(c) => c, Err((_, why)) => Came::NotStarted(why) };
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
/// #4368 — a name a Principal row could carry: lowercase letters, digits, '-'.
pub fn is_principal_name(n: &str) -> bool {
    !n.is_empty() && n.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// #4368 — who may log in is the Principal row's kind, read through the
/// identity API: an agent logs in, a service or a person is refused with why,
/// no row is refused naming where the principals are listed, and an API that
/// does not answer is refused with one next command, never a guess from a list.
fn principal_gate(ctx: &Ctx, name: &str) -> Result<Option<String>, String> {
    let url = format!("{}/v1/identity/principals/{}", ctx.api, name);
    let answer = door(ctx, &["-s", "-w", "\n%{http_code}", "--max-time", "5", &url]).unwrap_or_default();
    let (body, code) = answer.trim_end().rsplit_once('\n').unwrap_or(("", answer.trim()));
    let host = rows::login_verdict(name, code.trim(), body, &ctx.api)?;
    let home = ctx.role_dir(name);
    if !Path::new(&home).is_dir() { return Err(format!("{} is an agent, but has no role home at {}; nothing to start", name, home)); }
    // #4383 — a principal whose row names its own Mac account runs as that
    // account, so no other principal can read its token. The launcher must be
    // able to start it without a password.
    let me = env::var("USER").unwrap_or_default();
    match host.filter(|h| !h.is_empty() && *h != me) {
        None => Ok(None),
        Some(acct) => {
            if sh(&ctx.sudo, &["-n", "-u", &acct, "true"]).is_err() {
                return Err(format!("{} runs as the Mac account {}, and this launcher cannot start that account without a password (the sudo rule in /etc/sudoers.d/chorus-principals does not cover it)", name, acct));
            }
            Ok(Some(acct))
        }
    }
}

fn caller_refusal(role: &str, verb: &str) -> Option<String> {
    let in_agent = env::var("CLAUDECODE").map(|v| !v.is_empty()).unwrap_or(false);
    let caller = env::var("CHORUS_ROLE").ok().filter(|r| !r.is_empty());
    lifecycle::caller_may_act(in_agent, caller.as_deref(), role, verb).err()
}

const USAGE: &str = "usage: chorus-principal login|logout|status|up [name]   (an agent principal: its Principal row decides)
  login <role>   start it logged in, or log in the running one and go to its window (was: on)
  logout <role>  stop it and close its login (was: off)
  status         one line per role: running, logged in, answering
  up [--windows] all three after a reboot, one summary line; --windows opens VS Code and two Terminal windows
  <role>         same as on";

/// The whole command. Returns the exit code.
pub fn run(args: &[String]) -> i32 {
    let verb = args.first().map(String::as_str).unwrap_or("");
    // #4845 AC3 (Jeff 11-03 18:11: one trace ID per login) — a login mints one trace
    // and every step it takes carries it: chorus-log reads CHORUS_TRACE_ID, and the
    // retry it spawns inherits it. A caller's trace is kept, never replaced.
    if matches!(verb, "login" | "on" | "up" | "relogin" | "logout" | "off") {
        let trace = env::var("CHORUS_TRACE_ID").ok().filter(|t| !t.is_empty())
            .unwrap_or_else(|| login_trace(verb, args.get(1).map(String::as_str), now_ms(), std::process::id()));
        env::set_var("CHORUS_TRACE_ID", &trace);
        println!("trace: {}", trace);
    }
    let mut ctx = Ctx::from_env();
    let role_arg = |i: usize| -> Result<String, i32> {
        match args.get(i) {
            Some(r) => match agent_roles(&ctx) {
                Ok(known) if known.iter().any(|k| k == r) => Ok(r.clone()),
                Ok(known) => { eprintln!("chorus-principal: unknown role '{}' ({})", r, known.join(" | ")); Err(2) }
                Err(why) => { eprintln!("chorus-principal: REFUSED — {}", why); Err(2) }
            },
            None => { eprintln!("{}", USAGE); Err(2) }
        }
    };
    // #4368 — login and logout take any principal's name; the Principal row
    // decides (principal_gate), never a list in this file.
    let name_arg = |i: usize| -> Result<String, i32> {
        match args.get(i) {
            Some(r) if is_principal_name(r) => Ok(r.clone()),
            Some(r) => { eprintln!("chorus-principal: '{}' is not a principal name (lowercase letters, digits, -)", r); Err(2) }
            None => { eprintln!("{}", USAGE); Err(2) }
        }
    };
    match verb {
        "status" => status(&ctx),
        "up" => {
            if let Some(why) = caller_refusal("all roles", "start") { eprintln!("chorus-principal: REFUSED — {}", why); return 2; }
            up(&mut ctx, args.iter().any(|a| a == "--windows"))
        }
        "relogin" => match name_arg(1) { Ok(r) => relogin(&ctx, &r), Err(c) => c },
        // #4933 AC3 — the role's start hook, as the role's own account
        "establish" => match name_arg(1) {
            Ok(r) => match args.iter().position(|a| a == "--run").map(|i| args.get(i + 1).cloned().unwrap_or_default()) {
                Some(st) if st.is_empty() || !st.bytes().all(|b| b.is_ascii_hexdigit()) => { eprintln!("chorus-principal: establish {} --run needs the launch's hex stamp", r); 2 }
                stamp => {
                    let opt = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1).cloned()).filter(|v| !v.is_empty() && !v.starts_with("__CHORUS_"));
                    establish(&ctx, &r, stamp, opt("--pane"), opt("--tty"))
                }
            },
            Err(c) => c,
        },
        // #4328 — the UserPromptSubmit hook, every turn; never refused, never slow
        // #4520 reopen 1 — the per-turn stamp takes the role by name. Reading
        // the whole roles door on every prompt (and exiting 2 when it was slow)
        // stopped every one of Abby's prompts on 2026-10-13; revocation, which
        // may stop a prompt, reads its own row and fails open on a slow door.
        "seen" => match name_arg(1) { Ok(r) => seen(&ctx, &r), Err(c) => c },
        "seen-write" => match name_arg(1) {
            Ok(r) => seen_write(&ctx, &r, args.get(2).map(String::as_str).unwrap_or(""), args.get(3).map(String::as_str).unwrap_or("turn")),
            Err(c) => c,
        },
        "sweep" => sweep(&ctx),
        // #4895 — pulse, after a delivery failed for good on a dead pane; the principal
        // stays the only writer of a Presence
        "undelivered" => match (name_arg(1), args.get(2).filter(|p| rows::is_pane_id(p))) {
            (Ok(r), Some(pane)) => undelivered(&ctx, &r, pane),
            (Ok(_), None) => { eprintln!("chorus-principal: undelivered <role> <pane %N>"); 2 }
            (Err(c), _) => c,
        },
        // #4447 — the identity owner writes a role's Credential rows, once
        // #4455 — the writer is the caller's own role; none set is refused, never a teammate's name
        "credentials" => match (role_arg(1), credentials_writer(env::var("CHORUS_ROLE").ok())) { (Ok(r), Ok(w)) => record_credentials(&ctx, &r, &w), (Err(c), _) | (_, Err(c)) => c },
        // #4340 — messages.db into the model, one pass; run by com.chorus.messages-project
        "project-messages" => project_messages(&ctx),
        // #4345 — Jeff, 2026-09-27: login/logout is the convention; on/off stay as aliases.
        "logout" | "off" => match name_arg(1) {
            Ok(r) => {
                if let Some(why) = caller_refusal(&r, "stop") { eprintln!("chorus-principal: REFUSED — {}", why); return 2; }
                // the SessionEnd hook (--from-exit) runs inside a session that is
                // already logged in; it never waits on the identity API
                let from_exit = args.iter().any(|a| a == "--from-exit");
                if !from_exit { if let Err(why) = principal_gate(&ctx, &r) { eprintln!("chorus-principal: REFUSED — {}", why); return 2; } }
                // #4412 — a person ends it (/exit ends only the run, #4406)
                if !from_exit {
                    match person_signed_in(&ctx) {
                        Ok(p) => std::env::set_var("CHORUS_ENDED_BY", p),
                        Err(why) => { eprintln!("chorus-principal: REFUSED — {}", why); eprintln!("  nothing was stopped."); return 2; }
                    }
                }
                off(&ctx, &r, args.iter().any(|a| a == "--from-exit"))
            }
            Err(c) => c,
        },
        "login" | "on" => {
            let r = match name_arg(1) { Ok(r) => r, Err(c) => return c };
            if let Some(why) = caller_refusal(&r, "start") { eprintln!("chorus-principal: REFUSED — {}", why); eprintln!("  nothing was started."); return 2; }
            let attach = envd("AWAKE_NO_ATTACH", "0") != "1";
            match start_role(&mut ctx, &r, attach, true) {
                Ok(_) => 0,
                Err((2, why)) => { eprintln!("chorus-principal: REFUSED — {}", why); eprintln!("  nothing was started."); 2 }
                Err((c, _)) => c,
            }
        }
        "" => { eprintln!("{}", USAGE); 2 }
        other => { eprintln!("chorus-principal: unknown verb '{}'", other); eprintln!("{}", USAGE); 2 }
    }
}

/// #4686 — a SPARQL COUNT answer read as JSON, not by needle. Step 3c matched
/// `"value":"` and Fuseki answers `"value": "4"`, so every create read 0 of 4
/// hats and rolled back. None: not a results document, or no number in it.
pub fn sparql_count(body: &str, var: &str) -> Option<usize> {
    let v: Value = serde_json::from_str(body).ok()?;
    v.pointer("/results/bindings/0")?.get(var)?.get("value")?.as_str()?.parse().ok()
}

/// The hats every principal wears (step 3c writes and reads back all of them).
pub const HATS: usize = 4;

/// #4686 — step 3c's verdict: Ok when the store's answer shows all HATS,
/// Err(Some(n)) with the count it read otherwise, Err(None) when the answer
/// is unreadable (never reported as "0 hats").
pub fn wears_all_hats(body: &str) -> Result<usize, Option<usize>> {
    match sparql_count(body, "n") {
        Some(n) if n == HATS => Ok(n),
        other => Err(other),
    }
}

#[cfg(test)]
mod own_account_launch_tests_4777 {
    use super::*;

    #[test]
    fn own_account_inner_cmd_exports_shared_sessions_dir_and_claude_key_hook() {
        let cmd = own_account_inner_cmd(
            "/repo/roles/silas",
            "/repo",
            "/Users/chorus-silas/.chorus/identity",
            "/Users/chorus-silas/.chorus/identity/silas/token.cache",
            "/Users/jeffbridwell/.chorus/sessions",
            "silas",
            "/b/chorus-principal",
            "18f2a",
            "claude -c",
        );
        assert!(cmd.contains("CHORUS_SESSIONS_DIR='/Users/jeffbridwell/.chorus/sessions'"), "{cmd}");
        assert!(cmd.contains("CHORUS_OUTBOX_DIR=\"$HOME/.chorus/outbox\""), "{cmd}");
        assert!(cmd.contains("GIT_CONFIG_KEY_0='safe.directory'"), "{cmd}");
        assert!(cmd.contains("ENABLE_CLAUDEAI_MCP_SERVERS='false'"), "{cmd}");
        // #4931 — the self test runs as the role at every start, before the session
        assert!(cmd.contains("bash '/repo/platform/scripts/role-post'"), "{cmd}");
        assert!(cmd.find("role-post").unwrap() < cmd.find("claude -c").unwrap(), "{cmd}");
        // #4931 negative proof — a stale key file can no longer override the role's login
        assert!(!cmd.contains("claude.key"), "{cmd}");
        assert!(!cmd.contains("CLAUDE_CODE_OAUTH_TOKEN"), "{cmd}");
    }

    #[test]
    fn role_memory_dir_hands_only_own_role_memory_and_refuses_other_roles_4784() {
        let own = role_memory_source_dir("/Users/jeffbridwell", "/Users/jeffbridwell/CascadeProjects/chorus/roles/silas", "silas");
        assert_eq!(
            own.as_deref(),
            Some(std::path::Path::new("/Users/jeffbridwell/.claude/projects/-Users-jeffbridwell-CascadeProjects-chorus-roles-silas/memory"))
        );

        let dirs = role_memory_handoff_dirs(
            "/Users/jeffbridwell",
            "/Users/jeffbridwell/CascadeProjects/chorus",
            "/Users/jeffbridwell/CascadeProjects/chorus/roles/silas",
            "silas",
        )
        .unwrap();
        assert_eq!(dirs.len(), 2);
        assert_eq!(
            dirs[0].0,
            std::path::PathBuf::from("/Users/jeffbridwell/.claude/projects/-Users-jeffbridwell-CascadeProjects-chorus/memory")
        );
        assert_eq!(dirs[0].1, "-Users-jeffbridwell-CascadeProjects-chorus");
        assert_eq!(
            dirs[1].0,
            std::path::PathBuf::from("/Users/jeffbridwell/.claude/projects/-Users-jeffbridwell-CascadeProjects-chorus-roles-silas/memory")
        );
        assert_eq!(dirs[1].1, "-Users-jeffbridwell-CascadeProjects-chorus-roles-silas");

        // Negative proofs (#3734): another role's dir, shared root dir as role_dir, or traversal is refused
        assert!(
            role_memory_source_dir("/Users/jeffbridwell", "/Users/jeffbridwell/CascadeProjects/chorus/roles/wren", "silas").is_none(),
            "NEGATIVE (#3734): silas must never receive wren's memory folder"
        );
        assert!(
            role_memory_handoff_dirs(
                "/Users/jeffbridwell",
                "/Users/jeffbridwell/CascadeProjects/chorus",
                "/Users/jeffbridwell/CascadeProjects/chorus/roles/wren",
                "silas"
            )
            .is_none(),
            "NEGATIVE (#3734): handoff_dirs refuses another role's role_dir"
        );
        assert!(
            role_memory_source_dir("/Users/jeffbridwell", "/Users/jeffbridwell/CascadeProjects/chorus", "silas").is_none(),
            "NEGATIVE (#3734): shared root as role_dir must be refused"
        );
        assert!(
            role_memory_source_dir("/Users/jeffbridwell", "/Users/jeffbridwell/CascadeProjects/chorus/roles/silas/../kade", "silas").is_none(),
            "NEGATIVE (#3734): path traversal must be refused"
        );
    }
}
