// @test-type: unit — the door's role-activation decision, store read passed in
// @domain: identity
// #4933 AC2 — the door opens a session (actsAs role-R) only when the caller's
// Teammate signs in as the caller's Principal and serves as R. One place, at the
// door: the decision is pure (acts_as_refusal), the Teammate read is passed in.
//
// NEGATIVE PROOF (#3734): kade asking for role-wren is the state the rule exists
// to refuse. Without the check, that case returns None (allowed) and goes red.

use athena_make::{acts_as_refusal, WriteOp};

const SESSION: &str = "https://jeffbridwell.com/chorus#Session";

/// The Teammate read the door would make: does a Teammate signsInAs principal-<caller>
/// and servesAs <role>? Kade serves role-kade only.
fn kade_only(caller: &str, role: &str) -> Result<bool, String> {
    Ok(caller == "kade" && role == "role-kade")
}

#[test]
fn kade_opening_a_kade_session_is_allowed() {
    let r = acts_as_refusal(SESSION, &WriteOp::CreateEntity, r#"{"name":"s1","actsAs":"role-kade"}"#, "kade", kade_only);
    assert_eq!(r, None);
}

#[test]
fn negative_proof_kade_opening_a_wren_session_is_403_not_served() {
    let (code, body) = acts_as_refusal(SESSION, &WriteOp::CreateEntity, r#"{"name":"s2","actsAs":"role-wren"}"#, "kade", kade_only)
        .expect("kade must not open a session as role-wren");
    assert_eq!(code, 403, "{body}");
    assert!(body.contains("not-served"), "typed reason: {body}");
    assert!(body.contains("role-wren") && body.contains("kade"), "names caller and role: {body}");
}

#[test]
fn a_principal_with_no_teammate_row_is_403() {
    let none = |_: &str, _: &str| Ok(false);
    let (code, body) = acts_as_refusal(SESSION, &WriteOp::CreateEntity, r#"{"name":"s3","actsAs":"role-borg-queen"}"#, "borg-queen", none)
        .expect("no Teammate row, no session");
    assert_eq!(code, 403, "{body}");
}

#[test]
fn a_store_that_does_not_answer_is_503_never_403() {
    let down = |_: &str, _: &str| Err("connection refused".to_string());
    let (code, body) = acts_as_refusal(SESSION, &WriteOp::CreateEntity, r#"{"name":"s4","actsAs":"role-kade"}"#, "kade", down)
        .expect("an unanswered read is not an allow");
    assert_eq!(code, 503, "{body}");
}

#[test]
fn changing_actsas_on_an_existing_session_is_checked_too() {
    let op = WriteOp::PatchEntity { name: "s1".into() };
    let r = acts_as_refusal(SESSION, &op, r#"{"actsAs":"role-wren"}"#, "kade", kade_only);
    assert_eq!(r.map(|(c, _)| c), Some(403));
}

#[test]
fn a_batch_create_is_checked_on_every_row() {
    let body = r#"[{"name":"a","actsAs":"role-kade"},{"name":"b","actsAs":"role-wren"}]"#;
    let r = acts_as_refusal(SESSION, &WriteOp::CreateBatch, body, "kade", kade_only);
    assert_eq!(r.map(|(c, _)| c), Some(403));
}

#[test]
fn other_classes_and_bodies_without_actsas_are_untouched() {
    let never = |_: &str, _: &str| -> Result<bool, String> { panic!("no Teammate read expected") };
    assert_eq!(acts_as_refusal("https://jeffbridwell.com/chorus#Card", &WriteOp::CreateEntity, r#"{"actsAs":"role-wren"}"#, "kade", never), None);
    assert_eq!(acts_as_refusal(SESSION, &WriteOp::PatchEntity { name: "s1".into() }, r#"{"lastSeenAt":"x"}"#, "kade", never), None);
}

/// Silas 16:37 — the cross-class POST /batch takes raw triples; an inserted actsAs
/// is held to the same rule, so the batch is no way around role activation.
#[test]
fn negative_proof_cross_class_batch_inserting_actsas_role_wren_as_kade_is_403() {
    let ins = vec![(
        "<https://jeffbridwell.com/chorus#session-x>".to_string(),
        "<https://jeffbridwell.com/chorus#actsAs>".to_string(),
        "<https://jeffbridwell.com/chorus#role-wren>".to_string(),
    )];
    let r = athena_make::batch_acts_as_refusal(&ins, "kade", kade_only);
    assert_eq!(r.map(|(c, _)| c), Some(403));
}

#[test]
fn control_cross_class_batch_inserting_actsas_role_kade_as_kade_is_allowed() {
    let ins = vec![(
        "<https://jeffbridwell.com/chorus#session-x>".to_string(),
        "<https://jeffbridwell.com/chorus#actsAs>".to_string(),
        "<https://jeffbridwell.com/chorus#role-kade>".to_string(),
    )];
    assert_eq!(athena_make::batch_acts_as_refusal(&ins, "kade", kade_only), None);
}
