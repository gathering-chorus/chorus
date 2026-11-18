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
fn no_people() -> Result<athena_make::logins::People, String> { Ok(athena_make::logins::People::default()) }
const PANE: &str = r#"{"channel":"pane","hostAccount":"chorus-kade","actsAs":"role-kade"}"#;

#[test]
fn a_new_login_is_created_by_the_door_as_the_caller() {
    let wrote = RefCell::new(String::new());
    let (code, body) = handle_login(&kade(), PANE, host, serves, |_| Ok(None),
        |b| { *wrote.borrow_mut() = b.to_string(); (201, "{}".into()) }, |_| {}, |_| {}, no_people, NOW);
    assert_eq!(code, 201);
    assert!(body.contains("\"created\": true") && body.contains("\"name\": \"kade-"), "the answer names the row: {body}");
    let row: serde_json::Value = serde_json::from_str(&wrote.borrow()).unwrap();
    assert!(row.get("ownedBy").is_none(), "the door's create stamps the owner from the login");
    assert_eq!(row["actsAs"], "kade");
    assert!(row.get("startedBy").is_none(), "no person named: startedBy says who was present, never the writer");
    assert_eq!(row["tokenId"], "j1");
    assert!(row["name"].as_str().unwrap().starts_with("kade-"));
}

#[test]
fn a_repeat_of_the_same_login_answers_200_with_its_row_and_writes_nothing() {
    let (code, body) = handle_login(&kade(), PANE, host, serves, |j| Ok((j == "j1").then(|| "kade-abc".to_string())),
        |_| panic!("a repeat must not write"), |_| panic!("a repeat supersedes nothing"), |_| {}, no_people, NOW);
    assert_eq!(code, 200);
    assert!(body.contains("kade-abc") && body.contains("\"created\": false"), "{body}");
}

