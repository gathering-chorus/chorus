//! #4328 — the rows a login writes, and the edits that keep them true. Pure.
//!
//! Jeff, 2026-09-26 08:29: "is ur session better now?" It was not: #4302 and
//! #4323 landed the model and nothing wrote it. A login now writes, through
//! the generated API:
//!
//!   Session     actsAs + startedAt added to the row #4202 already wrote
//!   SessionRun  this Claude process; previousRun names the one it replaced
//!   Presence    where it can be reached: pane, tty, account; reachability
//!               starts "unknown" and turns "reachable" only when a delivery
//!               lands (lastDeliveredAt), never because a pid exists
//!   Context     kind boot: what the run was given at login
//!
//! Every PUT on these routes replaces the whole row, so each edit takes the
//! full row the login saved and changes only its own fields.

use serde_json::{json, Value};

/// The run row for this start. `conversation` is the transcript id when it is
/// known at start (an attach, or `claude -c`), else "pending": the first turn's
/// `seen` writes the real one (the hook input carries it).
pub fn run_row(role: &str, name: &str, session: &str, conversation: &str, started: &str, previous: Option<&str>) -> Value {
    let mut v = json!({
        "name": name,
        "label": format!("{} run {}", role, started),
        "comment": format!("{}'s Claude Code process started {} by chorus-principal. #4328.", role, started),
        "ownedBy": format!("principal-{}", role),
        "runOf": session,
        "startedAt": started,
        "conversationId": conversation,
    });
    if let Some(p) = previous.filter(|p| !p.is_empty()) { v["previousRun"] = Value::String(p.into()); }
    v
}

pub fn presence_row(role: &str, name: &str, run: &str, pane: &str, tty: &str, host_account: &str) -> Value {
    let mut v = json!({
        "name": name,
        "label": format!("{} presence {}", role, if pane.is_empty() { tty } else { pane }),
        "comment": format!("Where {}'s run {} is reached. reachable only after a delivery lands. #4328.", role, run),
        "ownedBy": format!("principal-{}", role),
        "presenceOf": run,
        "hostAccount": host_account,
        "reachability": "unknown",
        // #4340 — the pane is reached over the nudge channel (pulse types into it)
        "reachableOver": "nudge",
    });
    if !pane.is_empty() { v["pane"] = Value::String(pane.into()); }
    if !tty.is_empty() { v["tty"] = Value::String(tty.into()); }
    v
}

pub fn boot_context_row(role: &str, name: &str, run: &str, started: &str) -> Value {
    json!({
        "name": name,
        "label": format!("{} boot context {}", role, started),
        "comment": format!("The SessionStart envelope {} was given at login {}. #4328.", role, started),
        "ownedBy": format!("principal-{}", role),
        "contextOf": run,
        "contextKind": "boot",
    })
}

/// #4342 — the conversation a run speaks in: its Claude Code transcript id.
/// None while the id is still unknown ("pending"): no row for a guess.
pub fn conversation_row(role: &str, run: &str, conversation: &str) -> Option<Value> {
    if conversation.is_empty() || conversation == "pending" { return None; }
    let short: String = conversation.chars().take(8).collect();
    Some(json!({
        "name": format!("{}-conversation-{}", role, short),
        "label": format!("{} conversation {}", role, short),
        "comment": format!("The Claude Code transcript {} that {}'s run {} speaks in. #4342.", conversation, role, run),
        "ownedBy": format!("principal-{}", role),
        "conversationOf": run,
        "conversationId": conversation,
    }))
}

