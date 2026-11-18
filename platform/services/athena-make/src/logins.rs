//! #4517 AC5 — `POST /identity/logins`: the door writes every Session row.
//!
//! A role keeps its own login and calls this once. The door has already verified the
//! login; this decides the row (identity-login::plan_login) and writes it through the
//! normal create path with the caller's own login, so the DAL's owner rule holds and
//! `startedBy` is the door. Store reads come in as functions so the route is tested
//! without a store; `serve` passes the real ones.
use identity_login::{plan_login, Outcome, PrincipalFacts, VerifiedLogin};
use serde_json::Value;

pub const LOGINS_PATH: &str = "/identity/logins";

/// The verified caller, as the door's verifier resolved it.
pub struct Caller<'a> {
    pub principal: &'a str,
    pub jti: &'a str,
    pub expires_at: &'a str,
}

/// Decide and write. `host_account(principal)` reads the Principal row;
/// `serves(role)` is the door's Teammate rule; `existing(jti)` finds the row this login
/// already has; `create(body)` is the door's own create of one Session row.
pub fn handle_login(
    caller: &Caller,
    body: &str,
    host_account: impl Fn(&str) -> Result<Option<String>, String>,
    serves: impl Fn(&str) -> Result<bool, String>,
    existing: impl Fn(&str) -> Result<Option<String>, String>,
    create: impl Fn(&str) -> (u16, String),
    supersede: impl Fn(&str),
    touch: impl Fn(&str),
    now_iso: &str,
) -> (u16, String) {
    let parsed: Value = match serde_json::from_str(if body.trim().is_empty() { "{}" } else { body }) {
        Ok(v) => v,
        Err(e) => return (422, error("validation", &format!("the login body is not JSON: {e}"))),
    };
    let host = match host_account(caller.principal) {
        Ok(h) => h,
        Err(e) => return (503, error("unavailable", &format!("could not read {}'s Principal row: {e}", caller.principal))),
    };
    let found = if caller.jti.is_empty() { None } else {
        match existing(caller.jti) {
            Ok(f) => f,
            Err(e) => return (503, error("unavailable", &format!("could not read this login's Session row: {e}"))),
        }
    };
    // A store that did not answer is 503, never a "does not serve" refusal (Silas 00:27).
    let acts_as = parsed.get("actsAs").and_then(Value::as_str).unwrap_or("").to_string();
    let served = if acts_as.is_empty() { false } else {
        match serves(&acts_as) {
            Ok(b) => b,
            Err(e) => return (503, error("unavailable", &format!("could not read whether {} serves {acts_as}: {e}", caller.principal))),
        }
    };
    let login = VerifiedLogin { principal: caller.principal, token_id: caller.jti, expires_at: caller.expires_at };
    match plan_login(&login, &PrincipalFacts { host_account: host.as_deref() }, &parsed, |r| r == acts_as && served, found.as_deref(), now_iso) {
        Ok(Outcome::Existing(name)) => {
            // #4517 AC6 (Silas 00:56) — a browser sign-in stays live by its lastSeenAt: its
            // repeat login asks the door to touch it (at most once per window, see touch_due)
            if parsed.get("channel").and_then(Value::as_str) == Some("browser") { touch(&name); }
            (200, format!("{{ \"name\": \"{}\", \"created\": false }}", crate::json_escape(&name)))
        }
        Ok(Outcome::Create(name, mut row)) => {
            row["name"] = Value::String(name.clone());
            // The caller learns the row's name from the answer: the door chose it.
            let pane = row["channel"] == "pane";
            match create(&row.to_string()) {
                (c, _) if (200..300).contains(&c) => {
                    // #4517 AC6 (Silas 00:54) — one pane per role: only after the new row is
                    // written, end the role's older open pane rows. A browser login never
                    // supersedes: Jeff's phone and desktop are both live.
                    if pane { supersede(&name); }
                    (201, format!("{{ \"name\": \"{}\", \"created\": true }}", crate::json_escape(&name)))
                }
                other => other,
            }
        }
        Err((code, why)) => (code, error(if code == 401 { "authn" } else if code == 403 { "authz" } else { "validation" }, &why)),
    }
}

