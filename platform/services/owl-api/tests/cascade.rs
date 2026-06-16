//! #3437 — cascade resolver (Properties C). The four AC cases, 1:1.
//!
//! Effective config = the NEAREST override along a node's containment ancestry
//! (Service > SubDomain > Domain > Product > ValueStreamStep > ValueStream).
//! Pure logic, no live graph — the specificity LAW is what Properties A (#3435)
//! bakes in at projection time. Integration test = drives the public surface A calls.

use owl_api::cascade::{resolve, Binding, Effective, Level};

// AC4 case 1 — OVERRIDE-WINS: same key on Service AND Domain; the nearer wins.
#[test]
fn override_wins_nearest_node_beats_ancestor() {
    let bindings = vec![
        Binding::new("alert.threshold", "0.95", "string", Level::Domain),
        Binding::new("alert.threshold", "0.80", "string", Level::Service),
    ];
    let eff: Effective = resolve("alert.threshold", &bindings).expect("a value resolves");
    assert_eq!(eff.value, "0.80", "Service (nearer) overrides Domain");
    assert_eq!(eff.level, Level::Service, "provenance is the winning level");
}

// AC4 case 2 — INHERIT-WHEN-ABSENT: Service has no own value; inherits Product's.
#[test]
fn inherit_when_absent_uses_ancestor_value() {
    let bindings = vec![Binding::new("deploy.timeout", "30", "int", Level::Product)];
    let eff = resolve("deploy.timeout", &bindings).expect("inherited value resolves");
    assert_eq!(eff.value, "30", "inherits the Product value when the Service has none");
    assert_eq!(eff.level, Level::Product, "provenance is the ancestor it inherited from");
    assert_eq!(eff.value_type, "int", "the declared type is carried through untouched");
}

// AC4 case 3 — MULTI-LEVEL CHAIN: key at four levels → the single most specific,
// regardless of input order (order must not get a vote — the level rank decides).
#[test]
fn multi_level_chain_resolves_most_specific() {
    let bindings = vec![
        Binding::new("k", "vs", "string", Level::ValueStream),
        Binding::new("k", "svc", "string", Level::Service),
        Binding::new("k", "prod", "string", Level::Product),
        Binding::new("k", "dom", "string", Level::Domain),
    ];
    let eff = resolve("k", &bindings).expect("a value resolves");
    assert_eq!(eff.value, "svc", "Service is most specific across the whole chain");
    assert_eq!(eff.level, Level::Service);
}

// AC4 case 4 — NO-MATCH: key bound nowhere → None (caller falls back to code default).
#[test]
fn no_match_returns_none() {
    let bindings = vec![Binding::new("other.key", "x", "string", Level::Service)];
    assert!(resolve("missing.key", &bindings).is_none(), "unset key → None");
    assert!(resolve("missing.key", &[]).is_none(), "empty chain → None");
}

// The specificity ladder is the LAW — pin the full order so a future hierarchy
// edit can't silently reweight the cascade.
#[test]
fn rank_order_is_service_to_valuestream() {
    let ranked = [
        Level::Service,
        Level::SubDomain,
        Level::Domain,
        Level::Product,
        Level::ValueStreamStep,
        Level::ValueStream,
    ];
    for w in ranked.windows(2) {
        assert!(w[0].rank() < w[1].rank(), "{:?} must be more specific than {:?}", w[0], w[1]);
    }
}

// from_class maps EXACTLY the chorus:hasProperty union domain (chorus.ttl) and
// rejects non-config-bearing classes — the gather step drops those.
#[test]
fn from_class_maps_the_hasproperty_union_domain() {
    assert_eq!(Level::from_class("Service"), Some(Level::Service));
    assert_eq!(Level::from_class("SubDomain"), Some(Level::SubDomain));
    assert_eq!(Level::from_class("Domain"), Some(Level::Domain));
    assert_eq!(Level::from_class("Product"), Some(Level::Product));
    assert_eq!(Level::from_class("ValueStreamStep"), Some(Level::ValueStreamStep));
    assert_eq!(Level::from_class("ValueStream"), Some(Level::ValueStream));
    assert_eq!(Level::from_class("Property"), None, "a Property is not a config-bearing node");
    assert_eq!(Level::from_class("File"), None);
}

// SubDomain sits between Service and Domain — a SubDomain override beats the
// Domain it specializes (prove the rung is real, not decorative).
#[test]
fn subdomain_overrides_domain() {
    let bindings = vec![
        Binding::new("k", "dom", "string", Level::Domain),
        Binding::new("k", "sub", "string", Level::SubDomain),
    ];
    assert_eq!(resolve("k", &bindings).unwrap().value, "sub");
}
