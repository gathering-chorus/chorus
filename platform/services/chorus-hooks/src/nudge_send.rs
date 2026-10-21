// @domain: messages
//! #4515 — `chorus-hook-shim nudge-send`: the clearing-nudge tool, moved
//! out of the old TS MCP server (server.ts executeNudge, word-cap.ts,
//! nudge-transport.ts, peers.ts) into the binary its Skill row names.
//!
//! The order is the design, carried over unchanged from #3818:
//!   1. the recipient must be a role the roles door lists (#4432); a door that
//!      does not answer refuses the nudge rather than guessing
//!   2. the word cap, BEFORE anything is recorded: an over-cap nudge leaves no
//!      trace of having been sent, because it wasn't. The count is the one the
//!      Stop hook uses (word_cap::count_words), so there is one way to count.
//!   3. the spine records nudge.requested + nudge.emitted, before the send, so
//!      the audit trail survives an unreachable pulse
//!   4. POST to pulse, which owns delivery. A failed POST fails the call:
//!      "sent" never means "we tried".
//!
//! The sender is the runner's verified caller ({caller}), never an argument.

use crate::hooks::buzz_reply::{load_key, publish_authed, sign_event, NostrEvent, RoleKey};
use crate::hooks::word_cap::{check_cap, resolve_cap_for, CAP_KEY_NUDGE};
use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Duration;

/// #4629 S2 — the team channel's id (the same default pulse and Clearing use).
pub const TEAM_CHANNEL_DEFAULT: &str = "160e1f5d-6816-4c68-94b3-b5f1ade75565";

/// #4629 S2 — where the sender writes its own signed note: the room, or its outbox when the relay is down.
pub struct BuzzDeps {
    /// ~/.chorus/identity (the role's key, 0600, read by buzz_reply::load_key)
    pub identity_dir: PathBuf,
    /// BUZZ_RELAY_URL; unset means the sender signs nothing (no literal address here: #3370)
    pub relay_url: Option<String>,
    /// the LNP loopback tunnel, as the reply path uses
    pub tunnel: Option<String>,
    /// ~/.chorus/outbox; a role's queued notes live in <outbox>/<role>/ (0700)
    pub outbox_dir: PathBuf,
    pub channel: String,
}

/// What a nudge-send needs from the world. Tests bring their own.
pub struct Deps<'a> {
    /// athena-make, which serves the roles door
    pub door: String,
    /// chorus-api, which resolves the cap property
    pub api: String,
    /// pulse's nudge endpoint
    pub pulse_url: String,
    pub pulse_secret: Option<String>,
    /// #4534 — the caller's identity token (the runner hands every tool run
    /// CHORUS_IDENTITY_TOKEN). A role on its own macOS user cannot read the
    /// 0600 secret file, so pulse accepts this instead when its WebID names `from`.
    pub identity_token: Option<String>,
    /// writes one spine event: (event, role, payload)
    pub spine: &'a dyn Fn(&str, &str, &str),
    /// #4629 S2 — set when the sender signs its own room note (CHORUS_NUDGE_SIGNS=1); None keeps pulse's bridge copy.
    pub buzz: Option<BuzzDeps>,
}

