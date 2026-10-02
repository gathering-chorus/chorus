//! #4419 — a card runs every registered test in the domains its changed files
//! touch, any layer. Jeff, 2026-10-01: "both blast radius and per card test
//! execution require in synch domains that are tagged properly."
use std::collections::HashMap;
use werk_test::{domain_selection, parse_domains_of, TestRow};

fn row(f: &str, covers: &str, layer: &str) -> TestRow {
    TestRow { file_path: f.into(), covers: covers.into(), pyramid_layer: layer.into(), hermeticity: String::new(), test_concern: String::new() }
}

fn rows() -> Vec<TestRow> {
    vec![
        row("platform/api/tests/domain-page.integration.test.ts", "https://jeffbridwell.com/chorus#domains", "integration"),
        row("platform/api/tests/handlers/chorus-domain-pipeline.test.ts", "domains", "unit"),
        row("platform/api/tests/discover-pages.integration.test.ts", "code", "integration"),
        row("directing/clearing/tests/tunnel-auth.integration.test.ts", "messages", "integration"),
        row("platform/tests/4202-principal-login.bats", "roles", ""),
    ]
}

#[test]
fn replay_4353_server_ts_selects_the_domains_tests_the_nightly_failed() {
    let placed = parse_domains_of("platform/api/src/server.ts\tdomains\n");
    let s = domain_selection(&["platform/api/src/server.ts".into()], &placed, &rows());
    assert!(s.tests.contains_key("platform/api/tests/domain-page.integration.test.ts"));
    assert!(s.tests.contains_key("platform/api/tests/handlers/chorus-domain-pipeline.test.ts"));
    assert!(!s.tests.contains_key("directing/clearing/tests/tunnel-auth.integration.test.ts"));
    assert_eq!(s.tests["platform/api/tests/domain-page.integration.test.ts"], "domain:domains");
}

#[test]
fn replay_4417_clearing_server_selects_tunnel_auth_integration() {
    let placed = parse_domains_of("directing/clearing/src/server.ts\tmessages\n");
    let s = domain_selection(&["directing/clearing/src/server.ts".into()], &placed, &rows());
    assert!(s.tests.contains_key("directing/clearing/tests/tunnel-auth.integration.test.ts"));
}

#[test]
fn negative_proof_without_the_domain_tag_nothing_is_selected_and_the_file_is_named_untagged() {
    let placed = parse_domains_of("directing/clearing/src/server.ts\t\n");
    let s = domain_selection(&["directing/clearing/src/server.ts".into()], &placed, &rows());
    assert!(s.tests.is_empty());
    assert_eq!(s.untagged, vec!["directing/clearing/src/server.ts".to_string()]);
}

#[test]
fn a_file_with_two_domains_selects_both_and_a_changed_test_selects_itself() {
    let placed = parse_domains_of("platform/api/src/x.ts\tdomains,code\nplatform/tests/4202-principal-login.bats\troles\n");
    let s = domain_selection(&["platform/api/src/x.ts".into(), "platform/tests/4202-principal-login.bats".into()], &placed, &rows());
    assert!(s.tests.contains_key("platform/api/tests/discover-pages.integration.test.ts"));
    assert!(s.tests.contains_key("platform/api/tests/domain-page.integration.test.ts"));
    assert!(s.tests.contains_key("platform/tests/4202-principal-login.bats"));
    let d: Vec<&str> = s.domains.iter().map(|x| x.as_str()).collect();
    assert_eq!(d, vec!["code", "domains", "roles"]);
}

#[test]
fn a_path_the_seam_did_not_answer_is_untagged_not_silently_dropped() {
    let placed: HashMap<String, Vec<String>> = HashMap::new();
    let s = domain_selection(&["platform/api/src/new.ts".into()], &placed, &rows());
    assert_eq!(s.untagged, vec!["platform/api/src/new.ts".to_string()]);
}
