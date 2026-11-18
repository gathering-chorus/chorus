//! #4517 AC5 — the door writes every Session row; this decides which row.
//!
//! Jeff, 2026-11-17 19:35: "we have done 10-20 cards trying to fix sessions over time."
//! The cause: each role wrote its own Session row after its own login, so a refused role
//! could not record its refusal and a failed write left status guessing. Now the role
//! keeps its own login (its own credential, #4933 AC3) and calls the door once; the door
//! verifies the login and writes the row as itself.
//!
//! The door trusts nothing in the body it can check itself (Silas, 2026-11-18 00:11):
//!   ownedBy      the verified login's principal, never the body
//!   hostAccount  must equal the Principal row's hostAccount (pane); browser carries none
//!   actsAs       only a Role the caller's Teammate serves
//!   tokenId      one row per login: a repeat returns the same row

use serde_json::{json, Map, Value};

/// The principal the door writes Session rows as (`startedBy`).
pub const DOOR_PRINCIPAL: &str = "principal-door";

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

const ALLOWED: [&str; 3] = ["channel", "hostAccount", "actsAs"];

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
                return Err((422, "a browser login carries no hostAccount".into()));
            }
        }
        other => return Err((422, format!("channel must be pane or browser, not '{other}'"))),
    }
    let acts_as = text("actsAs");
    if !acts_as.is_empty() {
        if !serves(acts_as) {
            return Err((403, format!("{} does not serve '{acts_as}'", login.principal)));
        }
        row.insert("actsAs".into(), json!(acts_as));
    }
    let name = session_name(login.principal, login.token_id);
    row.insert("channel".into(), json!(channel));
    row.insert("ownedBy".into(), json!(format!("principal-{}", login.principal)));
    row.insert("startedBy".into(), json!(DOOR_PRINCIPAL));
    row.insert("startedAt".into(), json!(now_iso));
    row.insert("tokenId".into(), json!(login.token_id));
    row.insert("sessionState".into(), json!("open"));
    row.insert("expiresAt".into(), json!(login.expires_at));
    row.insert("label".into(), json!(format!("{} {} login", login.principal, channel)));
    Ok(Outcome::Create(name, Value::Object(row)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-11-18T05:20:00Z";
    fn kade() -> VerifiedLogin<'static> { VerifiedLogin { principal: "kade", token_id: "t1", expires_at: "2026-11-18T05:30:00Z" } }
    fn facts() -> PrincipalFacts<'static> { PrincipalFacts { host_account: Some("chorus-kade") } }
    fn serves_kade(r: &str) -> bool { r == "role-kade" }
    fn plan(body: Value) -> Result<Outcome, Refusal> { plan_login(&kade(), &facts(), &body, serves_kade, None, NOW) }

    #[test]
    fn a_pane_login_gets_one_row_owned_by_the_login_and_started_by_the_door() {
        let Outcome::Create(name, row) = plan(json!({"channel":"pane","hostAccount":"chorus-kade","actsAs":"role-kade"})).unwrap()
        else { panic!("expected a row") };
        assert_eq!(name, session_name("kade", "t1"));
        assert_eq!(row["sessionState"], "open");
        assert_eq!(row["expiresAt"], "2026-11-18T05:30:00Z");
        assert_eq!(row["ownedBy"], "principal-kade");
        assert_eq!(row["startedBy"], DOOR_PRINCIPAL);
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
        let none = PrincipalFacts { host_account: None };
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
    fn a_browser_login_carries_no_host_account_and_is_owned_by_the_person() {
        let jeff = VerifiedLogin { principal: "jeff", token_id: "b7", expires_at: "x" };
        let r = plan_login(&jeff, &PrincipalFacts { host_account: None }, &json!({"channel":"browser"}), |_| false, None, NOW);
        let Outcome::Create(_, row) = r.unwrap() else { panic!("expected a row") };
        assert_eq!(row["ownedBy"], "principal-jeff");
        assert!(row.get("hostAccount").is_none());
        let bad = plan_login(&jeff, &PrincipalFacts { host_account: None }, &json!({"channel":"browser","hostAccount":"x"}), |_| false, None, NOW);
        assert_eq!(bad.unwrap_err().0, 422);
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
}