/// #4344 — the three credential files a role holds, as Credential rows: what
/// kind, where held, what it grants, when it last changed. Never a value: the
/// file is named, not read. `mtimes` is (file name, ISO mtime) for the files
/// that exist.
pub fn credential_rows(role: &str, identity_dir: &str, mtimes: &[(String, String)]) -> Vec<Value> {
    let kinds = [("cred.json", "css-client", "the CSS client it signs in with"),
                 ("nostr.json", "nostr-key", "its nostr keypair (the public half is a KeyRegistryEntry)"),
                 ("token.cache", "access-token", "the current 10-minute access token")];
    kinds.iter().filter_map(|(file, kind, what)| {
        let (_, at) = mtimes.iter().find(|(f, _)| f == file)?;
        Some(json!({
            "name": format!("{}-{}", role, kind),
            "label": format!("{} {}", role, kind),
            "comment": format!("{}'s {}: {}. Held in {}; the value never enters the store. #4344.", role, kind, what, file),
            "ownedBy": format!("principal-{}", role),
            "credentialKind": kind,
            "source": format!("{}/{}/{}", identity_dir, role, file),
            "scope": "urn:chorus:identity",
            "rotatedAt": at,
        }))
    }).collect()
}

/// The session row as a login writes it now: #4202's row plus who it acts as
/// and when it started.
pub fn with_role_and_start(mut session: Value, role: &str, started: &str) -> Value {
    // the BARE name: the mint adds "role-" and refuses a name that already has it
    // (live 2026-09-26 09:30: 422 double-prefix on 'role-silas')
    session["actsAs"] = Value::String(role.into());
    session["startedAt"] = Value::String(started.into());
    session["lastSeenAt"] = Value::String(started.into());
    session
}

/// A run that ended: `reason` is exit, logout, restart, refusal or crash
/// (the SessionRunShape pattern). None for any other word, so a typo cannot
/// reach the store as a row the shape refuses.
pub fn ended_run(mut run: Value, ended: &str, reason: &str) -> Option<Value> {
    if !matches!(reason, "exit" | "logout" | "restart" | "refusal" | "crash") { return None; }
    run.get("name")?;
    run["runEndedAt"] = Value::String(ended.into());
    run["endReason"] = Value::String(reason.into());
    Some(run)
}

/// The session with this turn's time. One PUT, never a new row.
pub fn seen_session(session: Value, now: &str) -> Option<Value> {
    session.get("name")?;
    let mut session = bare_role(session);
    session["lastSeenAt"] = Value::String(now.into());
    Some(session)
}

/// A delivery landed: the presence is reachable, and says when.
pub fn delivered_presence(mut presence: Value, now: &str) -> Option<Value> {
    presence.get("name")?;
    presence["reachability"] = Value::String("reachable".into());
    presence["lastDeliveredAt"] = Value::String(now.into());
    Some(presence)
}

/// A presence whose run ended can no longer be reached there.
pub fn gone_presence(mut presence: Value) -> Option<Value> {
    presence.get("name")?;
    presence["reachability"] = Value::String("unreachable".into());
    Some(presence)
}

/// The run with its real transcript id, once a turn has told us.
pub fn with_conversation(mut run: Value, conversation: &str) -> Option<Value> {
    if conversation.is_empty() || run.get("conversationId").and_then(|c| c.as_str()) == Some(conversation) { return None; }
    run["conversationId"] = Value::String(conversation.into());
    Some(run)
}

/// #4339 — the only thing pulse types into a pane for a nudge. Must equal
/// `WAKE_LINE` in platform/pulse/src/delivery-worker.ts and
/// chorus-hooks' shared::wake.
pub const WAKE_LINE: &str = "[chorus] a message is waiting in your context under Pending nudges";

/// What one UserPromptSubmit turn tells `seen`: the conversation id, and
/// whether the prompt WAS a delivered message. Since #4339 a nudge arrives as
/// pulse's wake line, never as its words, so a "[nudge from" label typed into
/// the pane is Jeff's text, not a delivery. A message that never arrived never
/// produces a turn, so it can never mark the presence reachable.
pub fn turn_facts(hook_input: &str) -> (String, bool) {
    let v: Value = serde_json::from_str(hook_input).unwrap_or(Value::Null);
    let conv = v.get("session_id").and_then(|s| s.as_str()).unwrap_or("").to_string();
    let prompt = v.get("prompt").and_then(|s| s.as_str()).unwrap_or("");
    (conv, prompt.trim() == WAKE_LINE)
}

