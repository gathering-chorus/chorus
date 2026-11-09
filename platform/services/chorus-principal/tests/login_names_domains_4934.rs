// @test-type: unit — the domains a login establishes, read from the door's Permission rows
// @domain: identity
//! #4934 AC2 — Jeff 2026-11-09 08:50: "i want to be able to see in a trace how we do
//! both authn and authz and establish our login domains and make api calls to the
//! domains". The domains a role may write are its Principal's Permission rows
//! (agent + accessTo + mode; #4183 retired hasScope). Login reads them through the
//! door and names them on its trace, one event. Measured 08:58 for borg-queen:
//! cards Write; decisions, identity, memory Append.
//!
//! NEGATIVE PROOF (#3734): another principal's grant is never named, and an
//! unreadable listing is an error, never an empty "no domains".

use chorus_principal::rows::login_domains;
use serde_json::json;

fn listing(rows: serde_json::Value) -> String { json!({ "data": rows }).to_string() }

#[test]
fn the_roles_permission_rows_become_its_login_domains() {
    let l = listing(json!([
        {"agent":"principal-borg-queen","mode":"http://www.w3.org/ns/auth/acl#Write","accessTo":"urn:chorus:domains:cards"},
        {"agent":"principal-borg-queen","mode":"http://www.w3.org/ns/auth/acl#Append","accessTo":"urn:chorus:domains:memory"},
        {"agent":"principal-borg-queen","mode":"http://www.w3.org/ns/auth/acl#Append","accessTo":"urn:chorus:domains:decisions"},
    ]));
    assert_eq!(login_domains(&l, "borg-queen").unwrap(), vec!["cards:Write", "decisions:Append", "memory:Append"]);
}

#[test]
fn negative_proof_another_principals_grant_is_never_named() {
    let l = listing(json!([
        {"agent":"principal-borg-queen","mode":"http://www.w3.org/ns/auth/acl#Write","accessTo":"urn:chorus:domains:cards"},
        {"agent":"principal-wren","mode":"http://www.w3.org/ns/auth/acl#Write","accessTo":"urn:chorus:domains:roles"},
    ]));
    let d = login_domains(&l, "borg-queen").unwrap();
    assert!(!d.iter().any(|x| x.starts_with("roles")), "wren's grant named for borg-queen: {d:?}");
}

#[test]
fn negative_proof_an_unreadable_listing_is_an_error_not_no_domains() {
    assert!(login_domains("not json", "borg-queen").is_err());
    assert!(login_domains(r#"{"error":"unknown route"}"#, "borg-queen").is_err());
}

#[test]
fn a_role_with_no_grants_has_none_named_and_that_is_not_an_error() {
    assert_eq!(login_domains(&listing(json!([])), "kade").unwrap(), Vec::<String>::new());
}
