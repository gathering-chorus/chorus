//! #4517 AC5 — `POST /identity/logins`: the door writes every Session row.
// @domain: domains — placed by chorus-crawl's domain rules (#4538)
//! A role keeps its own login and calls this once. The door has already verified the
//! login; this decides the row (identity-login::plan_login) and writes it through the
//! normal create path with the caller's own login, so the DAL's owner rule holds and
//! `startedBy` is the door. Store reads come in as functions so the route is tested
//! without a store; `serve` passes the real ones.
use identity_login::{person_stamp, plan_login, Moment, Outcome, PrincipalFacts, VerifiedLogin};
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
    people: impl Fn() -> Result<People, String>,
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
    let present = match read_people(&parsed, &people) { Ok(p) => p, Err(r) => return r };
    let (persons, here) = present.refs();
    match plan_login(&login, &PrincipalFacts { host_account: host.as_deref(), persons: &persons, present: &here }, &parsed, |r| r == acts_as && served, found.as_deref(), now_iso) {
        Ok(Outcome::Existing(name)) => {
            // #4517 AC6 (Silas 00:56) — a browser sign-in stays live by its lastSeenAt: its
            // repeat login asks the door to touch it (at most once per window, see touch_due)
            if parsed.get("channel").and_then(Value::as_str) == Some("browser") { touch(&name); }
            (200, format!("{{ \"name\": \"{}\", \"created\": false }}", crate::json_escape(&name)))
        }
        Ok(Outcome::Create(name, mut row)) => {
            row["name"] = Value::String(name.clone());
            // The caller learns the row's name from the answer: the door chose it.
            // a role's runtime row: a pane, or a supervised agent (Abby, #4424)
            let runtime = row["channel"] == "pane" || row["channel"] == "agent";
            match create(&row.to_string()) {
                (c, _) if (200..300).contains(&c) => {
                    // #4517 AC6 (Silas 00:54, 07:57) — one runtime per role: only after the new
                    // row is written, end the role's older open pane and agent rows (else agent
                    // logins pile up, #4445's "2 sessions for abby"). A browser login never
                    // supersedes: Jeff's phone and desktop are both live.
                    if runtime { supersede(&name); }
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

/// #4517 (Silas 06:46) — Principal facts are read only from the graph that holds the
/// Principal rows (measured 11-18: all 18 principalKind and 16 hostAccount triples), never
/// `GRAPH ?g`: a role able to write some other graph could otherwise mint itself a person or
/// an account. When #3784 moves Principal rows, this one constant moves with them.
pub const PRINCIPAL_GRAPH: &str = "urn:chorus:domains:security";

/// The door's own store reads for a login, by SPARQL against its store.
pub fn principal_host_account(principal: &str) -> Result<Option<String>, String> {
    if !principal.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') { return Ok(None); }
    let q = format!("PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <{pg}> {{ <{ns}principal-{p}> chorus:hostAccount ?v }} }} LIMIT 1",
        ns = crate::NS, p = principal, pg = PRINCIPAL_GRAPH);
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
/// #4367 — a role reports that a person spoke in its session; the door writes it.
pub const ATTENDANCES_PATH: &str = "/identity/attendances";

/// What the door reads about a Session row before ending it.
pub struct SessionFacts {
    /// the row's owner, e.g. "principal-kade"
    pub owner: String,
    pub ended: bool,
    /// the row's expiresAt (ISO UTC), empty when it has none
    pub expires_at: String,
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
    people: impl Fn() -> Result<People, String>,
    now_iso: &str,
) -> (u16, String) {
    let v: Value = match serde_json::from_str(body) { Ok(v) => v, Err(e) => return (422, error("validation", &format!("the logout body is not JSON: {e}"))) };
    let Some(obj) = v.as_object() else { return (422, error("validation", "the logout body must be a JSON object")) };
    if let Some(k) = obj.keys().find(|k| !["session", "person", "personAt"].contains(&k.as_str())) {
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
    // an expired row ends AT its expiresAt, its true end, never later (#4966, Silas 01:39)
    let ended_at = if !facts.expires_at.is_empty() && facts.expires_at.as_str() < now_iso { facts.expires_at.as_str() } else { now_iso };
    // #4412 — the person present at the end, as the door can vouch for him (never "door")
    let present = match read_people(&v, &people) { Ok(p) => p, Err(r) => return r };
    let (persons, here) = present.refs();
    let facts_p = PrincipalFacts { host_account: None, persons: &persons, present: &here };
    let mut patch = serde_json::json!({"endedAt": ended_at, "sessionState": "closed"});
    match person_stamp(obj, &facts_p, Moment::End) {
        Ok(Some((field, who))) => { patch[field] = Value::String(who); }
        Ok(None) => {}
        Err((c, m)) => return (c, error(if c == 403 { "authz" } else { "validation" }, &m)),
    }
    let patch = patch.to_string();
    match end(name, &patch) {
        (c, _) if (200..300).contains(&c) => (200, format!("{{ \"session\": \"{}\", \"ended\": true }}", crate::json_escape(name))),
        other => other,
    }
}

/// The door's own read of a Session row's owner and whether it has ended.
pub fn session_facts(name: &str) -> Result<Option<SessionFacts>, String> {
    let q = format!("PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <urn:chorus:domains:identity> {{ <{ns}session-{n}> a chorus:Session . OPTIONAL {{ <{ns}session-{n}> chorus:ownedBy ?o }} OPTIONAL {{ <{ns}session-{n}> chorus:endedAt ?e }} OPTIONAL {{ <{ns}session-{n}> chorus:expiresAt ?x }} BIND(CONCAT(COALESCE(STR(?o), \"\"), \"|\", IF(BOUND(?e), \"ended\", \"open\"), \"|\", COALESCE(STR(?x), \"\")) AS ?v) }} }} LIMIT 1",
        ns = crate::NS, n = name);
    let rows = crate::select_v(&crate::sparql_json(&q)?);
    Ok(rows.first().map(|r| {
        let mut it = r.splitn(3, '|');
        let (owner, state, x) = (it.next().unwrap_or(""), it.next().unwrap_or("open"), it.next().unwrap_or(""));
        SessionFacts { owner: owner.to_string(), ended: state == "ended", expires_at: x.to_string() }
    }))
}


/// The role's open pane and agent rows on the same hostAccount other than `keep`: the ones
/// a new pane or agent login supersedes.
pub fn older_open_panes(principal: &str, host: &str, keep: &str) -> Result<Vec<String>, String> {
    let Some(q) = older_open_panes_query(principal, host) else { return Ok(vec![]) };
    Ok(crate::select_v(&crate::sparql_json(&q)?).into_iter().filter(|n| n != keep).collect())
}

/// #4517 (ex-#4445) — the query: this principal's own Session rows, pane channel, same
/// hostAccount, not ended. None for a name or host that could break out of the query.
pub fn older_open_panes_query(principal: &str, host: &str) -> Option<String> {
    if principal.is_empty() || !principal.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') || host.contains('"') || host.contains('\\') { return None; }
    Some(format!("PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <urn:chorus:domains:identity> {{ ?s a chorus:Session ; chorus:ownedBy <{ns}principal-{p}> ; chorus:channel ?c ; chorus:hostAccount ?h . FILTER((STR(?c) = \"pane\" || STR(?c) = \"agent\") && STR(?h) = \"{h}\") FILTER NOT EXISTS {{ ?s chorus:endedAt ?e }} BIND(REPLACE(STR(?s), '.*#session-', '') AS ?v) }} }}",
        ns = crate::NS, p = principal, h = host))
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

/// #4517 AC5 — the door records a refused login itself (it is the one that said no): the
/// spine line for a logins/logouts answer, or None when the answer was not a refusal.
/// `principal` is empty when the login itself did not verify.
pub fn refusal_event(code: u16, principal: &str, body: &str) -> Option<Vec<String>> {
    if code != 401 && code != 403 { return None; }
    let why = serde_json::from_str::<Value>(body).ok()
        .and_then(|v| v.get("message").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| format!("HTTP {code}"));
    Some(vec![
        "session.login.refused".into(),
        if principal.is_empty() { "unknown".into() } else { principal.to_string() },
        format!("http={code}"),
        format!("reason={}", why.replace('\n', " ")),
        "by=door".into(),
    ])
}


/// #4412, #4517 — the person principals and those of them with an open, unexpired browser
/// Session, from the door's own rows. Read only when a login or logout names a person.
#[derive(Default, Debug, Clone)]
pub struct People { pub persons: Vec<String>, pub present: Vec<String> }

impl People {
    fn refs(&self) -> (Vec<&str>, Vec<&str>) {
        (self.persons.iter().map(String::as_str).collect(), self.present.iter().map(String::as_str).collect())
    }
}

fn read_people(body: &Value, people: &impl Fn() -> Result<People, String>) -> Result<People, (u16, String)> {
    if body.get("person").is_none() && body.get("personAt").is_none() { return Ok(People::default()); }
    people().map_err(|e| (503, error("unavailable", &format!("could not read who is signed in: {e}"))))
}

/// The door's read: person principals, and those with an open browser Session not yet expired.
pub fn people_present() -> Result<People, String> {
    let now = iso_from_secs(crate::unix_now_secs());
    let persons = crate::select_v(&crate::sparql_json(&persons_query())?);
    let present = crate::select_v(&crate::sparql_json(&people_present_query(&now))?);
    Ok(People { persons, present })
}

/// The query for who is signed in: an open browser Session (no endedAt) whose expiresAt is
/// later than now, owned by a person principal.
pub fn people_present_query(now_iso: &str) -> String {
    format!("PREFIX chorus: <{ns}> SELECT DISTINCT ?v WHERE {{ GRAPH <urn:chorus:domains:identity> {{ ?s a chorus:Session ; chorus:channel ?c ; chorus:ownedBy ?p ; chorus:expiresAt ?x . FILTER(STR(?c) = \"browser\" && STR(?x) > \"{now}\") FILTER NOT EXISTS {{ ?s chorus:endedAt ?e }} }} GRAPH <{pg}> {{ ?p chorus:principalKind ?k . FILTER(STR(?k) = \"person\") }} BIND(REPLACE(STR(?p), '.*#principal-', '') AS ?v) }}",
        ns = crate::NS, now = now_iso.replace('"', ""), pg = PRINCIPAL_GRAPH)
}

/// The person principals, from the Principal graph only.
pub fn persons_query() -> String {
    format!("PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <{pg}> {{ ?p a chorus:Principal ; chorus:principalKind ?k . FILTER(STR(?k) = \"person\") BIND(REPLACE(STR(?p), '.*#principal-', '') AS ?v) }} }}",
        ns = crate::NS, pg = PRINCIPAL_GRAPH)
}

/// #4367, #4517 (Jeff 11-18 "yes") — a role reports that a person spoke in its own open
/// session: body `{"session":<name>,"person":<name>}`. The door writes attendedBy and
/// lastAttendedAt on that row. Role-reported (the door cannot see who typed), so it is only
/// ever the caller's own open row and only ever a person principal.
pub fn handle_attendance(
    principal: &str,
    body: &str,
    read: impl Fn(&str) -> Result<Option<SessionFacts>, String>,
    patch_row: impl Fn(&str, &str) -> (u16, String),
    people: impl Fn() -> Result<People, String>,
    now_iso: &str,
) -> (u16, String) {
    let v: Value = match serde_json::from_str(body) { Ok(v) => v, Err(e) => return (422, error("validation", &format!("the attendance body is not JSON: {e}"))) };
    let Some(obj) = v.as_object() else { return (422, error("validation", "the attendance body must be a JSON object")) };
    if let Some(k) = obj.keys().find(|k| !["session", "person"].contains(&k.as_str())) {
        return (422, error("validation", &format!("'{k}' is not the caller's to say; the door stamps when")));
    }
    let name = obj.get("session").and_then(Value::as_str).unwrap_or("");
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return (422, error("validation", "an attendance names one session by its row name"));
    }
    let person = obj.get("person").and_then(Value::as_str).unwrap_or("");
    let person = person.strip_prefix("principal-").unwrap_or(person);
    match people() {
        Ok(p) if p.persons.iter().any(|x| x == person) => {}
        Ok(_) => return (403, error("authz", &format!("'{person}' is not a person principal"))),
        Err(e) => return (503, error("unavailable", &format!("could not read the person principals: {e}"))),
    }
    let facts = match read(name) {
        Ok(Some(f)) => f,
        Ok(None) => return (404, error("not-found", &format!("no Session row '{name}'"))),
        Err(e) => return (503, error("unavailable", &format!("could not read Session '{name}': {e}"))),
    };
    if facts.owner.rsplit('#').next().unwrap_or("") != format!("principal-{principal}") {
        return (403, error("authz", &format!("{principal} does not own session '{name}'")));
    }
    if facts.ended {
        return (409, error("conflict", &format!("session '{name}' has ended; nobody attends it")));
    }
    let patch = serde_json::json!({"attendedBy": person, "lastAttendedAt": now_iso}).to_string();
    match patch_row(name, &patch) {
        (c, _) if (200..300).contains(&c) => (200, format!("{{ \"session\": \"{}\", \"attended\": true }}", crate::json_escape(name))),
        other => other,
    }
}
