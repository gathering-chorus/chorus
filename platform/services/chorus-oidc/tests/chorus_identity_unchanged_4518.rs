// @test-type: unit
// @domain: security
//! #4518 — with no product model set, the verifier reads exactly what it read
//! before: Chorus's vocabulary and graphs. Every existing door depends on it.

use chorus_oidc::oidc;

#[test]
fn chorus_doors_still_ask_chorus_graphs() {
    // This process sets no model and no CHORUS_OIDC_* env.
    assert_eq!(oidc::allow_set_graph(), "urn:chorus:domains:security");
    let q = oidc::principal_allow_query();
    assert_eq!(
        q,
        "PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?v WHERE { GRAPH <urn:chorus:domains:security> { ?p a chorus:Principal ; chorus:webId ?v } }"
    );
    assert!(oidc::principal_scope_query().contains("GRAPH <urn:chorus:domains:security>"));
}
