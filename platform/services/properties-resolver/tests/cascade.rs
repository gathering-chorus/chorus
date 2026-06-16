//! #3437 cascade resolver — red-first hermetic tests (DEC-1674). No Fuseki.
//!
//! The PURE precedence core: given a TOTALLY-ORDERED scope chain (most-specific
//! first: Service > Domain > Product > ValueStreamStep > ValueStream) and a key,
//! the nearest scope that SETS the key wins; absent scopes fall through.
//!
//! Pinned by Kade (navigator, 2026-06-16):
//!  - total-ordering is a PRECONDITION — core errors on a malformed chain, never
//!    silently picks (the model question stays in the graph-walk).
//!  - provenance: Resolution carries the winning Property IRI + value_type (audit
//!    + coercion without a re-fetch) — Jeff's deterministic+auditable bar.
//!  - explicit-empty (present, value="") OVERRIDES and stops; absent falls through.

use properties_resolver::{
    decide_effective_value, CascadeError, PropertyDatum, Resolution, ScopeKind, ScopeNode,
};

fn datum(iri: &str, key: &str, value: &str, vt: &str) -> PropertyDatum {
    PropertyDatum {
        iri: iri.into(),
        key: key.into(),
        value: value.into(),
        value_type: vt.into(),
    }
}

fn node(kind: ScopeKind, iri: &str, props: Vec<PropertyDatum>) -> ScopeNode {
    ScopeNode { kind, iri: iri.into(), properties: props }
}

/// A well-formed, totally-ordered chain where every level sets "alert.threshold".
fn full_chain() -> Vec<ScopeNode> {
    vec![
        node(ScopeKind::Service, "svc:api", vec![datum("p:svc", "alert.threshold", "5", "int")]),
        node(ScopeKind::Domain, "dom:obs", vec![datum("p:dom", "alert.threshold", "10", "int")]),
        node(ScopeKind::Product, "prod:borg", vec![datum("p:prod", "alert.threshold", "20", "int")]),
        node(ScopeKind::ValueStream, "vs:gather", vec![datum("p:vs", "alert.threshold", "100", "int")]),
    ]
}

#[test]
fn service_wins_when_all_levels_set_the_key() {
    let r = decide_effective_value(&full_chain(), "alert.threshold").unwrap();
    let r = r.expect("a value resolves");
    assert_eq!(r.value, "5");
    assert_eq!(r.winning_scope_kind, ScopeKind::Service);
}

#[test]
fn gap_at_service_falls_through_to_domain() {
    let mut chain = full_chain();
    chain[0].properties.clear(); // service does not set the key
    let r = decide_effective_value(&chain, "alert.threshold").unwrap().expect("resolves");
    assert_eq!(r.value, "10");
    assert_eq!(r.winning_scope_kind, ScopeKind::Domain);
}

#[test]
fn multi_level_chain_nearest_set_wins() {
    // only the value-stream (broadest) sets the key
    let mut chain = full_chain();
    for n in chain.iter_mut().take(3) {
        n.properties.clear();
    }
    let r = decide_effective_value(&chain, "alert.threshold").unwrap().expect("resolves");
    assert_eq!(r.value, "100");
    assert_eq!(r.winning_scope_kind, ScopeKind::ValueStream);
}

#[test]
fn no_match_returns_none() {
    let r = decide_effective_value(&full_chain(), "nonexistent.key").unwrap();
    assert!(r.is_none(), "no scope sets the key -> None, not an error");
}

#[test]
fn malformed_duplicate_rank_is_error() {
    let chain = vec![
        node(ScopeKind::Service, "svc:a", vec![]),
        node(ScopeKind::Service, "svc:b", vec![]), // duplicate rank
    ];
    assert!(matches!(
        decide_effective_value(&chain, "k"),
        Err(CascadeError::MalformedChain(_))
    ));
}

#[test]
fn malformed_increasing_specificity_is_error() {
    let chain = vec![
        node(ScopeKind::Domain, "dom:x", vec![]),
        node(ScopeKind::Service, "svc:y", vec![]), // less-specific-first then more-specific = not totally ordered
    ];
    assert!(matches!(
        decide_effective_value(&chain, "k"),
        Err(CascadeError::MalformedChain(_))
    ));
}

#[test]
fn explicit_empty_at_service_beats_domain_value() {
    // service sets the key to "" EXPLICITLY -> that is an override-to-empty, it WINS.
    let chain = vec![
        node(ScopeKind::Service, "svc:api", vec![datum("p:svc", "feature.flag", "", "string")]),
        node(ScopeKind::Domain, "dom:obs", vec![datum("p:dom", "feature.flag", "on", "string")]),
    ];
    let r = decide_effective_value(&chain, "feature.flag").unwrap().expect("explicit-empty resolves");
    assert_eq!(r.value, "", "present-empty is an explicit override, not a fall-through");
    assert_eq!(r.winning_scope_kind, ScopeKind::Service);
    assert_eq!(r.winning_property_iri, "p:svc");
}

#[test]
fn absent_at_service_falls_through_to_domain() {
    // service has NO datum for the key (absent) -> fall through to domain.
    let chain = vec![
        node(ScopeKind::Service, "svc:api", vec![datum("p:svc", "other.key", "x", "string")]),
        node(ScopeKind::Domain, "dom:obs", vec![datum("p:dom", "feature.flag", "on", "string")]),
    ];
    let r = decide_effective_value(&chain, "feature.flag").unwrap().expect("resolves at domain");
    assert_eq!(r.value, "on");
    assert_eq!(r.winning_scope_kind, ScopeKind::Domain);
}

#[test]
fn provenance_names_winning_property_iri_and_value_type() {
    let r = decide_effective_value(&full_chain(), "alert.threshold").unwrap().expect("resolves");
    assert_eq!(r.winning_property_iri, "p:svc", "audit: trace WHY a value resolved");
    assert_eq!(r.value_type, "int", "consumer coerces without re-fetching the Property");
    let _: Resolution = r; // type assertion: shape is the frozen contract
}
