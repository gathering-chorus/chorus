//! #3506 / ADR-047 — LIVE proof: the envelope wraps a REAL primitive read against
//! the running Fuseki, not just a unit fixture. Marked `#[ignore]` so the default
//! suite stays hermetic; run with `cargo test --test live_envelope -- --ignored`
//! against Fuseki on localhost:3030/pods. This is the ADR's prove-one-first bar:
//! one primitive, real data, through the contract.

use owl_api::{generate, handle};

#[test]
#[ignore]
fn live_envelope_wraps_real_domain_cards() {
    // generate the Domain RouteTable from the live ontology shape
    let table = generate("Domain").expect("DomainShape must be in urn:chorus:ontology");
    // hit the real read path → entity_json → live SPARQL → envelope
    let (code, body) = handle("/domains/cards", &table);
    assert_eq!(code, 200, "GET /domains/cards should be 200; got {}: {}", code, body);

    // the ADR-047 envelope, around REAL data
    for needle in [
        "\"apiVersion\": \"v1\"",
        "\"kind\": \"Domain\"",
        "\"id\": \"chorus:cards\"",
        "\"self\": \"/v1/domains/cards\"",
        "\"generatedFrom\":",
        "\"graph\": \"urn:chorus:ontology\"",
        "\"shape\": \"chorus:DomainShape\"",
        "\"data\":",
        "\"links\":",
        "\"requiresAuth\":",
        "\"deprecation\": null",
    ] {
        assert!(body.contains(needle), "live envelope missing `{}`:\n{}", needle, body);
    }
    // and the data slot carries the cards domain's REAL fields (not a fixture)
    assert!(body.contains("\"creator\""), "data must carry real entity fields:\n{}", body);
    assert!(body.contains("\"iri\""), "data carries the entity iri:\n{}", body);
}
