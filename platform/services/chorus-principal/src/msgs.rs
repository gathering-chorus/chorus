//! #4340 — messages in the model. Pure: the rows the projector writes from
//! messages.db, and the decisions it makes on the way.
//!
//! Jeff 2026-09-26: "refine on the channel piece and how that impacts messages
//! and even a migration to buzz"; option A: the address lives on Presence, the
//! durable key on the Principal. Pulse keeps owning delivery; this reads what
//! pulse recorded and writes it where everyone reads: a Message row per message,
//! a Delivery row per attempt, over one of a few Channel rows.

use serde_json::{json, Value};

/// One row of messages.db, as `sqlite3 -json` gives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Src {
    pub id: u64,
    pub kind: String,
    pub from: String,
    pub to: String,
    pub content: String,
    pub created_at: String,
    pub status: String,
    pub delivered_at: String,
    pub error: String,
}

pub fn parse_rows(json_text: &str) -> Vec<Src> {
    let v: Value = serde_json::from_str(json_text).unwrap_or(Value::Null);
    let s = |r: &Value, k: &str| r.get(k).map(|x| match x { Value::String(t) => t.clone(), Value::Null => String::new(), o => o.to_string() }).unwrap_or_default();
    v.as_array().map(|a| a.iter().filter_map(|r| Some(Src {
        id: r.get("id")?.as_u64()?,
        kind: s(r, "type"), from: s(r, "from"), to: s(r, "to"), content: s(r, "content"),
        created_at: s(r, "created_at"), status: s(r, "delivery_status"), delivered_at: s(r, "delivered_at"), error: s(r, "last_delivery_error"),
    })).collect()).unwrap_or_default()
}

/// sqlite's "2026-09-26 17:16:45" (UTC) as ISO-8601 "2026-09-26T17:16:45Z".
pub fn iso(sqlite_ts: &str) -> String {
    if sqlite_ts.is_empty() { return String::new(); }
    let t = sqlite_ts.replacen(' ', "T", 1);
    if t.ends_with('Z') { t } else { format!("{}Z", t) }
}

/// The principal behind a sender or recipient name, if one exists.
/// messages.db writes both "kade" and "principal-kade"; system, chorus-mcp,
/// pulse and unknown have no principal yet (Wren 13:16: provenance, not identity).
///
/// The BARE name: the service adds "principal-" and refuses a value that
/// already carries it (live 09-26 14:20: every message 422, "double-prefix").
pub fn principal_of(name: &str, principals: &[String]) -> Option<String> {
    let bare = name.trim_start_matches("principal-");
    principals.iter().find(|p| p.as_str() == bare || p.trim_start_matches("principal-") == bare).map(|p| p.trim_start_matches("principal-").to_string())
}

/// An edge value as the service wants it: the bare name, with the kind prefix
/// the listing or a create reply may carry taken off.
pub fn bare(name: &str, kind: &str) -> String { name.strip_prefix(&format!("{}-", kind)).unwrap_or(name).to_string() }

/// The channel a kind of message travels over today.
pub fn channel_for(kind: &str) -> &'static str {
    match kind { "jeff-input" => "terminal", "chat" => "clearing", _ => "nudge" }
}

pub fn message_row(src: &Src, principals: &[String], session: Option<&str>) -> Value {
    let mut v = json!({
        "name": format!("src-{}", src.id),
        "label": format!("{} from {} to {} at {}", src.kind, src.from, src.to, iso(&src.created_at)),
        "ownedBy": "principal-silas",
        "senderName": src.from,
        "messageBody": src.content,
        "messageKind": src.kind,
        "sentAt": iso(&src.created_at),
        "sourceId": src.id.to_string(),
        "overChannel": channel_for(&src.kind),
    });
    if let Some(p) = principal_of(&src.from, principals) { v["sentBy"] = Value::String(p); }
    if let Some(p) = principal_of(&src.to, principals) { v["sentTo"] = Value::String(p); }
    if let Some(s) = session.filter(|s| !s.is_empty()) { v["sentInSession"] = Value::String(s.into()); }
    v
}

pub fn delivery_row(src: &Src, message: &str, presence: Option<&str>, outcome: &str) -> Value {
    let mut v = json!({
        "name": format!("src-{}", src.id),
        "label": format!("{} of message {} to {}", outcome, src.id, src.to),
        "ownedBy": "principal-silas",
        "deliveryOf": bare(message, "message"),
        "overChannel": channel_for(&src.kind),
        "deliveryOutcome": outcome,
    });
    if !src.delivered_at.is_empty() { v["deliveredAt"] = Value::String(iso(&src.delivered_at)); }
    if !src.error.is_empty() { v["deliveryError"] = Value::String(src.error.clone()); }
    if let Some(p) = presence.filter(|p| !p.is_empty()) { v["deliveredTo"] = Value::String(p.into()); }
    v
}

