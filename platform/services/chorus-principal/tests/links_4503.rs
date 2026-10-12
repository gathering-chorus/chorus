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