/// #4339 — who a prompt is from. pulse types only WAKE_LINE for a nudge, and
/// the harness hands the role its own notices (task results, stop-hook
/// feedback, reminders) as prompts too. Everything else was typed into the
/// pane, and only Jeff types there: the Clearing types his Clearing words, his
/// terminal types the rest.
#[derive(Debug, PartialEq, Eq)]
pub enum Speaker { Delivery, Harness, Jeff }

pub fn speaker(hook_input: &str) -> Speaker {
    let v: Value = serde_json::from_str(hook_input).unwrap_or(Value::Null);
    let p = v.get("prompt").and_then(|s| s.as_str()).unwrap_or("").trim();
    if p == WAKE_LINE { return Speaker::Delivery; }
    if p.is_empty() || p.starts_with('<') || p.starts_with("Stop hook feedback:") || p.contains("[SYSTEM NOTIFICATION") { return Speaker::Harness; }
    Speaker::Jeff
}

/// Jeff spoke to this session now: say so on the session. One PUT. The edge
/// goes as the bare name: the API adds "principal-" itself and refuses a
/// value that already has it (#4346: 16:40 on 09-26, "double-prefix:
/// 'principal-jeff' already starts with 'principal-'", 422 on every turn).
pub fn attended_session(mut session: Value, principal: &str, now: &str) -> Option<Value> {
    session.get("name")?;
    let bare = principal.strip_prefix("principal-").unwrap_or(principal);
    session["attendedBy"] = Value::String(bare.into());
    session["lastAttendedAt"] = Value::String(now.into());
    Some(session)
}

/// Is a human terminal showing `pane` right now? `clients` is
/// `tmux list-clients -F '#{client_flags}|#{pane_id}'`: one line per attached
/// terminal, naming the pane it shows. tmux's own "focused" flag is not used:
/// on 2026-09-26 all three clients carried it at once.
pub fn pane_shown(clients: &str, pane: &str) -> bool {
    !pane.is_empty() && clients.lines().any(|l| {
        let mut it = l.split('|');
        let flags = it.next().unwrap_or("");
        flags.split(',').any(|f| f == "attached") && it.next() == Some(pane)
    })
}

/// The presence with this reading of whether his terminal is on its pane.
/// None when nothing changed, so a quiet turn is not a PUT.
pub fn focused_presence(mut presence: Value, shown: bool, now: &str) -> Option<Value> {
    presence.get("name")?;
    let want = if shown { "true" } else { "false" };
    if presence.get("focusedNow").and_then(|f| f.as_str()) == Some(want) { return None; }
    presence["focusedNow"] = Value::String(want.into());
    presence["checkedAt"] = Value::String(now.into());
    Some(presence)
}

/// "jeff: here now · last spoke 2026-09-26T18:40:00Z" — the room as the
/// store has it, for `status`. Unknown is said as unknown, never as "not here".
pub fn room_line(session: Option<&Value>, presence: Option<&Value>) -> String {
    let f = |v: Option<&Value>, k: &str| v.and_then(|r| r.get(k)).and_then(|x| x.as_str()).filter(|x| !x.is_empty()).map(String::from);
    let here = match (f(presence, "focusedNow").as_deref(), f(presence, "checkedAt")) {
        (Some("true"), Some(at)) => format!("here now (checked {at})"),
        (Some("false"), Some(at)) => format!("not on this pane (checked {at})"),
        _ => "presence unknown".to_string(),
    };
    let spoke = match (f(session, "attendedBy"), f(session, "lastAttendedAt")) {
        (Some(_), Some(at)) => format!("last spoke {at}"),
        _ => "has not spoken to this session".to_string(),
    };
    format!("jeff: {here} · {spoke}")
}

/// Is a last-seen write due? One every `every_secs`, so a fast back-and-forth
/// is not a PUT per keystroke; a delivery always writes.
pub fn seen_due(last_write_secs: Option<u64>, now_secs: u64, every_secs: u64, delivered: bool) -> bool {
    delivered || last_write_secs.map(|l| now_secs.saturating_sub(l) >= every_secs).unwrap_or(true)
}

