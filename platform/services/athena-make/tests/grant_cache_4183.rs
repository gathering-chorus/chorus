// #4183 reopen 1 — a Permission write drops the cached scopes; a scope refusal
// says how old the scope it judged by is.
use athena_make::{grants_changed, scope_age_note};

#[test]
fn only_a_permission_write_drops_the_scope_cache() {
    assert!(grants_changed("https://jeffbridwell.com/chorus#Permission"));
    // NEGATIVE PROOF: an ordinary row write leaves the cache alone
    assert!(!grants_changed("https://jeffbridwell.com/chorus#CodeKind"));
    assert!(!grants_changed("https://jeffbridwell.com/chorus#PermissionNote"));
}

#[test]
fn a_refusal_names_the_age_of_its_scope() {
    assert_eq!(scope_age_note(Some(240)), " (scopes read 240s ago)");
    assert_eq!(scope_age_note(None), "", "never read: nothing to claim");
}

#[test]
fn the_authz_refusal_names_its_scope_age_too() {
    use athena_make::{create_refusal, set_write_scope_age, WriteOp};
    set_write_scope_age(Some(17));
    let (code, body) = create_refusal(&WriteOp::CreateEntity, "urn:chorus:domains:heralds", &[], "quartet")
        .expect("no Write row: refused");
    assert_eq!(code, 403);
    assert!(body.contains("(scopes read 17s ago)"), "{body}");
    set_write_scope_age(None);
    let (_, body) = create_refusal(&WriteOp::CreateEntity, "urn:chorus:domains:heralds", &[], "quartet").unwrap();
    assert!(!body.contains("scopes read"), "unknown age claims nothing: {body}");
}