fn error(kind: &str, message: &str) -> String {
    format!("{{ \"error\": \"{}\", \"message\": \"{}\" }}", kind, crate::json_escape(message))
}

/// Seconds since the epoch as ISO UTC (`2026-11-18T05:30:00Z`), with no clock or process.
pub fn iso_from_secs(secs: u64) -> String {
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// The door's own store reads for a login, by SPARQL against its store.
pub fn principal_host_account(principal: &str) -> Result<Option<String>, String> {
    if !principal.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') { return Ok(None); }
    let q = format!("PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH ?g {{ <{ns}principal-{p}> chorus:hostAccount ?v }} }} LIMIT 1",
        ns = crate::NS, p = principal);
    crate::sparql_json(&q).map(|b| crate::select_v(&b).into_iter().next())
}

/// The Session row this login already has, by its tokenId, if any.
pub fn session_for_login(jti: &str) -> Result<Option<String>, String> {
    if !jti.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)) { return Err("login id carries characters the door will not query".into()); }
    let q = format!("PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <urn:chorus:domains:identity> {{ ?s a chorus:Session ; chorus:tokenId ?t . FILTER(STR(?t) = \"{j}\") BIND(REPLACE(STR(?s), '.*#session-', '') AS ?v) }} }} LIMIT 1",
        ns = crate::NS, j = jti);
    crate::sparql_json(&q).map(|b| crate::select_v(&b).into_iter().next())
}

#[cfg(test)]
mod tests {
    #[test]
    fn iso_from_secs_matches_known_instants() {
        assert_eq!(super::iso_from_secs(0), "1970-01-01T00:00:00Z");
        assert_eq!(super::iso_from_secs(1_794_978_600), "2026-11-18T05:10:00Z");
        assert_eq!(super::iso_from_secs(951_782_400), "2000-02-29T00:00:00Z");
    }
}

pub const LOGOUTS_PATH: &str = "/identity/logouts";

/// What the door reads about a Session row before ending it.
pub struct SessionFacts {
    /// the row's owner, e.g. "principal-kade"
    pub owner: String,
    pub ended: bool,
}

/// #4517 AC6 — `POST /identity/logouts {"session": <name>}`: the row's owner ends its own
/// session; the door stamps endedAt, endedBy door and sessionState closed. A repeat is 200
/// with nothing written. `read(name)` reads the row; `end(name, patch)` is the door's own
/// patch of it with the caller's login.
pub fn handle_logout(
    principal: &str,
    body: &str,
    read: impl Fn(&str) -> Result<Option<SessionFacts>, String>,
    end: impl Fn(&str, &str) -> (u16, String),
    now_iso: &str,
) -> (u16, String) {
    let v: Value = match serde_json::from_str(body) { Ok(v) => v, Err(e) => return (422, error("validation", &format!("the logout body is not JSON: {e}"))) };
    let Some(obj) = v.as_object() else { return (422, error("validation", "the logout body must be a JSON object")) };
    if let Some(k) = obj.keys().find(|k| k.as_str() != "session") {
        return (422, error("validation", &format!("'{k}' is not the caller's to say; the door stamps how a session ends")));
    }
    let name = obj.get("session").and_then(Value::as_str).unwrap_or("");
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return (422, error("validation", "a logout names one session by its row name"));
    }
    let facts = match read(name) {
        Ok(Some(f)) => f,
        Ok(None) => return (404, error("not-found", &format!("no Session row '{name}'"))),
        Err(e) => return (503, error("unavailable", &format!("could not read Session '{name}': {e}"))),
    };
    if facts.owner.rsplit('#').next().unwrap_or("") != format!("principal-{principal}") {
        return (403, error("authz", &format!("{principal} does not own session '{name}'; only its owner ends it")));
    }
    if facts.ended {
        return (200, format!("{{ \"session\": \"{}\", \"ended\": false }}", crate::json_escape(name)));
    }
    let patch = format!("{{\"endedAt\":\"{now_iso}\",\"endedBy\":\"door\",\"sessionState\":\"closed\"}}");
    match end(name, &patch) {
        (c, _) if (200..300).contains(&c) => (200, format!("{{ \"session\": \"{}\", \"ended\": true }}", crate::json_escape(name))),
        other => other,
    }
}