/// NEGATIVE PROOFS: a foreign actsAs and a wrong account are refused before any write.
#[test]
fn negative_proof_foreign_acts_as_and_wrong_account_are_refused_and_nothing_is_written() {
    let never = |_: &str| -> (u16, String) { panic!("a refused login must not write") };
    let (c, _) = handle_login(&kade(), r#"{"channel":"pane","hostAccount":"chorus-kade","actsAs":"role-silas"}"#, host, serves, |_| Ok(None), never, |_| panic!("a refused login supersedes nothing"), |_| {}, no_people, NOW);
    assert_eq!(c, 403);
    let (c, _) = handle_login(&kade(), r#"{"channel":"pane","hostAccount":"chorus-silas"}"#, host, serves, |_| Ok(None), never, |_| panic!("a refused login supersedes nothing"), |_| {}, no_people, NOW);
    assert_eq!(c, 403);
    let no_id = Caller { principal: "kade", jti: "", expires_at: "x" };
    let (c, b) = handle_login(&no_id, PANE, host, serves, |_| Ok(None), never, |_| panic!("a refused login supersedes nothing"), |_| {}, no_people, NOW);
    assert_eq!(c, 401, "{b}");
}

/// A store that did not answer is 503, never a refusal and never a write.
#[test]
fn an_unread_store_is_503_not_a_refusal() {
    let never = |_: &str| -> (u16, String) { panic!("no write on an unread store") };
    let (c, _) = handle_login(&kade(), PANE, |_| Err("timeout".into()), serves, |_| Ok(None), never, |_| panic!("a refused login supersedes nothing"), |_| {}, no_people, NOW);
    assert_eq!(c, 503);
    let (c, _) = handle_login(&kade(), PANE, host, serves, |_| Err("timeout".into()), never, |_| panic!("a refused login supersedes nothing"), |_| {}, no_people, NOW);
    assert_eq!(c, 503);
    // NEGATIVE PROOF (Silas 00:27): a Teammate read that failed is 503, not "does not serve"
    let (c, b) = handle_login(&kade(), PANE, host, |_| Err("timeout".into()), |_| Ok(None), never, |_| panic!("a refused login supersedes nothing"), |_| {}, no_people, NOW);
    assert_eq!(c, 503, "{b}");
}

mod logouts {
    use super::no_people;
    use athena_make::logins::{handle_logout, SessionFacts};
    use std::cell::RefCell;
    const NOW: &str = "2026-11-18T06:00:00Z";
    fn kades(_: &str) -> Result<Option<SessionFacts>, String> { Ok(Some(SessionFacts { owner: "principal-kade".into(), ended: false, expires_at: String::new() })) }

    #[test]
    fn the_owner_ends_its_session_and_the_door_stamps_how() {
        let sent = RefCell::new(String::new());
        let (c, b) = handle_logout("kade", r#"{"session":"kade-1"}"#, kades, |_, p| { *sent.borrow_mut() = p.to_string(); (200, "{}".into()) }, no_people, NOW);
        assert_eq!(c, 200, "{b}");
        assert!(b.contains("\"ended\": true"));
        let p: serde_json::Value = serde_json::from_str(&sent.borrow()).unwrap();
        assert_eq!(p["endedAt"], NOW);
        assert!(p.get("endedBy").is_none(), "no person named: endedBy says who was present, never the writer");
        assert_eq!(p["sessionState"], "closed");
    }

    /// NEGATIVE PROOFS (Silas 00:51): a role ending another role's session is 403; the
    /// caller cannot say how it ended; nothing is written for either.
    #[test]
    fn negative_proof_another_roles_session_and_caller_stamps_are_refused() {
        let never = |_: &str, _: &str| -> (u16, String) { panic!("a refused logout must not write") };
        assert_eq!(handle_logout("silas", r#"{"session":"kade-1"}"#, kades, never, no_people, NOW).0, 403);
        assert_eq!(handle_logout("kade", r#"{"session":"kade-1","endedBy":"silas"}"#, kades, never, no_people, NOW).0, 422);
        assert_eq!(handle_logout("kade", r#"{"session":"../x"}"#, kades, never, no_people, NOW).0, 422);
    }

    /// #4966 rule (Silas 01:39): an expired row ends AT its expiresAt; an unexpired one now.
    #[test]
    fn an_expired_row_ends_at_its_expiry_and_a_live_one_ends_now() {
        let sent = RefCell::new(String::new());
        let expired = |_: &str| Ok(Some(SessionFacts { owner: "principal-kade".into(), ended: false, expires_at: "2026-11-18T05:00:00Z".into() }));
        handle_logout("kade", r#"{"session":"kade-1"}"#, expired, |_, p| { *sent.borrow_mut() = p.into(); (200, "{}".into()) }, no_people, NOW);
        assert!(sent.borrow().contains("\"endedAt\":\"2026-11-18T05:00:00Z\""), "{}", sent.borrow());
        let live = |_: &str| Ok(Some(SessionFacts { owner: "principal-kade".into(), ended: false, expires_at: "2026-11-18T09:00:00Z".into() }));
        handle_logout("kade", r#"{"session":"kade-1"}"#, live, |_, p| { *sent.borrow_mut() = p.into(); (200, "{}".into()) }, no_people, NOW);
        assert!(sent.borrow().contains(&format!("\"endedAt\":\"{NOW}\"")), "never past now: {}", sent.borrow());
    }

    #[test]
    fn a_repeat_or_a_missing_row_writes_nothing_and_an_unread_store_is_503() {
        let never = |_: &str, _: &str| -> (u16, String) { panic!("must not write") };
        let ended = |_: &str| Ok(Some(SessionFacts { owner: "principal-kade".into(), ended: true, expires_at: String::new() }));
        assert_eq!(handle_logout("kade", r#"{"session":"kade-1"}"#, ended, never, no_people, NOW).0, 200);
        assert_eq!(handle_logout("kade", r#"{"session":"kade-1"}"#, |_| Ok(None), never, no_people, NOW).0, 404);
        assert_eq!(handle_logout("kade", r#"{"session":"kade-1"}"#, |_| Err("timeout".into()), never, no_people, NOW).0, 503);
    }
}

mod supersede {
    use super::no_people;
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
            |keep| order.borrow_mut().push(format!("supersede {keep}")), |_| {}, no_people, NOW);
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
            |_| (502, "{}".into()), |_| panic!("a failed create must not supersede"), |_| {}, no_people, NOW);
        assert_eq!(c, 502);
        let jeff = Caller { principal: "jeff", jti: "b1", expires_at: "x" };
        let (c, _) = handle_login(&jeff, r#"{"channel":"browser"}"#, |_| Ok(Some("jeffbridwell".into())), serves, |_| Ok(None),
            |_| (201, "{}".into()), |_| panic!("a browser login must not supersede"), |_| {}, no_people, NOW);
        assert_eq!(c, 201);
    }
}

mod browser_touch {
    use super::no_people;
    use athena_make::logins::{handle_login, touch_due, Caller};
    use std::cell::{Cell, RefCell};
    const NOW: &str = "2026-11-18T06:30:00Z";

    /// A repeat browser login touches its row; a repeat pane login does not.
    #[test]
    fn a_repeat_browser_login_touches_and_a_pane_does_not() {
        let jeff = Caller { principal: "jeff", jti: "b1", expires_at: "x" };
        let touched = RefCell::new(Vec::<String>::new());
        let (c, _) = handle_login(&jeff, r#"{"channel":"browser"}"#, |_| Ok(Some("jeffbridwell".into())), |_| Ok(false), |_| Ok(Some("jeff-b".into())),
            |_| panic!("a repeat writes no new row"), |_| panic!("a repeat supersedes nothing"), |n| touched.borrow_mut().push(n.into()), no_people, NOW);
        assert_eq!(c, 200);
        assert_eq!(touched.borrow().as_slice(), ["jeff-b"]);
        let kade = Caller { principal: "kade", jti: "p1", expires_at: "x" };
        let (c, _) = handle_login(&kade, r#"{"channel":"pane","hostAccount":"chorus-kade"}"#, |_| Ok(Some("chorus-kade".into())), |_| Ok(true), |_| Ok(Some("kade-p".into())),
            |_| panic!("no new row"), |_| panic!("no supersede"), |_| panic!("a pane is never touched"), no_people, NOW);
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
            handle_login(&jeff, r#"{"channel":"browser"}"#, |_| Ok(Some("jeffbridwell".into())), |_| Ok(false), |_| Ok(Some("jeff-b".into())),
                |_| (201, String::new()), |_| {}, touch, no_people, NOW);
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

/// #4517 (Silas 01:45) — the DAL mints "principal-"/"role-" on a link and refuses a name that
/// already carries it (normalize_name, double-prefix). Every link the door writes on a Session
/// row names its target bare. NEGATIVE PROOF: the prefixed spelling that shipped in 6db4b76e6's
/// parent would trip this.
#[test]
fn every_link_on_a_door_written_row_names_its_target_bare() {
    use athena_make::logins::{handle_login, Caller};
    use std::cell::RefCell;
    let wrote = RefCell::new(String::new());
    let kade = Caller { principal: "kade", jti: "j9", expires_at: "x" };
    handle_login(&kade, r#"{"channel":"pane","hostAccount":"chorus-kade","actsAs":"role-kade"}"#,
        |_| Ok(Some("chorus-kade".into())), |r| Ok(r == "role-kade"), |_| Ok(None),
        |b| { *wrote.borrow_mut() = b.to_string(); (201, "{}".into()) }, |_| {}, |_| {}, no_people, "2026-11-18T06:50:00Z");
    let row: serde_json::Value = serde_json::from_str(&wrote.borrow()).unwrap();
    for (link, prefix) in [("startedBy", "principal-"), ("actsAs", "role-"), ("ownedBy", "principal-"), ("endedBy", "principal-")] {
        if let Some(v) = row.get(link).and_then(|v| v.as_str()) {
            assert!(!v.starts_with(prefix), "{link} = {v}: the DAL refuses a double prefix");
        }
    }
    assert!(row.get("startedBy").is_none());
    assert_eq!(row["actsAs"], "kade");
}

/// #4517 AC5 — a refused login is recorded by the door, with its reason. NEGATIVE PROOF:
/// an accepted login (201) or a repeat (200) records no refusal.
#[test]
fn the_door_records_a_refused_login_with_its_reason() {
    use athena_make::logins::refusal_event;
    let e = refusal_event(403, "kade", r#"{ "error": "authz", "message": "kade does not serve 'role-silas'" }"#).unwrap();
    assert_eq!(e[0], "session.login.refused");
    assert_eq!(e[1], "kade");
    assert!(e.iter().any(|a| a == "reason=kade does not serve 'role-silas'"), "{e:?}");
    assert!(e.iter().any(|a| a == "by=door"));
    let unverified = refusal_event(401, "", r#"{ "error": "authn-missing", "message": "a verified login is required to open a session" }"#).unwrap();
    assert_eq!(unverified[1], "unknown");
    assert!(refusal_event(201, "kade", "{}").is_none() && refusal_event(200, "kade", "{}").is_none());
}

/// #4517 (ex-#4445, Jeff 10-08 "i now see 2 sessions for abby") — a new pane login supersedes
/// only the same principal's open pane rows on the same account. NEGATIVE PROOFS: another
/// principal's row, a closed row and another channel are outside the query, and a name or
/// host that could break out of it asks nothing.
#[test]
fn the_supersede_query_names_only_this_principals_open_pane_rows_on_this_account() {
    use athena_make::logins::older_open_panes_query;
    let q = older_open_panes_query("silas", "chorus-silas").unwrap();
    assert!(q.contains("chorus:ownedBy <https://jeffbridwell.com/chorus#principal-silas>"), "{q}");
    assert!(q.contains(r#"STR(?c) = "pane""#) && q.contains(r#"STR(?h) = "chorus-silas""#));
    assert!(q.contains("FILTER NOT EXISTS { ?s chorus:endedAt ?e }"), "a closed row is never superseded");
    assert!(!q.contains("principal-kade"));
    assert!(older_open_panes_query("silas> . ?x ?y ?z", "chorus-silas").is_none());
    assert!(older_open_panes_query("silas", "a\" } #").is_none());
    assert!(older_open_panes_query("", "chorus-silas").is_none());
}

/// #4412 (Jeff 11-18 "yes", Silas 06:22) — the person present at a logout, as the door can
/// vouch for him: endedBy only with his open browser Session, endReportedBy for a terminal
/// report. NEGATIVE PROOFS: a browser claim with nobody signed in is 403 and ends nothing; a
/// store that cannot say who is signed in is 503, never a guess.
mod logout_person {
    use athena_make::logins::{handle_logout, People, SessionFacts};
    use std::cell::RefCell;
    const NOW: &str = "2026-11-18T06:40:00Z";
    fn kades(_: &str) -> Result<Option<SessionFacts>, String> { Ok(Some(SessionFacts { owner: "https://jeffbridwell.com/chorus#principal-kade".into(), ended: false, expires_at: "2026-11-18T09:00:00Z".into() })) }
    fn jeff_here() -> Result<People, String> { Ok(People { persons: vec!["jeff".into()], present: vec!["jeff".into()] }) }
    fn nobody_here() -> Result<People, String> { Ok(People { persons: vec!["jeff".into()], present: vec![] }) }
    fn end(body: &str, people: fn() -> Result<People, String>) -> (u16, serde_json::Value) {
        let sent = RefCell::new(String::new());
        let (c, _) = handle_logout("kade", body, kades, |_, p| { *sent.borrow_mut() = p.to_string(); (200, "{}".into()) }, people, NOW);
        let v = serde_json::from_str(&sent.borrow()).unwrap_or(serde_json::Value::Null);
        (c, v)
    }
    #[test]
    fn the_person_present_at_a_logout_is_stamped_only_as_the_door_can_vouch() {
        let (c, p) = end(r#"{"session":"kade-1","person":"jeff","personAt":"browser"}"#, jeff_here);
        assert_eq!((c, &p["endedBy"]), (200, &serde_json::json!("jeff")));
        let (c, p) = end(r#"{"session":"kade-1","person":"jeff","personAt":"terminal"}"#, nobody_here);
        assert_eq!(c, 200);
        assert_eq!(p["endReportedBy"], "jeff");
        assert!(p.get("endedBy").is_none(), "a terminal report is never endedBy");
        let (c, p) = end(r#"{"session":"kade-1","person":"jeff","personAt":"browser"}"#, nobody_here);
        assert_eq!(c, 403);
        assert!(p.is_null(), "a refused logout ends nothing");
        let down = || -> Result<People, String> { Err("timeout".into()) };
        let sent = RefCell::new(false);
        let (c, _) = handle_logout("kade", r#"{"session":"kade-1","person":"jeff","personAt":"browser"}"#, kades, |_, _| { *sent.borrow_mut() = true; (200, "{}".into()) }, down, NOW);
        assert_eq!(c, 503);
        assert!(!*sent.borrow());
    }
}

/// #4412 — who is signed in: an open, unexpired browser Session owned by a person.
/// NEGATIVE PROOFS: the query excludes ended rows, expired rows, pane rows and non-persons.
#[test]
fn the_signed_in_query_counts_only_open_unexpired_browser_sessions_of_persons() {
    let q = athena_make::logins::people_present_query("2026-11-18T06:40:00Z");
    assert!(q.contains(r#"STR(?c) = "browser""#), "{q}");
    assert!(q.contains(r#"STR(?x) > "2026-11-18T06:40:00Z""#), "an expired sign-in is not presence");
    assert!(q.contains("FILTER NOT EXISTS { ?s chorus:endedAt ?e }"));
    assert!(q.contains(r#"STR(?k) = "person""#));
    assert!(!athena_make::logins::people_present_query(r#"x" } DROP"#).contains(r#"x" }"#), "a quote cannot break out");
}

/// #4367 (Jeff 11-18 "yes") — a role reports Jeff spoke in its session; the door writes
/// attendedBy + lastAttendedAt on the caller's own open row. NEGATIVE PROOFS: another
/// principal's row is 403, an ended row 409, a non-person 403, an extra field 422, and the
/// row is not written in any of them.
mod attendance {
    use athena_make::logins::{handle_attendance, People, SessionFacts};
    use std::cell::RefCell;
    const NOW: &str = "2026-11-18T06:50:00Z";
    fn row(owner: &str, ended: bool) -> Result<Option<SessionFacts>, String> { Ok(Some(SessionFacts { owner: format!("https://jeffbridwell.com/chorus#principal-{owner}"), ended, expires_at: "2026-11-18T09:00:00Z".into() })) }
    fn people() -> Result<People, String> { Ok(People { persons: vec!["jeff".into()], present: vec![] }) }
    #[test]
    fn a_role_records_that_jeff_spoke_in_its_own_open_session_and_nothing_else() {
        let sent = RefCell::new(String::new());
        let (c, _) = handle_attendance("kade", r#"{"session":"kade-1","person":"jeff"}"#, |_| row("kade", false), |_, p| { *sent.borrow_mut() = p.into(); (200, "{}".into()) }, people, NOW);
        assert_eq!(c, 200);
        let p: serde_json::Value = serde_json::from_str(&sent.borrow()).unwrap();
        assert_eq!((p["attendedBy"].as_str(), p["lastAttendedAt"].as_str()), (Some("jeff"), Some(NOW)));
        let never = |_: &str, _: &str| -> (u16, String) { panic!("a refused attendance writes nothing") };
        assert_eq!(handle_attendance("silas", r#"{"session":"kade-1","person":"jeff"}"#, |_| row("kade", false), never, people, NOW).0, 403);
        assert_eq!(handle_attendance("kade", r#"{"session":"kade-1","person":"jeff"}"#, |_| row("kade", true), never, people, NOW).0, 409);
        assert_eq!(handle_attendance("kade", r#"{"session":"kade-1","person":"silas"}"#, |_| row("kade", false), never, people, NOW).0, 403);
        assert_eq!(handle_attendance("kade", r#"{"session":"kade-1","person":"jeff","lastAttendedAt":"x"}"#, |_| row("kade", false), never, people, NOW).0, 422);
    }
}