impl Deps<'_> {
    pub fn from_env(spine: &dyn Fn(&str, &str, &str)) -> Deps<'_> {
        let env = |k: &str, d: &str| std::env::var(k).ok().filter(|v| !v.is_empty()).unwrap_or_else(|| d.to_string());
        let home = env("HOME", "");
        let secret = std::env::var("CHORUS_PULSE_SECRET").ok().filter(|s| !s.is_empty()).or_else(|| {
            let path = env("CHORUS_PULSE_SECRET_FILE", &format!("{home}/.chorus/pulse-nudge.secret"));
            std::fs::read_to_string(path).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
        });
        Deps {
            door: env("ATHENA_MAKE_URL", "http://localhost:3360"),
            api: env("CHORUS_API_URL", "http://localhost:3340"),
            pulse_url: env("CHORUS_PULSE_URL", "http://localhost:3475/api/nudge"),
            pulse_secret: secret,
            identity_token: std::env::var("CHORUS_IDENTITY_TOKEN").ok().filter(|t| !t.is_empty()),
            spine,
            buzz: (std::env::var("CHORUS_NUDGE_SIGNS").as_deref() == Ok("1")).then(|| BuzzDeps {
                identity_dir: PathBuf::from(&home).join(".chorus/identity"),
                relay_url: std::env::var("BUZZ_RELAY_URL").ok().filter(|v| !v.is_empty()),
                tunnel: std::env::var("BUZZ_RELAY_TUNNEL").ok().filter(|t| !t.is_empty()),
                outbox_dir: PathBuf::from(&home).join(".chorus/outbox"),
                channel: env("BUZZ_TEAM_CHANNEL", TEAM_CHANNEL_DEFAULT),
            }),
        }
    }
}

/// Every agent or human role the roles door lists (peers.ts roleSetsFrom).
pub fn peers_from(body: &Value) -> Result<Vec<String>, String> {
    let rows = body.get("data").and_then(Value::as_array).ok_or("the roles door answered with no data list")?;
    let kind = |r: &Value| r.get("roleKind").and_then(Value::as_str).unwrap_or("").to_string();
    let mut peers: Vec<String> = rows
        .iter()
        .filter(|r| matches!(kind(r).as_str(), "agent" | "human"))
        .filter_map(|r| r.get("name").and_then(Value::as_str).map(String::from))
        .filter(|n| !n.is_empty())
        .collect();
    if !rows.iter().any(|r| kind(r) == "agent") {
        return Err("the roles door lists no agent role".into());
    }
    peers.sort();
    Ok(peers)
}

fn read_peers(door: &str) -> Result<Vec<String>, String> {
    let url = format!("{door}/v1/roles/roles?limit=500");
    let resp = ureq::get(&url).timeout(Duration::from_secs(2)).call().map_err(|e| format!("the roles door did not answer ({url}): {e}"))?;
    let body: Value = resp.into_json().map_err(|e| format!("the roles door answered with no JSON: {e}"))?;
    peers_from(&body)
}

