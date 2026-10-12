//! #4503 — which WebIDs leave Jeff's sign-in, against account-API answers shaped
//! as the provider gave them on 2026-10-12 (12 links, 33 client credentials).

use chorus_principal::links::*;

const LINKS: &str = r#"{"webIdLinks":{"https://id.example.test/jeff/profile/card#me":"https://id.example.test/.account/account/A/webid/1/","https://id.example.test/zzz-test/profile/card#me":"https://id.example.test/.account/account/A/webid/2/","https://id.example.test/silas/profile/card#me":"https://id.example.test/.account/account/A/webid/3/"}}"#;
const CREDS: &str = r#"{"clientCredentials":{"chorus-agent-zzz-test_6c9c":"https://id.example.test/.account/account/A/client-credentials/c1/","chorus-agent-silas_2580":"https://id.example.test/.account/account/A/client-credentials/c2/","chorus-3669-silas_a5de":"https://id.example.test/.account/account/A/client-credentials/c3/","gathering-app_5cf0":"https://id.example.test/.account/account/A/client-credentials/c4/"}}"#;

#[test]
fn the_account_lists_its_webids_with_their_link_urls() {
    let l = webid_links(LINKS);
    assert_eq!(l.len(), 3);
    let names: Vec<_> = l.iter().filter_map(|(w, _)| webid_name(w)).collect();
    assert_eq!(names, vec!["jeff", "zzz-test", "silas"]);
    assert!(l.iter().any(|(w, u)| w.contains("/zzz-test/") && u.ends_with("/webid/2/")));
}

#[test]
fn a_names_credentials_are_matched_whole_never_by_prefix() {
    assert_eq!(credentials_for(CREDS, "zzz-test"), vec!["https://id.example.test/.account/account/A/client-credentials/c1/"]);
    assert_eq!(credentials_for(CREDS, "silas").len(), 2, "both labels: chorus-agent- and chorus-3669-");
    assert!(credentials_for(CREDS, "si").is_empty(), "NEGATIVE: a prefix of a name matches nothing");
    assert!(credentials_for(CREDS, "gathering").is_empty());
}

#[test]
fn only_a_name_with_no_login_and_no_principal_may_be_unlinked() {
    assert_eq!(unlink_refusal("zzz-test", false, false), None);
    assert!(unlink_refusal("silas", true, true).unwrap().contains("move it to its own account first"));
    assert!(unlink_refusal("canary", true, false).unwrap().contains("logs in today"), "NEGATIVE: a live login is refused");
    assert!(unlink_refusal("crawler", false, true).is_some(), "NEGATIVE: a Principal is refused");
    assert!(unlink_refusal("jeff", false, false).is_some(), "his own WebID never leaves");
}

#[test]
fn an_answer_without_links_lists_nothing_rather_than_guessing() {
    assert!(webid_links("{}").is_empty());
    assert!(credentials_for(r#"{"error":"unauthorized"}"#, "zzz-test").is_empty());
}

#[test]
fn the_link_refusal_names_the_token_to_write() {
    let reply = r#"{"name":"BadRequestHttpError","message":"Verification token not found. Please add the RDF triple <https://id.example.test/zzz-test/profile/card#me> <http://www.w3.org/ns/solid/terms#oidcIssuerRegistrationToken> \"09a2b112-6ecc-4e05-83de-93b12dd95661\". to the WebID document"}"#;
    assert_eq!(registration_token(reply).as_deref(), Some("09a2b112-6ecc-4e05-83de-93b12dd95661"));
    assert_eq!(registration_token(r#"{"name":"BadRequestHttpError","message":"something else"}"#), None, "NEGATIVE: any other refusal is not a token");
}

#[test]
fn the_profile_is_restored_byte_for_byte_after_the_link() {
    let card = "@prefix solid: <http://www.w3.org/ns/solid/terms#>.\n<#me> solid:oidcIssuer <https://id.example.test/>.\n";
    let w = "https://id.example.test/zzz-test/profile/card#me";
    let with = format!("{card}{}", registration_line(w, "abc-123"));
    assert!(with.contains("oidcIssuerRegistrationToken> \"abc-123\""));
    assert_eq!(without_registration_line(&with, w, "abc-123"), card);
}

#[test]
fn a_swapped_credential_keeps_the_webid_and_issuer_and_changes_only_id_and_secret() {
    let old = r#"{"agent":"canary","id":"chorus-agent-canary_old","secret":"s-old","issuer":"https://id.example.test/","tokenEndpoint":"https://id.example.test/.oidc/token","webId":"https://id.example.test/canary/profile/card#me"}"#;
    let new = swapped_credential(old, "chorus-agent-canary_new", "s-new").unwrap();
    assert!(new.contains(r#""id":"chorus-agent-canary_new""#) && new.contains(r#""secret":"s-new""#));
    assert!(new.contains(r#""webId":"https://id.example.test/canary/profile/card#me""#) && new.contains(r#""agent":"canary""#));
    assert!(!new.contains("s-old") && !new.contains("_old"));
    assert_eq!(swapped_credential(r#"{"token":"x"}"#, "a", "b"), None, "NEGATIVE: an unexpected file is never rewritten");
}

#[test]
fn fields_are_read_from_a_pretty_printed_credential_file() {
    let pretty = "{\n  \"agent\": \"canary\",\n  \"webId\": \"https://id.example.test/canary/profile/card#me\"\n}";
    assert_eq!(json_field(pretty, "webId").as_deref(), Some("https://id.example.test/canary/profile/card#me"));
    assert_eq!(json_field(r#"{"webId":"x"}"#, "webId").as_deref(), Some("x"));
    assert_eq!(json_field(pretty, "secret"), None);
}

#[test]
fn a_control_is_found_inside_its_block_whatever_the_key_order() {
    let c = r#"{"controls":{"password":{"forgot":"https://x/forgot/","create":"https://x/account/A/login/password/"},"account":{"create":"https://x/account/","webId":"https://x/account/A/webid/"}}}"#;
    assert_eq!(control(c, "password", "create").as_deref(), Some("https://x/account/A/login/password/"));
    assert_eq!(control(c, "account", "create").as_deref(), Some("https://x/account/"), "NEGATIVE: another block's create is never taken");
    assert_eq!(control(c, "pod", "create"), None);
}
