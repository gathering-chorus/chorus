//! #4517 AC5 — the door writes every Session row; this decides which row.
//!
//! Jeff, 2026-11-17 19:35: "we have done 10-20 cards trying to fix sessions over time."
//! The cause: each role wrote its own Session row after its own login, so a refused role
//! could not record its refusal and a failed write left status guessing. Now the role
//! keeps its own login (its own credential, #4933 AC3) and calls the door once; the door
//! verifies the login and writes the row as itself.
//!
//! The door trusts nothing in the body it can check itself (Silas, 2026-11-18 00:11):
//!   ownedBy      the verified login's principal (stamped by the door's create), never the body
//!   hostAccount  must equal the Principal row's hostAccount (pane); browser carries none
//!   actsAs       only a Role the caller's Teammate serves
//!   tokenId      one row per login: a repeat returns the same row
//!   startedBy    the person present, only when the door finds his open browser Session
//!                (#4412); a person at his own terminal is startReportedBy (role-reported)

use serde_json::{json, Map, Value};

/// #4412, #4517 (Jeff 11-18 "yes", Silas 06:22) — who was present at a login or logout.
/// `browser`: the door found his open, unexpired browser Session in its own rows, so it
/// stamps startedBy/endedBy (he was present; not proof he typed the command). `terminal`: the
/// role reports him at his own Mac account; the door cannot verify that, so it is labelled
/// startReportedBy/endReportedBy, never startedBy/endedBy. Links are bare names (the DAL
/// mints `principal-`, ADR-040 Rule 0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Moment { Start, End }

pub fn person_stamp(obj: &Map<String, Value>, facts: &PrincipalFacts, moment: Moment) -> Result<Option<(&'static str, String)>, Refusal> {
    let person = obj.get("person").and_then(Value::as_str).unwrap_or("");
    let at = obj.get("personAt").and_then(Value::as_str).unwrap_or("");
    if person.is_empty() && at.is_empty() { return Ok(None); }
    let person = person.strip_prefix("principal-").unwrap_or(person);
    if person.is_empty() { return Err((422, "personAt needs a person".into())); }
    if !facts.persons.contains(&person) { return Err((403, format!("'{person}' is not a person principal"))); }
    let (verified, reported) = match moment { Moment::Start => ("startedBy", "startReportedBy"), Moment::End => ("endedBy", "endReportedBy") };
    match at {
        "browser" if facts.present.contains(&person) => Ok(Some((verified, person.to_string()))),
        "browser" => Err((403, format!("{person} has no open browser Session: sign in to the Clearing, or say personAt terminal"))),
        "terminal" => Ok(Some((reported, person.to_string()))),
        other => Err((422, format!("personAt must be browser or terminal, not '{other}'"))),
    }
}

/// A login the door has already verified (signature, issuer, expiry).
pub struct VerifiedLogin<'a> {
    /// local principal name, e.g. "kade" or "jeff"
    pub principal: &'a str,
    /// the login's own id (the token's jti); one Session row per login
    pub token_id: &'a str,
    /// when the login itself expires (the token's exp), ISO UTC
    pub expires_at: &'a str,
}

/// The row's name: principal plus a stable hash of the login id. A jti may carry dots or
/// slashes, which the door refuses as an entity name (Silas 00:14), so it is never used raw.
pub fn session_name(principal: &str, token_id: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325; // FNV-1a 64
    for b in token_id.bytes() { h ^= u64::from(b); h = h.wrapping_mul(0x100000001b3); }
    format!("{principal}-{h:016x}")
}

/// What the door holds about that principal, read from its Principal row.
pub struct PrincipalFacts<'a> {
    pub host_account: Option<&'a str>,
    /// every person principal's bare name
    pub persons: &'a [&'a str],
    /// the persons with an open, unexpired browser Session, read by the door from its own rows
    pub present: &'a [&'a str],
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// write this row (name, body)
    Create(String, Value),
    /// this login already has its row; return it, write nothing
    Existing(String),
}

/// A refusal: HTTP status and the reason in words.
pub type Refusal = (u16, String);

const ALLOWED: [&str; 5] = ["channel", "hostAccount", "actsAs", "person", "personAt"];

/// What the door read about the browser row a renewal names.
pub struct RenewedRow<'a> {
    pub owner: &'a str,
    pub open: bool,
    /// the row's expiresAt as ISO UTC
    pub expires_at: &'a str,
}

