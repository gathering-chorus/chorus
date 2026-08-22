//! #3863 — the effective-config chain is the node's OWN ancestry.
//!
//! Prior work: `decide_effective_value` (properties-resolver) already walks a
//! multi-node chain and refuses one that is not strictly descending by
//! specificity. `ScopeKind::rank` has ordered Role(6) → ValueStream(1) since
//! #3437. None of it has ever run against a real chain, because
//! `effective_response` built ONE node and hardcoded `ScopeKind::Service`
//! (lib.rs:2150) for whatever was asked about. The cascade existed; nothing
//! ever cascaded.
//!
//! Silas settled the shape 2026-08-13: the chain is the node's own ancestry,
//! anchored by the key's `appliesToClass`. No actor parameter. A role-rooted
//! chain never contains a Service, so Role and Service never compete — a
//! per-actor view of a service key is a different key.
//!
//! NEGATIVE PROOF (#3734): every test below fails against the pre-#3863 code.
//! `build_scope_chain` and `scope_kind_from_class` do not exist, and a
//! one-element chain with an assumed kind cannot express precedence at all.
//! The load-bearing case is `the_nearer_scope_wins_and_the_response_says_which`
//! — a hardcoded-Service build cannot pass it even by accident, because it
//! never sees the second scope.

use athena_make::{build_scope_chain, effective_response, scope_kind_from_class, ScopeKind};

/// Rows carry the owning node and its class so the chain builds in ONE
/// round-trip: `owner|ownerClass|propIri|key|valueType|value`. `value` stays
/// LAST (splitn) so a config value may itself contain '|'.
fn row(owner: &str, class: &str, prop: &str, key: &str, vt: &str, v: &str) -> String {
    format!("{}|{}|{}|{}|{}|{}", owner, class, prop, key, vt, v)
}

#[test]
fn scope_kind_is_read_from_the_class_never_assumed() {
    assert_eq!(
        scope_kind_from_class("https://jeffbridwell.com/chorus#Role"),
        Some(ScopeKind::Role)
    );
    assert_eq!(scope_kind_from_class("Service"), Some(ScopeKind::Service));
    assert_eq!(
        scope_kind_from_class("ValueStreamStep"),
        Some(ScopeKind::ValueStreamStep)
    );
    // A class that is not a scope is NOT coerced into one. The old code's
    // failure mode was exactly this — everything became Service.
    assert_eq!(scope_kind_from_class("TestCoverage"), None);
}

#[test]
fn a_chain_orders_most_specific_first() {
    let rows = vec![
        row("https://x#chorus", "Product", "https://x#p1", "response.word.cap", "int", "300"),
        row("https://x#role-wren", "Role", "https://x#p2", "response.word.cap", "int", "100"),
    ];
    let chain = build_scope_chain(&rows).expect("chain builds");
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0].kind, ScopeKind::Role);
    assert_eq!(chain[1].kind, ScopeKind::Product);
}

#[test]
fn several_properties_on_one_scope_collapse_to_one_node() {
    let rows = vec![
        row("https://x#role-wren", "Role", "https://x#p1", "response.word.cap", "int", "100"),
        row("https://x#role-wren", "Role", "https://x#p2", "nudge.word.cap", "int", "50"),
    ];
    let chain = build_scope_chain(&rows).expect("chain builds");
    assert_eq!(chain.len(), 1, "one scope, not one per property");
    assert_eq!(chain[0].properties.len(), 2);
}

#[test]
fn a_malformed_row_fails_loud_never_a_dropped_scope() {
    let rows = vec!["not|enough".to_string()];
    assert!(build_scope_chain(&rows).is_err());
}

#[test]
fn a_row_whose_class_is_not_a_scope_fails_loud() {
    let rows = vec![row("https://x#t", "TestCoverage", "https://x#p", "k", "string", "v")];
    assert!(build_scope_chain(&rows).is_err());
}

/// The point of the card. A one-element chain cannot express this: the nearer
/// scope must win, and the response must name which one did.
#[test]
fn the_nearer_scope_wins_and_the_response_says_which() {
    let rows = vec![
        row("https://x#chorus", "Product", "https://x#p1", "response.word.cap", "int", "300"),
        row("https://x#role-wren", "Role", "https://x#p2", "response.word.cap", "int", "100"),
    ];
    let (code, body) = effective_response("role-wren", "response.word.cap", &rows);
    assert_eq!(code, 200, "body: {}", body);
    assert!(body.contains("\"value\":100"), "role must beat product: {}", body);
    assert!(body.contains("role-wren"), "winning scope must be named: {}", body);
}

/// The other direction, so the fix cannot be "always take the first row".
#[test]
fn a_key_only_the_farther_scope_sets_still_resolves() {
    let rows = vec![
        row("https://x#chorus", "Product", "https://x#p1", "nudge.word.cap", "int", "50"),
        row("https://x#role-wren", "Role", "https://x#p2", "response.word.cap", "int", "100"),
    ];
    let (code, body) = effective_response("role-wren", "nudge.word.cap", &rows);
    assert_eq!(code, 200, "body: {}", body);
    assert!(body.contains("\"value\":50"), "product value must resolve: {}", body);
}

// ── the fetch: the query must actually produce what the parser expects ──
//
// This is the seam that would otherwise reproduce today's recurring defect in a
// new costume: chain-builder correct, query still emitting 4-field rows, every
// live request 500ing while the unit tests stay green. The row contract is
// asserted by FIELD COUNT, not by "contains CONCAT" — the old test asserted the
// latter and would have passed through this entire change.

#[test]
fn the_fetch_walks_ancestry_not_just_the_node() {
    let q = athena_make::effective_fetch_query("https://x#role-wren", "urn:chorus:instances");
    assert!(q.contains("urn:chorus:instances"), "must read the live instances graph");
    assert!(q.contains("https://x#role-wren"), "anchored on the node");
    // zero-or-more containment hops: the node ITSELF plus its ancestors.
    assert!(q.contains('*'), "ancestry walk must be a zero-or-more path: {}", q);
    assert!(!q.contains("FILTER"), "key-selection stays in pure code");
}

#[test]
fn the_fetch_row_has_six_fields_matching_the_parser() {
    let q = athena_make::effective_fetch_query("https://x#role-wren", "urn:chorus:instances");
    // Five separators = six fields = owner|class|prop|key|type|value.
    let seps = q.matches("\"|\"").count();
    assert_eq!(seps, 5, "row must carry owner+class before the property fields: {}", q);
    assert!(q.contains("AS ?v"), "single-var seam preserved");
}

/// A key nobody sets is still 404 — a chain must not invent a value.
#[test]
fn an_unset_key_is_still_404() {
    let rows = vec![row(
        "https://x#role-wren", "Role", "https://x#p", "response.word.cap", "int", "100",
    )];
    let (code, _) = effective_response("role-wren", "no.such.key", &rows);
    assert_eq!(code, 404);
}