/// The door's own read of a Session row's owner and whether it has ended.
pub fn session_facts(name: &str) -> Result<Option<SessionFacts>, String> {
    let q = format!("PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <urn:chorus:domains:identity> {{ <{ns}session-{n}> a chorus:Session . OPTIONAL {{ <{ns}session-{n}> chorus:ownedBy ?o }} OPTIONAL {{ <{ns}session-{n}> chorus:endedAt ?e }} BIND(CONCAT(COALESCE(STR(?o), \"\"), \"|\", IF(BOUND(?e), \"ended\", \"open\")) AS ?v) }} }} LIMIT 1",
        ns = crate::NS, n = name);
    let rows = crate::select_v(&crate::sparql_json(&q)?);
    Ok(rows.first().map(|r| {
        let (owner, state) = r.split_once('|').unwrap_or((r.as_str(), "open"));
        SessionFacts { owner: owner.to_string(), ended: state == "ended" }
    }))
}


/// The role's open pane rows on the same hostAccount other than `keep`: the ones a new
/// pane login supersedes.
pub fn older_open_panes(principal: &str, host: &str, keep: &str) -> Result<Vec<String>, String> {
    if !principal.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') || host.contains('"') { return Ok(vec![]); }
    let q = format!("PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <urn:chorus:domains:identity> {{ ?s a chorus:Session ; chorus:ownedBy <{ns}principal-{p}> ; chorus:channel ?c ; chorus:hostAccount ?h . FILTER(STR(?c) = \"pane\" && STR(?h) = \"{h}\") FILTER NOT EXISTS {{ ?s chorus:endedAt ?e }} BIND(REPLACE(STR(?s), '.*#session-', '') AS ?v) }} }}",
        ns = crate::NS, p = principal, h = host);
    Ok(crate::select_v(&crate::sparql_json(&q)?).into_iter().filter(|n| n != keep).collect())
}


/// The window a browser row's lastSeenAt is touched in, at most once (Silas 01:31).
pub const TOUCH_WINDOW_SECS: u64 = 300;

/// Is a touch due: the row was last seen at `last_secs` (None = never), now is `now_secs`.
pub fn touch_due(last_secs: Option<u64>, now_secs: u64) -> bool {
    last_secs.is_none_or(|l| now_secs.saturating_sub(l) >= TOUCH_WINDOW_SECS)
}

/// Seconds since the epoch from an ISO UTC instant (`2026-11-18T05:30:00Z`); None if not one.
pub fn secs_from_iso(iso: &str) -> Option<u64> {
    let b = iso.as_bytes();
    if b.len() < 20 || b[4] != b'-' || b[10] != b'T' { return None; }
    let n = |r: std::ops::Range<usize>| iso.get(r)?.parse::<i64>().ok();
    let (y, m, d, hh, mm, ss) = (n(0..4)?, n(5..7)?, n(8..10)?, n(11..13)?, n(14..16)?, n(17..19)?);
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(days * 86_400 + hh * 3600 + mm * 60 + ss).ok()
}

/// The browser row's lastSeenAt, read by the door.
pub fn session_last_seen(name: &str) -> Result<Option<String>, String> {
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') { return Ok(None); }
    let q = format!("PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <urn:chorus:domains:identity> {{ <{ns}session-{n}> chorus:lastSeenAt ?t . BIND(STR(?t) AS ?v) }} }} LIMIT 1",
        ns = crate::NS, n = name);
    crate::sparql_json(&q).map(|b| crate::select_v(&b).into_iter().next())
}