/// Decide the one Session row a verified login gets.
///
/// `serves(role)` answers whether the caller's Teammate serves that Role (the door's
/// acts_as rule). `existing` is the name of the row already written for this tokenId.
pub fn plan_login(
    login: &VerifiedLogin,
    facts: &PrincipalFacts,
    body: &Value,
    serves: impl Fn(&str) -> bool,
    existing: Option<&str>,
    now_iso: &str,
) -> Result<Outcome, Refusal> {
    if login.principal.is_empty() {
        return Err((401, "no verified login".into()));
    }
    if login.token_id.is_empty() {
        return Err((401, "login has no id: one Session row per login needs one, never an empty key".into()));
    }
    let obj = body.as_object().ok_or((422, "the body must be a JSON object".to_string()))?;
    if let Some(k) = obj.keys().find(|k| !ALLOWED.contains(&k.as_str())) {
        return Err((422, format!("'{k}' is not the caller's to say; the door sets it")));
    }
    if let Some(name) = existing {
        return Ok(Outcome::Existing(name.to_string()));
    }
    let text = |k: &str| obj.get(k).and_then(Value::as_str).unwrap_or("");
    let channel = text("channel");
    let mut row = Map::new();
    match channel {
        "pane" => {
            let claimed = text("hostAccount");
            match facts.host_account {
                Some(h) if !h.is_empty() && h == claimed => { row.insert("hostAccount".into(), json!(h)); }
                Some(h) if !h.is_empty() => {
                    return Err((403, format!("hostAccount '{claimed}' is not {}'s account '{h}'", login.principal)));
                }
                _ => return Err((403, format!("{} has no hostAccount on its Principal row; a pane login needs one", login.principal))),
            }
        }
        "browser" => {
            if obj.contains_key("hostAccount") {
                return Err((422, "a browser login carries no hostAccount; the door reads it from the Principal".into()));
            }
            // SessionShape requires one hostAccount: the door takes it from the Principal row
            match facts.host_account {
                Some(h) if !h.is_empty() => { row.insert("hostAccount".into(), json!(h)); }
                _ => return Err((403, format!("{} has no hostAccount on its Principal row; a Session needs one", login.principal))),
            }
        }
        other => return Err((422, format!("channel must be pane or browser, not '{other}'"))),
    }
    let acts_as = text("actsAs");
    if !acts_as.is_empty() {
        if !serves(acts_as) {
            return Err((403, format!("{} does not serve '{acts_as}'", login.principal)));
        }
        // actsAs is an edge to a Role: the DAL mints "role-", so the row carries the bare name
        // (a prefixed one is refused as a double prefix, #4343)
        row.insert("actsAs".into(), json!(acts_as.strip_prefix("role-").unwrap_or(acts_as)));
    }
    let name = session_name(login.principal, login.token_id);
    row.insert("channel".into(), json!(channel));
    // no ownedBy: the door's create stamps the verified caller as owner and ignores any in the body
    if let Some((field, who)) = person_stamp(obj, facts, Moment::Start)? { row.insert(field.into(), json!(who)); }
    row.insert("startedAt".into(), json!(now_iso));
    row.insert("tokenId".into(), json!(login.token_id));
    row.insert("sessionState".into(), json!("open"));
    row.insert("expiresAt".into(), json!(login.expires_at));
    row.insert("label".into(), json!(format!("{} {} login", login.principal, channel)));
    Ok(Outcome::Create(name, Value::Object(row)))
}

