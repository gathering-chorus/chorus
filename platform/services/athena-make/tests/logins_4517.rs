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
        |b| { *wrote.borrow_mut() = b.to_string(); (201, "{}".into()) }, |_| {}, |_| {}, NOW);
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
        |_| panic!("a repeat must not write"), |_| panic!("a repeat supersedes nothing"), |_| {}, NOW);
    assert_eq!(code, 200);
    assert!(body.contains("kade-abc") && body.contains("\"created\": false"), "{body}");
}

/// NEGATIVE PROOFS: a foreign actsAs and a wrong account are refused before any write.
#[test]
fn negative_proof_foreign_acts_as_and_wrong_account_are_refused_and_nothing_is_written() {
    let never = |_: &str| -> (u16, String) { panic!("a refused login must not write") };
    let (c, _) = handle_login(&kade(), r#"{"channel":"pane","hostAccount":"chorus-kade","actsAs":"role-silas"}"#, host, serves, |_| Ok(None), never, |_| panic!("a refused login supersedes nothing"), |_| {}, NOW);
    assert_eq!(c, 403);
    let (c, _) = handle_login(&kade(), r#"{"channel":"pane","hostAccount":"chorus-silas"}"#, host, serves, |_| Ok(None), never, |_| panic!("a refused login supersedes nothing"), |_| {}, NOW);
    assert_eq!(c, 403);
    let no_id = Caller { principal: "kade", jti: "", expires_at: "x" };
    let (c, b) = handle_login(&no_id, PANE, host, serves, |_| Ok(None), never, |_| panic!("a refused login supersedes nothing"), |_| {}, NOW);
    assert_eq!(c, 401, "{b}");
}

/// A store that did not answer is 503, never a refusal and never a write.
#[test]
fn an_unread_store_is_503_not_a_refusal() {
    let never = |_: &str| -> (u16, String) { panic!("no write on an unread store") };
    let (c, _) = handle_login(&kade(), PANE, |_| Err("timeout".into()), serves, |_| Ok(None), never, |_| panic!("a refused login supersedes nothing"), |_| {}, NOW);
    assert_eq!(c, 503);
    let (c, _) = handle_login(&kade(), PANE, host, serves, |_| Err("timeout".into()), never, |_| panic!("a refused login supersedes nothing"), |_| {}, NOW);
    assert_eq!(c, 503);
    // NEGATIVE PROOF (Silas 00:27): a Teammate read that failed is 503, not "does not serve"
    let (c, b) = handle_login(&kade(), PANE, host, |_| Err("timeout".into()), |_| Ok(None), never, |_| panic!("a refused login supersedes nothing"), |_| {}, NOW);
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
            |keep| order.borrow_mut().push(format!("supersede {keep}")), |_| {}, NOW);
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
            |_| (502, "{}".into()), |_| panic!("a failed create must not supersede"), |_| {}, NOW);
        assert_eq!(c, 502);
        let jeff = Caller { principal: "jeff", jti: "b1", expires_at: "x" };
        let (c, _) = handle_login(&jeff, r#"{"channel":"browser"}"#, |_| Ok(None), serves, |_| Ok(None),
            |_| (201, "{}".into()), |_| panic!("a browser login must not supersede"), |_| {}, NOW);
        assert_eq!(c, 201);
    }
}

mod browser_touch {
    use athena_make::logins::{handle_login, touch_due, Caller};
    use std::cell::{Cell, RefCell};
    const NOW: &str = "2026-11-18T06:30:00Z";

