// @domain: messages
//! #4515 — `chorus-hook-shim nudge-send`: the chorus-nudge-message tool, moved
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

use crate::hooks::word_cap::{check_cap, resolve_cap_for, CAP_KEY_NUDGE};
use serde_json::{json, Value};
use std::time::Duration;

/// What a nudge-send needs from the world. Tests bring their own.
pub struct Deps<'a> {
    /// athena-make, which serves the roles door
    pub door: String,
    /// chorus-api, which resolves the cap property
    pub api: String,
    /// pulse's nudge endpoint
    pub pulse_url: String,
    pub pulse_secret: Option<String>,
    /// writes one spine event: (event, role, payload)
    pub spine: &'a dyn Fn(&str, &str, &str),
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
            spine,
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

    let mut req = ureq::post(&deps.pulse_url)
        .timeout(Duration::from_secs(5))
        .set("Content-Type", "application/json")
        .set("X-Chorus-Trace-Id", &trace)
        .set("X-Chorus-MCP-Caller", "1");
    if let Some(s) = &deps.pulse_secret {
        req = req.set("X-Chorus-Pulse-Secret", s);
    }
    let body = json!({ "from": from, "to": to, "content": message, "traceId": trace, "expects": expects });
    let resolved = match req.send_json(body) {
        Ok(resp) => resp.into_json::<Value>().ok().and_then(|b| b.get("resolved").and_then(Value::as_str).map(String::from)).unwrap_or_else(|| to.to_string()),
        Err(ureq::Error::Status(code, resp)) => {
            let text = resp.into_string().unwrap_or_default();
            return Err(format!("pulse POST returned {code}: {}", text.chars().take(200).collect::<String>()));
        }
        Err(e) => return Err(format!("pulse did not answer: {e}")),
    };
    Ok(format!("nudge sent: {from} → {resolved} (trace={trace})"))
}