/// #4517 AC6 (Silas 01:34) — a browser sign-in renews its own open row when its login is
/// refreshed: a fresh verified login, the body `{"channel":"browser","renews":<name>}`.
/// Only the login's own principal's row, only a browser row, only while it is open and not
/// yet expired (an expired row is never revived). The row's expiresAt becomes the new
/// login's, never later, and its tokenId moves to the new login. Returns (name, patch).
pub fn plan_renewal(login: &VerifiedLogin, body: &Value, row: Option<&RenewedRow>, now_iso: &str) -> Result<(String, Value), Refusal> {
    if login.principal.is_empty() { return Err((401, "no verified login".into())); }
    if login.token_id.is_empty() { return Err((401, "login has no id: a renewal moves the row to it".into())); }
    let obj = body.as_object().ok_or((422, "the body must be a JSON object".to_string()))?;
    if let Some(k) = obj.keys().find(|k| !["channel", "renews"].contains(&k.as_str())) {
        return Err((422, format!("'{k}' is not the caller's to say on a renewal")));
    }
    if obj.get("channel").and_then(Value::as_str) != Some("browser") {
        return Err((422, "only a browser sign-in renews; a pane logs in again".into()));
    }
    let name = obj.get("renews").and_then(Value::as_str).unwrap_or("");
    let Some(r) = row else { return Err((404, format!("no Session row '{name}' to renew"))) };
    if r.owner.rsplit('#').next().unwrap_or("") != format!("principal-{}", login.principal) {
        return Err((403, format!("'{name}' is not {}'s to renew", login.principal)));
    }
    if !r.open || r.expires_at.is_empty() || r.expires_at <= now_iso {
        return Err((409, format!("'{name}' has ended or expired; an expired row is never revived, sign in again")));
    }
    Ok((name.to_string(), json!({ "lastSeenAt": now_iso, "expiresAt": login.expires_at, "tokenId": login.token_id })))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-11-18T05:20:00Z";
    fn kade() -> VerifiedLogin<'static> { VerifiedLogin { principal: "kade", token_id: "t1", expires_at: "2026-11-18T05:30:00Z" } }
    fn facts() -> PrincipalFacts<'static> { PrincipalFacts { host_account: Some("chorus-kade"), persons: &["jeff"], present: &["jeff"] } }
    fn serves_kade(r: &str) -> bool { r == "role-kade" }
    fn plan(body: Value) -> Result<Outcome, Refusal> { plan_login(&kade(), &facts(), &body, serves_kade, None, NOW) }

    #[test]
    fn a_pane_login_gets_one_row_owned_by_the_login_and_started_by_the_door() {
        let Outcome::Create(name, row) = plan(json!({"channel":"pane","hostAccount":"chorus-kade","actsAs":"role-kade"})).unwrap()
        else { panic!("expected a row") };
        assert_eq!(name, session_name("kade", "t1"));
        assert_eq!(row["sessionState"], "open");
        assert_eq!(row["expiresAt"], "2026-11-18T05:30:00Z");
        assert!(row.get("ownedBy").is_none(), "the door's create stamps the owner from the login");
        assert_eq!(row["actsAs"], "kade", "a bare role name: the DAL mints role-");
        assert!(row.get("startedBy").is_none(), "nobody named present: no startedBy");
        assert_eq!(row["tokenId"], "t1");
        assert_eq!(row["hostAccount"], "chorus-kade");
    }

    #[test]
    fn negative_proof_the_body_cannot_name_the_owner_or_the_writer() {
        for k in ["ownedBy", "startedBy", "tokenId", "startedAt", "name"] {
            let r = plan(json!({"channel":"pane","hostAccount":"chorus-kade", k: "principal-silas"}));
            assert_eq!(r.unwrap_err().0, 422, "{k} from the body must be refused");
        }
    }

    #[test]
    fn negative_proof_a_wrong_or_missing_host_account_is_refused() {
        assert_eq!(plan(json!({"channel":"pane","hostAccount":"chorus-silas"})).unwrap_err().0, 403);
        assert_eq!(plan(json!({"channel":"pane"})).unwrap_err().0, 403);
        let none = PrincipalFacts { host_account: None, persons: &["jeff"], present: &["jeff"] };
        let r = plan_login(&kade(), &none, &json!({"channel":"pane","hostAccount":""}), serves_kade, None, NOW);
        assert_eq!(r.unwrap_err().0, 403, "a principal with no hostAccount cannot open a pane session");
    }

    #[test]
    fn negative_proof_a_foreign_acts_as_is_refused() {
        let r = plan(json!({"channel":"pane","hostAccount":"chorus-kade","actsAs":"role-silas"}));
        assert_eq!(r.unwrap_err().0, 403);
    }

    #[test]
    fn a_repeat_of_the_same_login_returns_its_row_and_writes_nothing() {
        let r = plan_login(&kade(), &facts(), &json!({"channel":"pane","hostAccount":"chorus-kade"}), serves_kade, Some("kade-t1"), NOW);
        assert_eq!(r.unwrap(), Outcome::Existing("kade-t1".into()));
    }

    /// The door refuses entity names with dots or slashes; a jti may carry them.
    #[test]
    fn negative_proof_a_dotted_login_id_never_reaches_the_row_name() {
        let n = session_name("kade", "abc.def/ghi");
        assert!(n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'), "{n}");
        assert_ne!(session_name("kade", "a.b"), session_name("kade", "a/b"), "different logins, different rows");
        assert_eq!(session_name("kade", "a.b"), session_name("kade", "a.b"), "the same login, the same row");
    }

    /// A repeat answers before the channel, hostAccount and actsAs checks. Harmless: it
    /// writes nothing and returns only the row this verified login already owns (Silas
    /// 00:14, point 3). A field the caller may never set is still refused first.
    #[test]
    fn a_repeat_skips_the_row_checks_and_writes_nothing_but_still_refuses_foreign_fields() {
        let r = plan_login(&kade(), &facts(), &json!({"channel":"ssh","hostAccount":"chorus-silas"}), serves_kade, Some("kade-1"), NOW);
        assert_eq!(r.unwrap(), Outcome::Existing("kade-1".into()));
        let foreign = plan_login(&kade(), &facts(), &json!({"ownedBy":"x"}), serves_kade, Some("kade-1"), NOW);
        assert_eq!(foreign.unwrap_err().0, 422);
    }

    #[test]
    fn a_browser_login_takes_its_host_account_from_the_principal_and_is_owned_by_the_person() {
        let jeff = VerifiedLogin { principal: "jeff", token_id: "b7", expires_at: "x" };
        let his = PrincipalFacts { host_account: Some("jeffbridwell"), persons: &["jeff"], present: &["jeff"] };
        let r = plan_login(&jeff, &his, &json!({"channel":"browser"}), |_| false, None, NOW);
        let Outcome::Create(_, row) = r.unwrap() else { panic!("expected a row") };
        assert!(row.get("ownedBy").is_none());
        assert_eq!(row["hostAccount"], "jeffbridwell", "the shape requires it; the door takes it from his Principal");
        let bad = plan_login(&jeff, &his, &json!({"channel":"browser","hostAccount":"x"}), |_| false, None, NOW);
        assert_eq!(bad.unwrap_err().0, 422, "the body never names the account");
        // NEGATIVE PROOF: no hostAccount on his Principal, no row the shape would refuse
        let none = plan_login(&jeff, &PrincipalFacts { host_account: None, persons: &["jeff"], present: &["jeff"] }, &json!({"channel":"browser"}), |_| false, None, NOW);
        assert_eq!(none.unwrap_err().0, 403);
    }

    #[test]
    fn negative_proof_an_unverified_login_or_an_unknown_channel_is_refused() {
        let anon = VerifiedLogin { principal: "", token_id: "t1", expires_at: "x" };
        assert_eq!(plan_login(&anon, &facts(), &json!({"channel":"pane"}), serves_kade, None, NOW).unwrap_err().0, 401);
        assert_eq!(plan(json!({"channel":"ssh"})).unwrap_err().0, 422);
    }

    /// NEGATIVE PROOF (Silas 00:22): a verified login without an id is refused, never
    /// keyed on an empty string — two such logins would otherwise share one row.
    #[test]
    fn negative_proof_a_login_with_no_id_is_refused() {
        let no_id = VerifiedLogin { principal: "kade", token_id: "", expires_at: "x" };
        let (code, why) = plan_login(&no_id, &facts(), &json!({"channel":"pane","hostAccount":"chorus-kade"}), serves_kade, None, NOW).unwrap_err();
        assert_eq!(code, 401);
        assert!(why.contains("login has no id"), "{why}");
    }

    mod renewal {
        use super::super::*;
        const NOW: &str = "2026-11-18T06:40:00Z";
        fn jeff() -> VerifiedLogin<'static> { VerifiedLogin { principal: "jeff", token_id: "b2", expires_at: "2026-11-18T07:40:00Z" } }
        fn body() -> Value { json!({"channel":"browser","renews":"jeff-b"}) }
        fn open_row(owner: &'static str) -> RenewedRow<'static> { RenewedRow { owner, open: true, expires_at: "2026-11-18T07:00:00Z" } }

        #[test]
        fn his_open_browser_row_takes_the_new_logins_expiry_and_id() {
            let (name, patch) = plan_renewal(&jeff(), &body(), Some(&open_row("principal-jeff")), NOW).unwrap();
            assert_eq!(name, "jeff-b");
            assert_eq!(patch["expiresAt"], "2026-11-18T07:40:00Z", "the new login's exp, never later");
            assert_eq!(patch["tokenId"], "b2");
            assert_eq!(patch["lastSeenAt"], NOW);
        }

        /// NEGATIVE PROOFS (Silas 01:34): another's row, an expired or ended row, a pane,
        /// a missing row and an extra field are all refused.
        #[test]
        fn negative_proofs_anothers_expired_ended_pane_missing_and_extra_are_refused() {
            assert_eq!(plan_renewal(&jeff(), &body(), Some(&open_row("principal-kade")), NOW).unwrap_err().0, 403);
            let expired = RenewedRow { owner: "principal-jeff", open: true, expires_at: "2026-11-18T06:39:59Z" };
            assert_eq!(plan_renewal(&jeff(), &body(), Some(&expired), NOW).unwrap_err().0, 409);
            let ended = RenewedRow { owner: "principal-jeff", open: false, expires_at: "2026-11-18T07:00:00Z" };
            assert_eq!(plan_renewal(&jeff(), &body(), Some(&ended), NOW).unwrap_err().0, 409);
            assert_eq!(plan_renewal(&jeff(), &json!({"channel":"pane","renews":"jeff-b"}), Some(&open_row("principal-jeff")), NOW).unwrap_err().0, 422);
            assert_eq!(plan_renewal(&jeff(), &body(), None, NOW).unwrap_err().0, 404);
            assert_eq!(plan_renewal(&jeff(), &json!({"channel":"browser","renews":"jeff-b","expiresAt":"2099"}), Some(&open_row("principal-jeff")), NOW).unwrap_err().0, 422);
        }
    }
}