    /// A repeat browser login touches its row; a repeat pane login does not.
    #[test]
    fn a_repeat_browser_login_touches_and_a_pane_does_not() {
        let jeff = Caller { principal: "jeff", jti: "b1", expires_at: "x" };
        let touched = RefCell::new(Vec::<String>::new());
        let (c, _) = handle_login(&jeff, r#"{"channel":"browser"}"#, |_| Ok(None), |_| Ok(false), |_| Ok(Some("jeff-b".into())),
            |_| panic!("a repeat writes no new row"), |_| panic!("a repeat supersedes nothing"), |n| touched.borrow_mut().push(n.into()), NOW);
        assert_eq!(c, 200);
        assert_eq!(touched.borrow().as_slice(), ["jeff-b"]);
        let kade = Caller { principal: "kade", jti: "p1", expires_at: "x" };
        let (c, _) = handle_login(&kade, r#"{"channel":"pane","hostAccount":"chorus-kade"}"#, |_| Ok(Some("chorus-kade".into())), |_| Ok(true), |_| Ok(Some("kade-p".into())),
            |_| panic!("no new row"), |_| panic!("no supersede"), |_| panic!("a pane is never touched"), NOW);
        assert_eq!(c, 200);
    }

    /// NEGATIVE PROOF (Silas 01:31): two touches inside the window make one write. The serve
    /// block's touch writes only when touch_due on the row's own lastSeenAt.
    #[test]
    fn negative_proof_two_touches_inside_the_window_write_once() {
        let jeff = Caller { principal: "jeff", jti: "b1", expires_at: "x" };
        let last = Cell::new(Some(1_000u64));
        let writes = Cell::new(0);
        let now = Cell::new(1_000 + 400);
        let touch = |_: &str| if touch_due(last.get(), now.get()) { writes.set(writes.get() + 1); last.set(Some(now.get())); };
        for step in [0u64, 60] {
            now.set(1_400 + step);
            handle_login(&jeff, r#"{"channel":"browser"}"#, |_| Ok(None), |_| Ok(false), |_| Ok(Some("jeff-b".into())),
                |_| (201, String::new()), |_| {}, touch, NOW);
        }
        assert_eq!(writes.get(), 1, "two touches 60 s apart wrote {} times", writes.get());
    }
}

mod renewal {
    use athena_make::logins::{handle_renewal, Caller};
    use std::cell::RefCell;
    const NOW: &str = "2026-11-18T06:40:00Z";
    fn jeff() -> Caller<'static> { Caller { principal: "jeff", jti: "b2", expires_at: "2026-11-18T07:40:00Z" } }
    fn his(_: &str) -> Result<Option<(String, bool, String)>, String> { Ok(Some(("principal-jeff".into(), true, "2026-11-18T07:00:00Z".into()))) }
    const RENEW: &str = r#"{"channel":"browser","renews":"jeff-b"}"#;

    #[test]
    fn his_refreshed_login_renews_his_row_and_writes_no_new_one() {
        let sent = RefCell::new(String::new());
        let (c, b) = handle_renewal(&jeff(), RENEW, his, |_, p| { *sent.borrow_mut() = p.into(); (200, "{}".into()) }, NOW);
        assert_eq!(c, 200, "{b}");
        let p: serde_json::Value = serde_json::from_str(&sent.borrow()).unwrap();
        assert_eq!(p["tokenId"], "b2");
        assert_eq!(p["expiresAt"], "2026-11-18T07:40:00Z");
    }

    /// NEGATIVE PROOFS: another's row and an expired row are refused, and nothing is written.
    #[test]
    fn negative_proofs_anothers_or_an_expired_row_is_refused_with_no_write() {
        let never = |_: &str, _: &str| -> (u16, String) { panic!("a refused renewal must not write") };
        let kades = |_: &str| Ok(Some(("principal-kade".into(), true, "2026-11-18T07:00:00Z".into())));
        assert_eq!(handle_renewal(&jeff(), RENEW, kades, never, NOW).0, 403);
        let expired = |_: &str| Ok(Some(("principal-jeff".into(), true, "2026-11-18T06:00:00Z".into())));
        assert_eq!(handle_renewal(&jeff(), RENEW, expired, never, NOW).0, 409);
        assert_eq!(handle_renewal(&jeff(), RENEW, |_| Err("timeout".into()), never, NOW).0, 503);
    }
}
