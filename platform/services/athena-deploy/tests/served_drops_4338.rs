//! #4338 — a land fails when it takes away what was served before it. On
//! 2026-10-02 15:51 the serve step checked /health only, so athena-make came back
//! with 12 fewer Domains and their classes' routes, and the land read as served.
use athena_deploy::{served_drops, served_snapshot, unapproved_drops};

const DISCO: &str = r#"{ "kind": "Discovery", "primitives": [
  { "kind": "Machine", "collection": "/v1/infrastructure/machines", "openapi": "/v1/infrastructure/machines/openapi.json" },
  { "kind": "Session", "collection": "/v1/identity/sessions" },
  { "kind": "ADR", "collection":"/v1/decisions/adrs" } ] }"#;
const COUNT_60: &str = "n\n60\n";

#[test]
fn a_snapshot_lists_every_route_and_the_domain_count() {
    let s = served_snapshot(DISCO, COUNT_60).unwrap();
    assert_eq!(s, "route /v1/decisions/adrs\nroute /v1/identity/sessions\nroute /v1/infrastructure/machines\ndomains 60\n");
}

#[test]
fn negative_proof_one_route_disappears() {
    let before = served_snapshot(DISCO, COUNT_60).unwrap();
    let after = served_snapshot(&DISCO.replace("/v1/identity/sessions", "/v1/identity/other"), COUNT_60).unwrap();
    assert_eq!(served_drops(&before, &after), vec!["route gone: /v1/identity/sessions"]);
}

#[test]
fn negative_proof_the_domain_count_drops() {
    let before = served_snapshot(DISCO, COUNT_60).unwrap();
    let after = served_snapshot(DISCO, "n\n49\n").unwrap();
    assert_eq!(served_drops(&before, &after), vec!["domains 60 -> 49"]);
}

#[test]
fn an_unchanged_or_grown_surface_is_not_a_drop() {
    let before = served_snapshot(DISCO, COUNT_60).unwrap();
    assert!(served_drops(&before, &before).is_empty());
    let grown = DISCO.replace("] }", ", { \"kind\": \"Tool\", \"collection\": \"/v1/toolchain/tools\" } ] }");
    assert!(served_drops(&before, &served_snapshot(&grown, "n\n61\n").unwrap()).is_empty());
}

#[test]
fn an_unmeasured_side_refuses_instead_of_passing() {
    // a dead service answers nothing: no snapshot, never an empty one
    assert!(served_snapshot("", COUNT_60).is_err());
    assert!(served_snapshot(DISCO, "").is_err());
    // a before file with no count cannot vouch for the count
    let after = served_snapshot(DISCO, COUNT_60).unwrap();
    assert_eq!(served_drops("route /v1/decisions/adrs\n", &after).len(), 1);
}

#[test]
fn a_drop_named_exactly_in_the_allow_list_passes_and_nothing_else_does() {
    let before = served_snapshot(DISCO, COUNT_60).unwrap();
    let after = served_snapshot(&DISCO.replace("/v1/identity/sessions", "/v1/identity/other"), "n\n59\n").unwrap();
    let drops = served_drops(&before, &after);
    // the card retires one Domain on purpose and says so
    assert_eq!(unapproved_drops(drops.clone(), "domains 60 -> 59\n"), vec!["route gone: /v1/identity/sessions"]);
    // NEGATIVE PROOF: a near-miss allow line ("60 -> 58") approves nothing
    assert_eq!(unapproved_drops(drops.clone(), "domains 60 -> 58\n").len(), 2);
    assert_eq!(unapproved_drops(drops, "").len(), 2);
}