/// A row as the listing returns it, made fit to PUT back: the listing adds
/// "status" (not in any shape; 422 "off-model property") and reports every
/// absent field as "" (an empty edge is refused as a name). Both go.
/// #4343 — actsAs as the bare role name, whatever form a row was saved in. A
/// login saved before the fix carries "role-wren"; sent back, every PUT is a 422.
pub fn bare_role(mut row: Value) -> Value {
    if let Some(a) = row.get("actsAs").and_then(|a| a.as_str()).and_then(|a| a.strip_prefix("role-")).map(String::from) {
        row["actsAs"] = Value::String(a);
    }
    row
}

pub fn putable(mut row: Value) -> Value {
    row = bare_role(row);
    if let Some(o) = row.as_object_mut() {
        o.remove("status");
        o.retain(|_, v| !matches!(v, Value::String(s) if s.is_empty()));
    }
    row
}

/// Sessions to close in the sweep: open, past expiry, and not a live login.
/// The 80+ rows from before #4302 read "open" long after their token died.
pub fn expired_open(sessions_listing: &str, now_iso: &str, keep: &[String]) -> Vec<Value> {
    let v: Value = serde_json::from_str(sessions_listing).unwrap_or(Value::Null);
    let Some(rows) = v.get("data").and_then(|d| d.as_array()) else { return vec![] };
    rows.iter().filter(|r| {
        let s = |k: &str| r.get(k).and_then(|x| x.as_str()).unwrap_or("");
        s("sessionState") == "open" && !s("expiresAt").is_empty() && s("expiresAt") < now_iso
            && !keep.iter().any(|k| k == s("name")) && !s("tokenId").is_empty() && s("ownedBy").starts_with("principal-")
            // the token behind a live login expires every 10 minutes; a session
            // that took a turn in the last hour is live whatever its expiresAt says
            && !(!s("lastSeenAt").is_empty() && s("lastSeenAt") >= hour_before(now_iso).as_str())
    }).cloned().collect()
}

/// The ISO time one hour before `iso` (same YYYY-MM-DDTHH:MM:SSZ form), for the
/// string compare above. A malformed time gives "" (so nothing counts as recent).
fn hour_before(iso: &str) -> String {
    let p = |a: usize, b: usize| iso.get(a..b).and_then(|x| x.parse::<i64>().ok());
    let (Some(y), Some(mo), Some(d), Some(h), Some(mi), Some(s)) = (p(0,4), p(5,7), p(8,10), p(11,13), p(14,16), p(17,19)) else { return String::new() };
    // days from civil (Howard Hinnant), then back
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400); let yoe = y2 - era * 400;
    let doy = (153 * (if mo > 2 { mo - 3 } else { mo + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    let secs = days * 86400 + h * 3600 + mi * 60 + s - 3600;
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    let z = days + 719468; let era = z.div_euclid(146097); let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let yy = yoe + era * 400; let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153; let dd = doy - (153 * mp + 2) / 5 + 1;
    let mm = if mp < 10 { mp + 3 } else { mp - 9 }; let yy = if mm <= 2 { yy + 1 } else { yy };
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", yy, mm, dd, rem / 3600, (rem % 3600) / 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn hour_before_crosses_midnight() { assert_eq!(hour_before("2026-09-26T00:30:00Z"), "2026-09-25T23:30:00Z"); assert_eq!(hour_before("2026-03-01T00:10:00Z"), "2026-02-28T23:10:00Z"); }
}

/// One variable from `ps eww` output ("... KEY=value ..."). None when absent.
pub fn env_value(ps_eww: &str, key: &str) -> Option<String> {
    let want = format!("{}=", key);
    ps_eww.split_whitespace().find(|w| w.starts_with(&want)).map(|w| w[want.len()..].to_string()).filter(|v| !v.is_empty())
}