#[cfg(test)]
mod touch_tests {
    use super::*;
    #[test]
    fn iso_round_trips_and_the_window_holds() {
        assert_eq!(secs_from_iso("2026-11-18T05:10:00Z"), Some(1_794_978_600));
        assert_eq!(secs_from_iso(&iso_from_secs(951_782_400)), Some(951_782_400));
        assert_eq!(secs_from_iso("not a time"), None);
        assert!(touch_due(None, 10));
        assert!(!touch_due(Some(1000), 1000 + TOUCH_WINDOW_SECS - 1), "inside the window: no write");
        assert!(touch_due(Some(1000), 1000 + TOUCH_WINDOW_SECS));
    }
}

/// What the door reads about a browser row a renewal names: owner, open, expiresAt.
pub fn renewed_row_facts(name: &str) -> Result<Option<(String, bool, String)>, String> {
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') { return Ok(None); }
    let q = format!("PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <urn:chorus:domains:identity> {{ <{ns}session-{n}> a chorus:Session ; chorus:channel ?c . FILTER(STR(?c) = \"browser\") OPTIONAL {{ <{ns}session-{n}> chorus:ownedBy ?o }} OPTIONAL {{ <{ns}session-{n}> chorus:endedAt ?e }} OPTIONAL {{ <{ns}session-{n}> chorus:expiresAt ?x }} BIND(CONCAT(COALESCE(STR(?o), \"\"), \"|\", IF(BOUND(?e), \"ended\", \"open\"), \"|\", COALESCE(STR(?x), \"\")) AS ?v) }} }} LIMIT 1",
        ns = crate::NS, n = name);
    let rows = crate::select_v(&crate::sparql_json(&q)?);
    Ok(rows.first().map(|r| {
        let mut it = r.splitn(3, '|');
        let (o, st, x) = (it.next().unwrap_or(""), it.next().unwrap_or("open"), it.next().unwrap_or(""));
        (o.to_string(), st == "open", x.to_string())
    }))
}

/// #4517 AC6 (Silas 01:34) — `POST /identity/logins {"channel":"browser","renews":<name>}`:
/// his refreshed login renews his own open browser row instead of writing a new one.
pub fn handle_renewal(
    caller: &Caller,
    body: &str,
    read: impl Fn(&str) -> Result<Option<(String, bool, String)>, String>,
    write: impl Fn(&str, &str) -> (u16, String),
    now_iso: &str,
) -> (u16, String) {
    let parsed: Value = match serde_json::from_str(body) { Ok(v) => v, Err(e) => return (422, error("validation", &format!("the login body is not JSON: {e}"))) };
    let name = parsed.get("renews").and_then(Value::as_str).unwrap_or("").to_string();
    let facts = match read(&name) {
        Ok(f) => f,
        Err(e) => return (503, error("unavailable", &format!("could not read Session '{name}': {e}"))),
    };
    let row = facts.as_ref().map(|(o, open, x)| identity_login::RenewedRow { owner: o, open: *open, expires_at: x });
    let login = VerifiedLogin { principal: caller.principal, token_id: caller.jti, expires_at: caller.expires_at };
    match identity_login::plan_renewal(&login, &parsed, row.as_ref(), now_iso) {
        Ok((name, patch)) => match write(&name, &patch.to_string()) {
            (c, _) if (200..300).contains(&c) => (200, format!("{{ \"name\": \"{}\", \"renewed\": true }}", crate::json_escape(&name))),
            other => other,
        },
        Err((code, why)) => (code, error(match code { 401 => "authn", 403 => "authz", 404 => "not-found", 409 => "conflict", _ => "validation" }, &why)),
    }
}
