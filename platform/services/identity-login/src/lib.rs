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
    if login.principal.is_empty() || login.token_id.is_empty() {
        return Err((401, "no verified login".into()));
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
    let name = format!("{}-{}", login.principal, login.token_id);
    row.insert("channel".into(), json!(channel));
    row.insert("ownedBy".into(), json!(format!("principal-{}", login.principal)));
    row.insert("startedBy".into(), json!(DOOR_PRINCIPAL));
    row.insert("startedAt".into(), json!(now_iso));
    row.insert("tokenId".into(), json!(login.token_id));
    row.insert("label".into(), json!(format!("{} {} login", login.principal, channel)));
    Ok(Outcome::Create(name, Value::Object(row)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-11-18T05:20:00Z";
    fn kade() -> VerifiedLogin<'static> { VerifiedLogin { principal: "kade", token_id: "t1" } }
    fn facts() -> PrincipalFacts<'static> { PrincipalFacts { host_account: Some("chorus-kade") } }
    fn serves_kade(r: &str) -> bool { r == "role-kade" }
    fn plan(body: Value) -> Result<Outcome, Refusal> { plan_login(&kade(), &facts(), &body, serves_kade, None, NOW) }

    #[test]
    fn a_pane_login_gets_one_row_owned_by_the_login_and_started_by_the_door() {
        let Outcome::Create(name, row) = plan(json!({"channel":"pane","hostAccount":"chorus-kade","actsAs":"role-kade"})).unwrap()
        else { panic!("expected a row") };
        assert_eq!(name, "kade-t1");
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

    #[test]
    fn a_browser_login_carries_no_host_account_and_is_owned_by_the_person() {
        let jeff = VerifiedLogin { principal: "jeff", token_id: "b7" };
        let r = plan_login(&jeff, &PrincipalFacts { host_account: None }, &json!({"channel":"browser"}), |_| false, None, NOW);
        let Outcome::Create(_, row) = r.unwrap() else { panic!("expected a row") };
        assert_eq!(row["ownedBy"], "principal-jeff");
        assert!(row.get("hostAccount").is_none());
        let bad = plan_login(&jeff, &PrincipalFacts { host_account: None }, &json!({"channel":"browser","hostAccount":"x"}), |_| false, None, NOW);
        assert_eq!(bad.unwrap_err().0, 422);
    }

    #[test]
    fn negative_proof_an_unverified_login_or_an_unknown_channel_is_refused() {
        let anon = VerifiedLogin { principal: "", token_id: "t1" };
        assert_eq!(plan_login(&anon, &facts(), &json!({"channel":"pane"}), serves_kade, None, NOW).unwrap_err().0, 401);
        assert_eq!(plan(json!({"channel":"ssh"})).unwrap_err().0, 422);
    }
}
