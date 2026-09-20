//! #3752 / #4166 — the serve-gate on claim retirement. A claim under a live
//! route must not retire, and an unanswerable athena-make must not read as
//! "nothing is served".
use athena_deploy::{route_for_class, route_is_served, serve_check_answered};

#[test]
fn a_class_name_becomes_its_collection() {
    assert_eq!(route_for_class("Card"), "cards");
    assert_eq!(route_for_class("Policy"), "policies");
    assert_eq!(route_for_class("Property"), "properties");
    assert_eq!(route_for_class("PropertyKey"), "propertykeys");
}

#[test]
fn negative_proof_a_versioned_domain_route_still_matches() {
    // The bug that made this guard vacuous until #4166: routes carry a version
    // and a domain, so a bare "/credentials" needle never matched and NO
    // staged claim could be refused for being served.
    let resp = r#"{"served":["/v1/security/credentials","/v1/tests/results"]}"#;
    assert!(route_is_served(resp, "credentials"), "a live surface must be seen");
    assert!(route_is_served(resp, "results"));
    assert!(!route_is_served(resp, "cards"), "an unserved class must not match");
}

#[test]
fn negative_proof_an_unanswered_door_is_not_an_empty_route_list() {
    // #4080: defer, never blind-execute — and never read silence as "clear".
    assert!(!serve_check_answered(""));
    assert!(!serve_check_answered("curl: (7) Failed to connect"));
    assert!(serve_check_answered(r#"{"served":[]}"#));
    assert!(!route_is_served("", "cards"));
}
