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

/// Every property SessionShape requires (sh:minCount 1), read from the shape files.
fn required_session_fields() -> Vec<String> {
    let root = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../..");
    let mut out = Vec::new();
    for f in ["roles/silas/ontology/session-4202.ttl", "roles/silas/ontology/session-model-4302.ttl"] {
        let text: String = std::fs::read_to_string(root.join(f)).unwrap_or_else(|e| panic!("{f}: {e}"))
            .lines().filter(|l| !l.trim_start().starts_with('#')).collect::<Vec<_>>().join("\n");
        let parts: Vec<&str> = text.split("a sh:PropertyShape").collect();
        for w in parts.windows(2) {
            if !w[0].trim_end().rsplit(char::is_whitespace).next().unwrap_or("").starts_with("chorus:SessionShape-") { continue; }
            let stmt = w[1].split(" .\n").next().unwrap_or("");
            if !stmt.contains("sh:minCount 1") { continue; }
            if let Some(p) = stmt.split("sh:path chorus:").nth(1) {
                out.push(p.chars().take_while(|c| c.is_alphanumeric()).collect());
            }
        }
    }
    out
}

/// The gate's 07:52 finding: plan_login never set issuedAt, which SessionShape requires.
/// Every required field is in the planned row on every channel, except ownedBy (the door's
/// create stamps the verified caller) and name (the door sets it from the plan's name).
/// NEGATIVE PROOF: the shape list is non-empty and holds issuedAt, so dropping the door's
/// issuedAt line turns this red (run by hand 11-18).
#[test]
fn every_field_the_session_shape_requires_is_in_the_planned_row_on_every_channel() {
    let req = required_session_fields();
    assert!(req.iter().any(|f| f == "issuedAt") && req.iter().any(|f| f == "tokenId"), "shape parse found {req:?}");
    let bodies = [
        json!({"channel":"pane","hostAccount":"chorus-kade","actsAs":"role-kade"}),
        json!({"channel":"agent","hostAccount":"chorus-kade","actsAs":"role-kade"}),
        json!({"channel":"browser"}),
    ];
    for body in bodies {
        let Outcome::Create(_, row) = plan_login(&kade(), &facts(), &body, serves_kade, None, NOW).unwrap()
        else { panic!("expected a row for {body}") };
        for f in req.iter().filter(|f| *f != "ownedBy" && *f != "name") {
            assert!(row.get(f.as_str()).is_some(), "{} row lacks required {f}", body["channel"]);
        }
    }
}

/// #4424 — a supervised runtime logs in on the agent channel (chorus-principal's
/// session_channel allows it): like a pane, its account is checked.
/// NEGATIVE PROOF: an agent login naming another account is 403; an unknown channel is 422.
#[test]
fn an_agent_channel_login_opens_a_row_and_its_account_is_checked() {
    let ok = plan_login(&kade(), &facts(), &json!({"channel":"agent","hostAccount":"chorus-kade"}), serves_kade, None, NOW).unwrap();
    let Outcome::Create(_, row) = ok else { panic!("expected a row") };
    assert_eq!(row["channel"], "agent");
    assert_eq!(row["hostAccount"], "chorus-kade");
    assert_eq!(plan_login(&kade(), &facts(), &json!({"channel":"agent","hostAccount":"chorus-silas"}), serves_kade, None, NOW).unwrap_err().0, 403);
    assert_eq!(plan_login(&kade(), &facts(), &json!({"channel":"tty"}), serves_kade, None, NOW).unwrap_err().0, 422);
}