/// `nudge-send <from> <to> <message> [--expects none|reply|decision|action]`.
/// Ok = the line the caller sees; Err = the refusal, nothing sent.
pub fn send(args: &[String], deps: &Deps) -> Result<String, String> {
    let (from, to, message) = match args {
        [f, t, m, ..] => (f.as_str(), t.as_str(), m.as_str()),
        _ => return Err("usage: nudge-send <from> <to> <message> [--expects none|reply|decision|action]".into()),
    };
    let mut expects = "none".to_string();
    let mut rest = args[3..].iter();
    while let Some(a) = rest.next() {
        match a.as_str() {
            "--expects" => expects = rest.next().cloned().unwrap_or_default(),
            other => return Err(format!("nudge-send: unknown argument {other}")),
        }
    }
    if !matches!(expects.as_str(), "none" | "reply" | "decision" | "action") {
        return Err(format!("expects must be none, reply, decision or action (got '{expects}')"));
    }
    if from.is_empty() || from == "unverified" {
        return Err("nudge-send needs a verified sender".into());
    }
    if message.trim().is_empty() {
        return Err("the message is empty".into());
    }
    let peers = read_peers(&deps.door)?;
    if !peers.iter().any(|p| p == to) {
        return Err(format!("Unknown recipient '{to}' — the roles door lists {}", peers.join(" | ")));
    }
    let cap = resolve_cap_for(from, &deps.api, CAP_KEY_NUDGE);
    if let Err(over) = check_cap(message, cap) {
        return Err(format!(
            "word-cap: {} words, cap is {}. Rewrite shorter and resend — nothing was sent and nothing was truncated. \
             Fenced code blocks, inline code, and URLs do not count toward the total, so receipts belong in a fence.",
            over.words, over.cap
        ));
    }
    let trace = uuid::Uuid::now_v7().to_string();
    let payload = format!("from={from},to={to},chars={},trace={trace},origin=mcp,content={message}", message.chars().count());
    (deps.spine)("nudge.requested", from, &payload);
    (deps.spine)("nudge.emitted", from, &payload);

    // #4629 S2 — sign first (the trace is the note's msg tag, the join key pulse stores as trace_id),
    // then POST with buzz=sender so pulse makes no bridge copy, then publish or queue.
    let signed = deps.buzz.as_ref().and_then(|b| sign_note(b, from, to, message, &trace).map_err(|e| {
        (deps.spine)("buzz.sign.skipped", from, &format!("trace={trace},reason={e}"));
    }).ok());
    let mut req = ureq::post(&deps.pulse_url)
        .timeout(Duration::from_secs(5))
        .set("Content-Type", "application/json")
        .set("X-Chorus-Trace-Id", &trace)
        .set("X-Chorus-MCP-Caller", "1");
    if let Some(s) = &deps.pulse_secret {
        req = req.set("X-Chorus-Pulse-Secret", s);
    }
    if let Some(t) = &deps.identity_token {
        req = req.set("Authorization", &format!("Bearer {t}"));
    }
    let mut body = json!({ "from": from, "to": to, "content": message, "traceId": trace, "expects": expects });
    if signed.is_some() {
        body["buzz"] = json!("sender");
    }
    let resolved = match req.send_json(body) {
        Ok(resp) => resp.into_json::<Value>().ok().and_then(|b| b.get("resolved").and_then(Value::as_str).map(String::from)).unwrap_or_else(|| to.to_string()),
        Err(ureq::Error::Status(code, resp)) => {
            let text = resp.into_string().unwrap_or_default();
            return Err(format!("pulse POST returned {code}: {}", text.chars().take(200).collect::<String>()));
        }
        Err(e) => return Err(format!("pulse did not answer: {e}")),
    };
    let room = match (&deps.buzz, signed) {
        (Some(b), Some((key, ev))) => {
            let line = publish_or_queue(b, from, &key, &ev, deps.spine);
            // #4629 S3 — the note is not in the room, so pulse would wait for its fallback: release it now
            if !line.starts_with("stored") {
                release_held(deps, from, &trace);
            }
            format!(" · Buzz: {line}")
        }
        _ => String::new(),
    };
    Ok(format!("nudge sent: {from} → {resolved} (trace={trace}){room}"))
}

/// #4629 S3 — tell pulse the note did not reach the room (POST /api/buzz/release {from, trace}),
/// so a nudge pulse holds for its note is delivered at once. Best effort: pulse's 30 s fallback covers a miss.
fn release_held(deps: &Deps, from: &str, trace: &str) {
    let url = deps.pulse_url.replace("/api/nudge", "/api/buzz/release");
    let mut req = ureq::post(&url).timeout(Duration::from_secs(3)).set("Content-Type", "application/json");
    if let Some(s) = &deps.pulse_secret {
        req = req.set("X-Chorus-Pulse-Secret", s);
    }
    if let Some(t) = &deps.identity_token {
        req = req.set("Authorization", &format!("Bearer {t}"));
    }
    let outcome = match req.send_json(json!({ "from": from, "trace": trace })) {
        Ok(_) => "asked".to_string(),
        Err(e) => format!("failed: {e}"),
    };
    (deps.spine)("buzz.release.requested", from, &format!("trace={trace},outcome={outcome}"));
}

/// The room's text for a nudge, as pulse's bridge copy writes it (buzz-writer.ts roomText).
pub fn room_text(from: &str, to: &str, message: &str) -> String {
    if to == "jeff" { message.to_string() } else { format!("[nudge from {from} → {to}] {message}") }
}

