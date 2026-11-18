//! #4517 AC5 — the session decision through the crate's public API, as the door calls it:
//! one row per verified login, the door's stamps, the body never names owner or writer.
//! (werk-test registers a crate's tests by its tests/ files; the inline unit tests stay.)

use identity_login::{plan_login, session_name, Outcome, PrincipalFacts, VerifiedLogin, DOOR_PRINCIPAL};
use serde_json::json;

const NOW: &str = "2026-11-18T05:20:00Z";
fn kade() -> VerifiedLogin<'static> { VerifiedLogin { principal: "kade", token_id: "t1", expires_at: "2026-11-18T05:30:00Z" } }
fn facts() -> PrincipalFacts<'static> { PrincipalFacts { host_account: Some("chorus-kade") } }
fn serves_kade(r: &str) -> bool { r == "role-kade" }

#[test]
fn a_verified_pane_login_plans_one_door_stamped_row_and_a_repeat_writes_nothing() {
    let body = json!({"channel":"pane","hostAccount":"chorus-kade","actsAs":"role-kade"});
    let Outcome::Create(name, row) = plan_login(&kade(), &facts(), &body, serves_kade, None, NOW).unwrap()
    else { panic!("expected a row") };
    assert_eq!(name, session_name("kade", "t1"));
    assert_eq!(row["startedBy"], DOOR_PRINCIPAL);
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
