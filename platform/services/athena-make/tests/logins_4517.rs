// @test-type: unit — hermetic: the logins route with its store reads passed in; no store, no DAL.
// @domain: security
// @card: #4517
//! #4517 AC5 — POST /identity/logins: the door writes the Session row for a verified
//! login (Silas 00:22: new login 201, repeat 200 same row, foreign actsAs 403).
use athena_make::logins::{handle_login, Caller};
use std::cell::RefCell;

const NOW: &str = "2026-11-18T05:30:00Z";
fn kade() -> Caller<'static> { Caller { principal: "kade", jti: "j1", expires_at: "2026-11-18T05:40:00Z" } }
fn host(p: &str) -> Result<Option<String>, String> { Ok((p == "kade").then(|| "chorus-kade".to_string())) }
fn serves(r: &str) -> Result<bool, String> { Ok(r == "role-kade") }
const PANE: &str = r#"{"channel":"pane","hostAccount":"chorus-kade","actsAs":"role-kade"}"#;

#[test]
fn a_new_login_is_created_by_the_door_as_the_caller() {
    let wrote = RefCell::new(String::new());
    let (code, _) = handle_login(&kade(), PANE, host, serves, |_| Ok(None),
        |b| { *wrote.borrow_mut() = b.to_string(); (201, "{}".into()) }, NOW);
    assert_eq!(code, 201);
    let row: serde_json::Value = serde_json::from_str(&wrote.borrow()).unwrap();
    assert_eq!(row["ownedBy"], "principal-kade");
    assert_eq!(row["startedBy"], "principal-door");
    assert_eq!(row["tokenId"], "j1");
    assert!(row["name"].as_str().unwrap().starts_with("kade-"));
}

#[test]
fn a_repeat_of_the_same_login_answers_200_with_its_row_and_writes_nothing() {
    let (code, body) = handle_login(&kade(), PANE, host, serves, |j| Ok((j == "j1").then(|| "kade-abc".to_string())),
        |_| panic!("a repeat must not write"), NOW);
    assert_eq!(code, 200);
    assert!(body.contains("kade-abc") && body.contains("\"created\": false"), "{body}");
}

/// NEGATIVE PROOFS: a foreign actsAs and a wrong account are refused before any write.
#[test]
fn negative_proof_foreign_acts_as_and_wrong_account_are_refused_and_nothing_is_written() {
    let never = |_: &str| -> (u16, String) { panic!("a refused login must not write") };
    let (c, _) = handle_login(&kade(), r#"{"channel":"pane","hostAccount":"chorus-kade","actsAs":"role-silas"}"#, host, serves, |_| Ok(None), never, NOW);
    assert_eq!(c, 403);
    let (c, _) = handle_login(&kade(), r#"{"channel":"pane","hostAccount":"chorus-silas"}"#, host, serves, |_| Ok(None), never, NOW);
    assert_eq!(c, 403);
    let no_id = Caller { principal: "kade", jti: "", expires_at: "x" };
    let (c, b) = handle_login(&no_id, PANE, host, serves, |_| Ok(None), never, NOW);
    assert_eq!(c, 401, "{b}");
}

/// A store that did not answer is 503, never a refusal and never a write.
#[test]
fn an_unread_store_is_503_not_a_refusal() {
    let never = |_: &str| -> (u16, String) { panic!("no write on an unread store") };
    let (c, _) = handle_login(&kade(), PANE, |_| Err("timeout".into()), serves, |_| Ok(None), never, NOW);
    assert_eq!(c, 503);
    let (c, _) = handle_login(&kade(), PANE, host, serves, |_| Err("timeout".into()), never, NOW);
    assert_eq!(c, 503);
    // NEGATIVE PROOF (Silas 00:27): a Teammate read that failed is 503, not "does not serve"
    let (c, b) = handle_login(&kade(), PANE, host, |_| Err("timeout".into()), |_| Ok(None), never, NOW);
    assert_eq!(c, 503, "{b}");
}
