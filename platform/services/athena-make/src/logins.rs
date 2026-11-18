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
        Ok(Outcome::Existing(name)) => (200, format!("{{ \"name\": \"{}\", \"created\": false }}", crate::json_escape(&name))),
        Ok(Outcome::Create(name, mut row)) => {
            row["name"] = Value::String(name.clone());
            // The caller learns the row's name from the answer: the door chose it.
            match create(&row.to_string()) {
                (c, _) if (200..300).contains(&c) => (201, format!("{{ \"name\": \"{}\", \"created\": true }}", crate::json_escape(&name))),
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