/// What happened to the attempt. pulse knows delivered / failed / queued /
/// pending. It cannot know the text arrived shorter than it was sent: the
/// recipient's own turn can. `received` is what the recipient's prompt held
/// (the seen hook logs it); a received text that is a strict part of the
/// content means the message landed cut. 09-26 12:45: Wren's 1,167-char answer
/// reached Silas's pane as its last line only, and pulse said "delivered".
pub fn outcome(src: &Src, received: &[String]) -> String {
    let base = match src.status.as_str() { "delivered" | "failed" | "queued" | "pending" => src.status.as_str(), _ => "pending" };
    if base != "delivered" { return base.to_string(); }
    // a pane joins wrapped lines, so compare with whitespace collapsed (live
    // 09-26: "as data.\n  Buzz" arrived as "as data. Buzz")
    let squash = |t: &str| t.split_whitespace().collect::<Vec<_>>().join(" ");
    let full = squash(&src.content);
    for r in received {
        let r = squash(r);
        if r.len() >= 12 && r.len() < full.len() && full.contains(&r) { return "truncated".into(); }
    }
    "delivered".into()
}

/// The presence of `principal` that was live at `at`: its run started at or
/// before, and had not ended. `runs`/`presences` are the API listings. The
/// edge values read back as the target's full name (session-run-<name>), so
/// they match on suffix.
pub fn presence_at(principal: &str, at: &str, runs: &Value, presences: &Value) -> Option<String> {
    let rows = |v: &Value| v.get("data").and_then(|d| d.as_array()).cloned().unwrap_or_default();
    let s = |r: &Value, k: &str| r.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let same = |edge: &str, n: &str| !edge.is_empty() && (edge == n || edge.ends_with(&format!("-{}", n)));
    let mut live: Vec<Value> = rows(runs).into_iter().filter(|r| {
        s(r, "ownedBy") == principal && !s(r, "startedAt").is_empty() && s(r, "startedAt").as_str() <= at
            && (s(r, "runEndedAt").is_empty() || s(r, "runEndedAt").as_str() >= at)
    }).collect();
    live.sort_by_key(|r| s(r, "startedAt"));
    let run = live.last()?;
    let run_name = s(run, "name");
    rows(presences).into_iter().find(|p| same(&s(p, "presenceOf"), &run_name)).map(|p| s(&p, "name"))
}

/// The sender's session open at `at` (for sentInSession): owned by it, started at or before.
pub fn session_at(principal: &str, at: &str, sessions: &Value) -> Option<String> {
    let s = |r: &Value, k: &str| r.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let mut c: Vec<Value> = sessions.get("data")?.as_array()?.iter().filter(|r| {
        s(r, "ownedBy") == principal && !s(r, "startedAt").is_empty() && s(r, "startedAt").as_str() <= at
            && (s(r, "endedAt").is_empty() || s(r, "endedAt").as_str() >= at)
    }).cloned().collect();
    c.sort_by_key(|r| s(r, "startedAt"));
    c.last().map(|r| s(r, "name"))
}

fn epoch(iso: &str) -> Option<i64> {
    let p = |a: usize, b: usize| iso.get(a..b)?.parse::<i64>().ok();
    let (y, mo, d, h, mi, se) = (p(0,4)?, p(5,7)?, p(8,10)?, p(11,13)?, p(14,16)?, p(17,19)?);
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400); let yoe = y2 - era * 400;
    let doy = (153 * (if mo > 2 { mo - 3 } else { mo + 9 }) + 2) / 5 + d - 1;
    let days = era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468;
    Some(days * 86400 + h * 3600 + mi * 60 + se)
}

/// Seconds from `from` to `to` (both ISO-8601 UTC); None if either is malformed.
pub fn age_secs(from: &str, to: &str) -> Option<i64> { Some(epoch(to)? - epoch(from)?) }

/// The prompts that can be this message arriving: received at or after it was
/// sent, within `window` seconds. Kade's review of #4340: without the window a
/// 200-prompt log makes any old short prompt a false "truncated".
pub fn received_for(log: &[(String, String)], sent_at: &str, window: i64) -> Vec<String> {
    log.iter().filter(|(at, _)| age_secs(sent_at, at).map(|d| (0..=window).contains(&d)).unwrap_or(false)).map(|(_, t)| t.clone()).collect()
}
