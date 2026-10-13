// @test-type: unit
// @domain: security
//! #4518 — a product's verifier reads ITS OWN identity model, never Chorus's.
//!
//! Each tests/*.rs file is its own process, so this one configures the wine
//! diary's model once and asks every resolver what it would send to the store.
//! What Art sees: his sign-in is checked against the diary's people and grants
//! only; nothing in Chorus's security graphs can let anyone into his diary.

use chorus_oidc::identity_model::{configure, identity_model, IdentityModel};
use chorus_oidc::oidc;
use std::cell::RefCell;

const NS: &str = "https://jeffbridwell.com/wine-diary#";
const GP: &str = "urn:wine-diary:";

fn wine_diary() {
    configure(IdentityModel::new(NS, GP)).expect("one model per process");
}

/// Every query the verifier would send, captured through the injected store.
fn every_query() -> Vec<String> {
    wine_diary();
    let seen = RefCell::new(Vec::new());
    let ask = |q: &str| { seen.borrow_mut().push(q.to_string()); None::<String> };
    oidc::prime_allow_set_graph(ask);
    let _ = oidc::resolve_principal_webids(ask);
    let _ = oidc::resolve_person_clients(ask);
    let _ = oidc::resolve_principal_names(ask);
    let _ = oidc::resolve_principal_roles(ask);
    let _ = oidc::resolve_principal_scopes(ask);
    seen.into_inner()
}

#[test]
fn every_resolver_asks_the_products_graphs_and_vocabulary() {
    let qs = every_query();
    assert_eq!(qs.len(), 6, "six resolvers, six queries: {qs:#?}");
    for q in &qs {
        assert!(q.contains(NS), "names the diary's vocabulary: {q}");
        assert!(q.contains(GP), "names the diary's graphs: {q}");
    }
}

#[test]
fn no_query_can_reach_chorus_identity() {
    // NEGATIVE PROOF, the guarded condition: a single query left on Chorus's
    // namespace or graphs would let Chorus's people into the diary. Removing the
    // localize call from any one resolver in src/oidc.rs turns this red.
    for q in every_query() {
        assert!(!q.contains("jeffbridwell.com/chorus#"), "Chorus vocabulary leaked: {q}");
        assert!(!q.contains("urn:chorus:"), "Chorus graph leaked: {q}");
    }
}

#[test]
fn the_security_graph_defaults_under_the_product_prefix() {
    wine_diary();
    // No CHORUS_ALLOW_SET_GRAPH in this process: the default follows the model.
    assert_eq!(oidc::allow_set_graph(), "urn:wine-diary:domains:security");
}

#[test]
fn a_principal_home_outside_the_product_is_ignored() {
    wine_diary();
    let rows = vec!["urn:chorus:domains:identity".to_string()];
    assert_eq!(
        oidc::principal_home_from(&rows, "urn:wine-diary:domains:security"),
        "urn:wine-diary:domains:security",
        "a Chorus graph named as home must not move the diary's verifier onto it"
    );
    let ours = vec!["urn:wine-diary:domains:access".to_string()];
    assert_eq!(oidc::principal_home_from(&ours, "x"), "urn:wine-diary:domains:access");
}

#[test]
fn the_model_cannot_be_swapped_mid_run() {
    wine_diary();
    assert!(configure(IdentityModel::chorus()).is_err(), "a second, different model is refused");
    assert_eq!(identity_model().namespace, NS);
}
