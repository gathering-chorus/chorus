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
    let (code, body) = handle_login(&kade(), PANE, host, serves, |_| Ok(None),
        |b| { *wrote.borrow_mut() = b.to_string(); (201, "{}".into()) }, |_| {}, NOW);
    assert_eq!(code, 201);
    assert!(body.contains("\"created\": true") && body.contains("\"name\": \"kade-"), "the answer names the row: {body}");
    let row: serde_json::Value = serde_json::from_str(&wrote.borrow()).unwrap();
    assert_eq!(row["ownedBy"], "principal-kade");
    assert_eq!(row["startedBy"], "principal-door");
    assert_eq!(row["tokenId"], "j1");
    assert!(row["name"].as_str().unwrap().starts_with("kade-"));
}

#[test]
fn a_repeat_of_the_same_login_answers_200_with_its_row_and_writes_nothing() {
    let (code, body) = handle_login(&kade(), PANE, host, serves, |j| Ok((j == "j1").then(|| "kade-abc".to_string())),
        |_| panic!("a repeat must not write"), |_| panic!("a repeat supersedes nothing"), NOW);
    assert_eq!(code, 200);
    assert!(body.contains("kade-abc") && body.contains("\"created\": false"), "{body}");
}

/// NEGATIVE PROOFS: a foreign actsAs and a wrong account are refused before any write.
#[test]
fn negative_proof_foreign_acts_as_and_wrong_account_are_refused_and_nothing_is_written() {
    let never = |_: &str| -> (u16, String) { panic!("a refused login must not write") };
    let (c, _) = handle_login(&kade(), r#"{"channel":"pane","hostAccount":"chorus-kade","actsAs":"role-silas"}"#, host, serves, |_| Ok(None), never, |_| panic!("a refused login supersedes nothing"), NOW);
    assert_eq!(c, 403);
    let (c, _) = handle_login(&kade(), r#"{"channel":"pane","hostAccount":"chorus-silas"}"#, host, serves, |_| Ok(None), never, |_| panic!("a refused login supersedes nothing"), NOW);
    assert_eq!(c, 403);
    let no_id = Caller { principal: "kade", jti: "", expires_at: "x" };
    let (c, b) = handle_login(&no_id, PANE, host, serves, |_| Ok(None), never, |_| panic!("a refused login supersedes nothing"), NOW);
    assert_eq!(c, 401, "{b}");
}

/// A store that did not answer is 503, never a refusal and never a write.
#[test]
fn an_unread_store_is_503_not_a_refusal() {
    let never = |_: &str| -> (u16, String) { panic!("no write on an unread store") };
    let (c, _) = handle_login(&kade(), PANE, |_| Err("timeout".into()), serves, |_| Ok(None), never, |_| panic!("a refused login supersedes nothing"), NOW);
    assert_eq!(c, 503);
    let (c, _) = handle_login(&kade(), PANE, host, serves, |_| Err("timeout".into()), never, |_| panic!("a refused login supersedes nothing"), NOW);
    assert_eq!(c, 503);
    // NEGATIVE PROOF (Silas 00:27): a Teammate read that failed is 503, not "does not serve"
    let (c, b) = handle_login(&kade(), PANE, host, |_| Err("timeout".into()), |_| Ok(None), never, |_| panic!("a refused login supersedes nothing"), NOW);
    assert_eq!(c, 503, "{b}");
}

mod logouts {
    use athena_make::logins::{handle_logout, SessionFacts};
    use std::cell::RefCell;
    const NOW: &str = "2026-11-18T06:00:00Z";
    fn kades(_: &str) -> Result<Option<SessionFacts>, String> { Ok(Some(SessionFacts { owner: "principal-kade".into(), ended: false })) }

    #[test]
    fn the_owner_ends_its_session_and_the_door_stamps_how() {
        let sent = RefCell::new(String::new());
        let (c, b) = handle_logout("kade", r#"{"session":"kade-1"}"#, kades, |_, p| { *sent.borrow_mut() = p.to_string(); (200, "{}".into()) }, NOW);
        assert_eq!(c, 200, "{b}");
        assert!(b.contains("\"ended\": true"));
        let p: serde_json::Value = serde_json::from_str(&sent.borrow()).unwrap();
        assert_eq!(p["endedAt"], NOW);
        assert_eq!(p["endedBy"], "door");
        assert_eq!(p["sessionState"], "closed");
    }

    /// NEGATIVE PROOFS (Silas 00:51): a role ending another role's session is 403; the
    /// caller cannot say how it ended; nothing is written for either.
    #[test]
    fn negative_proof_another_roles_session_and_caller_stamps_are_refused() {
        let never = |_: &str, _: &str| -> (u16, String) { panic!("a refused logout must not write") };
        assert_eq!(handle_logout("silas", r#"{"session":"kade-1"}"#, kades, never, NOW).0, 403);
        assert_eq!(handle_logout("kade", r#"{"session":"kade-1","endedBy":"silas"}"#, kades, never, NOW).0, 422);
        assert_eq!(handle_logout("kade", r#"{"session":"../x"}"#, kades, never, NOW).0, 422);
    }

    #[test]
    fn a_repeat_or_a_missing_row_writes_nothing_and_an_unread_store_is_503() {
        let never = |_: &str, _: &str| -> (u16, String) { panic!("must not write") };
        let ended = |_: &str| Ok(Some(SessionFacts { owner: "principal-kade".into(), ended: true }));
        assert_eq!(handle_logout("kade", r#"{"session":"kade-1"}"#, ended, never, NOW).0, 200);
        assert_eq!(handle_logout("kade", r#"{"session":"kade-1"}"#, |_| Ok(None), never, NOW).0, 404);
        assert_eq!(handle_logout("kade", r#"{"session":"kade-1"}"#, |_| Err("timeout".into()), never, NOW).0, 503);
    }
}

mod supersede {
    use athena_make::logins::{handle_login, Caller};
    use std::cell::RefCell;
    const NOW: &str = "2026-11-18T06:00:00Z";
    fn kade() -> Caller<'static> { Caller { principal: "kade", jti: "j2", expires_at: "x" } }
    fn host(_: &str) -> Result<Option<String>, String> { Ok(Some("chorus-kade".into())) }
    fn serves(_: &str) -> Result<bool, String> { Ok(true) }

    #[test]
    fn a_new_pane_login_supersedes_only_after_its_row_is_written() {
        let order = RefCell::new(Vec::<String>::new());
        let (c, _) = handle_login(&kade(), r#"{"channel":"pane","hostAccount":"chorus-kade"}"#, host, serves, |_| Ok(None),
            |_| { order.borrow_mut().push("create".into()); (201, "{}".into()) },
            |keep| order.borrow_mut().push(format!("supersede {keep}")), NOW);
        assert_eq!(c, 201);
        let o = order.borrow();
        assert_eq!(o.len(), 2);
        assert_eq!(o[0], "create");
        assert!(o[1].starts_with("supersede kade-"), "{o:?}");
    }

    /// NEGATIVE PROOFS (Silas 00:54): a failed create supersedes nothing (the role is never
    /// left with no open row), and a browser login never supersedes (phone and desktop).
    #[test]
    fn negative_proofs_a_failed_create_or_a_browser_login_supersedes_nothing() {
        let (c, _) = handle_login(&kade(), r#"{"channel":"pane","hostAccount":"chorus-kade"}"#, host, serves, |_| Ok(None),
            |_| (502, "{}".into()), |_| panic!("a failed create must not supersede"), NOW);
        assert_eq!(c, 502);
        let jeff = Caller { principal: "jeff", jti: "b1", expires_at: "x" };
        let (c, _) = handle_login(&jeff, r#"{"channel":"browser"}"#, |_| Ok(None), serves, |_| Ok(None),
            |_| (201, "{}".into()), |_| panic!("a browser login must not supersede"), NOW);
        assert_eq!(c, 201);
    }
}
