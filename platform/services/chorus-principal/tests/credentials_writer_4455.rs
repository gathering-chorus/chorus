// @domain: identity
//! #4455 — `chorus-principal credentials <role>` writes as the caller's own
//! role. With no CHORUS_ROLE it refuses; it never borrows a teammate's name.
use chorus_principal::credentials_writer;

#[test]
fn the_callers_role_is_the_writer() {
    assert_eq!(credentials_writer(Some("kade".into())), Ok("kade".to_string()));
}

#[test]
fn negative_proof_no_role_is_refused_not_defaulted() {
    assert_eq!(credentials_writer(None), Err(2));
    assert_eq!(credentials_writer(Some("  ".into())), Err(2));
}
