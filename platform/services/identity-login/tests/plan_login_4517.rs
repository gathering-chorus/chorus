// @test-type: unit — the crate's public API only; no store, no door, no DAL.
// @domain: security
// @card: #4517
//! #4517 AC5 — the session decision through the crate's public API, as the door calls it:
//! one row per verified login, the door's stamps, the body never names owner or writer.
//! (werk-test registers a crate's tests by its tests/ files; the inline unit tests stay.)

use identity_login::{plan_login, session_name, Outcome, PrincipalFacts, VerifiedLogin};
use serde_json::json;

const NOW: &str = "2026-11-18T05:20:00Z";
fn kade() -> VerifiedLogin<'static> { VerifiedLogin { principal: "kade", token_id: "t1", expires_at: "2026-11-18T05:30:00Z" } }
fn facts() -> PrincipalFacts<'static> { PrincipalFacts { host_account: Some("chorus-kade"), persons: &["jeff"], present: &["jeff"] } }
fn serves_kade(r: &str) -> bool { r == "role-kade" }

#[test]
fn a_verified_pane_login_plans_one_door_stamped_row_and_a_repeat_writes_nothing() {
    let body = json!({"channel":"pane","hostAccount":"chorus-kade","actsAs":"role-kade"});
    let Outcome::Create(name, row) = plan_login(&kade(), &facts(), &body, serves_kade, None, NOW).unwrap()
    else { panic!("expected a row") };
    assert_eq!(name, session_name("kade", "t1"));
    assert!(row.get("startedBy").is_none(), "nobody named present: no startedBy");
    assert_eq!(row["actsAs"], "kade");
    assert!(row.get("ownedBy").is_none());
    let again = plan_login(&kade(), &facts(), &body, serves_kade, Some(&name), NOW).unwrap();
    assert_eq!(again, Outcome::Existing(name));
}

/// NEGATIVE PROOFS: no verified login is 401; a body naming the owner is 422; a role the
/// login does not serve, or another principal's account, is 403.
#[test]
fn negative_proofs_unverified_owner_in_body_foreign_role_and_account_are_refused() {
    let none = VerifiedLogin { principal: "", token_id: "", expires_at: "" };
    assert_eq!(plan_login(&none, &facts(), &json!({"channel":"pane","hostAccount":"chorus-kade"}), serves_kade, None, NOW).unwrap_err().0, 401);
    assert_eq!(plan_login(&kade(), &facts(), &json!({"channel":"pane","hostAccount":"chorus-kade","ownedBy":"silas"}), serves_kade, None, NOW).unwrap_err().0, 422);
    assert_eq!(plan_login(&kade(), &facts(), &json!({"channel":"pane","hostAccount":"chorus-kade","actsAs":"role-silas"}), serves_kade, None, NOW).unwrap_err().0, 403);
    assert_eq!(plan_login(&kade(), &facts(), &json!({"channel":"pane","hostAccount":"chorus-silas"}), serves_kade, None, NOW).unwrap_err().0, 403);
}

/// #4412 (Jeff 11-18 "yes", Silas 06:22) — the person present at a login. A Clearing sign-in
/// the door can see is startedBy; a person at his own terminal is only startReportedBy.
/// NEGATIVE PROOFS: a browser claim with no open browser Session is 403; a name that is not a
/// person is 403; a person with no personAt, or a bad one, is 422; terminal never writes startedBy.
#[test]
fn the_person_present_is_stamped_only_as_the_door_can_vouch() {
    let body = |at: &str, who: &str| json!({"channel":"pane","hostAccount":"chorus-kade","person":who,"personAt":at});
    let none_present = PrincipalFacts { host_account: Some("chorus-kade"), persons: &["jeff"], present: &[] };
    let row = |r: Outcome| match r { Outcome::Create(_, v) => v, _ => panic!("expected a row") };
    let b = row(plan_login(&kade(), &facts(), &body("browser", "jeff"), serves_kade, None, NOW).unwrap());
    assert_eq!(b["startedBy"], "jeff");
    assert!(b.get("startReportedBy").is_none());
    let t = row(plan_login(&kade(), &none_present, &body("terminal", "jeff"), serves_kade, None, NOW).unwrap());
    assert_eq!(t["startReportedBy"], "jeff");
    assert!(t.get("startedBy").is_none(), "a terminal report is never startedBy");
    assert_eq!(plan_login(&kade(), &none_present, &body("browser", "jeff"), serves_kade, None, NOW).unwrap_err().0, 403);
    assert_eq!(plan_login(&kade(), &facts(), &body("browser", "silas"), serves_kade, None, NOW).unwrap_err().0, 403);
    assert_eq!(plan_login(&kade(), &facts(), &json!({"channel":"pane","hostAccount":"chorus-kade","person":"jeff"}), serves_kade, None, NOW).unwrap_err().0, 422);
    assert_eq!(plan_login(&kade(), &facts(), &body("phone", "jeff"), serves_kade, None, NOW).unwrap_err().0, 422);
}