/// #4629 S2 — the sender's own kind 9 note in #team, joined to pulse's row by msg=trace-<uuid>.
pub fn sign_note(b: &BuzzDeps, from: &str, to: &str, message: &str, trace: &str) -> Result<(RoleKey, NostrEvent), String> {
    if b.relay_url.is_none() {
        return Err("BUZZ_RELAY_URL is not set".into());
    }
    let key = load_key(&b.identity_dir, from)?;
    let tags = vec![
        vec!["h".to_string(), b.channel.clone()],
        vec!["t".to_string(), "team".to_string()],
        vec!["author".to_string(), from.to_string()],
        vec!["kind".to_string(), "nudge".to_string()],
        vec!["to".to_string(), to.to_string()],
        vec!["msg".to_string(), format!("trace-{trace}")],
    ];
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let ev = sign_event(&key, 9, tags, &room_text(from, to, message), now)?;
    Ok((key, ev))
}

/// Flush this role's queued notes, then publish `ev`; a failure queues it. Returns the line the caller sees.
/// A flush that meets a down relay queues `ev` without trying it, so a send waits one timeout, not two.
fn publish_or_queue(b: &BuzzDeps, role: &str, key: &RoleKey, ev: &NostrEvent, spine: &dyn Fn(&str, &str, &str)) -> String {
    let relay = b.relay_url.as_deref().unwrap_or_default();
    let tried = match flush_outbox(b, role, key, spine) {
        Ok(flushed) => publish_authed(relay, b.tunnel.as_deref(), key, ev).map(|()| flushed),
        Err(e) => Err(e),
    };
    match tried {
        Ok(flushed) => {
            spine("buzz.publish.stored", role, &format!("event={},flushed={flushed}", ev.id));
            "stored".to_string()
        }
        Err(e) => match queue(b, role, ev) {
            Ok(path) => {
                spine("buzz.publish.queued", role, &format!("event={},reason={e}", ev.id));
                format!("queued in {} (relay: {e})", path.display())
            }
            Err(q) => {
                spine("buzz.publish.lost", role, &format!("event={},reason={e},queue={q}", ev.id));
                format!("NOT stored and NOT queued (relay: {e}; outbox: {q})")
            }
        },
    }
}

/// The role's outbox dir, created 0700.
fn outbox_for(b: &BuzzDeps, role: &str) -> Result<PathBuf, String> {
    let dir = b.outbox_dir.join(role);
    std::fs::create_dir_all(&dir).map_err(|e| format!("outbox dir: {e}"))?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).map_err(|e| format!("outbox dir mode: {e}"))?;
    Ok(dir)
}

fn queue(b: &BuzzDeps, role: &str, ev: &NostrEvent) -> Result<PathBuf, String> {
    let path = outbox_for(b, role)?.join(format!("{}.json", ev.id));
    let body = serde_json::to_string(ev).map_err(|e| format!("encode: {e}"))?;
    std::fs::write(&path, body).map_err(|e| format!("write: {e}"))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).map_err(|e| format!("file mode: {e}"))?;
    Ok(path)
}

/// Publish this role's queued notes, oldest first; each one stored is removed. Ok(how many went out);
/// Err(the relay's answer) at the first publish that fails, the rest left queued. An unreadable file is
/// reported (buzz.outbox.unreadable) and left in place, never skipped silently.
pub fn flush_outbox(b: &BuzzDeps, role: &str, key: &RoleKey, spine: &dyn Fn(&str, &str, &str)) -> Result<usize, String> {
    let Ok(dir) = outbox_for(b, role) else { return Ok(0) };
    let relay = b.relay_url.as_deref().unwrap_or_default();
    let mut files: Vec<_> = std::fs::read_dir(&dir).map(|r| r.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    files.sort_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
    let mut sent = 0;
    for path in files {
        let Some(ev) = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<NostrEvent>(&t).ok()) else {
            spine("buzz.outbox.unreadable", role, &format!("file={}", path.display()));
            continue;
        };
        publish_authed(relay, b.tunnel.as_deref(), key, &ev)?;
        let _ = std::fs::remove_file(&path);
        spine("buzz.publish.flushed", role, &format!("event={}", ev.id));
        sent += 1;
    }
    Ok(sent)
}
