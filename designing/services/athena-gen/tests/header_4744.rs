// @domain: domains
// @test-type: unit — model slice read from a TTL fixture; no store, no services
//! #4744 — Jeff 10-26 09:22: "certain elements of the child domains are on the pulse
//! header as a composition from the domains". A field a domain puts on its product's
//! header is marked in the model (`c:inHeader true` on the shape's property); the
//! product's root route serves the sum of those fields over every domain it owns or
//! composes. Nothing about the header is hand-written per domain.
//! Covers src/model (Field.header), src/emit (header, routes).
use athena_gen::emit::{header, routes};
use athena_gen::model::{load, product};
use std::path::PathBuf;

const FIXTURE: &str = r#"
@prefix c: <https://jeffbridwell.com/chorus#> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
c:pulse a c:Product ; c:hasDomain c:awareness ; c:composes c:cards ; c:hasProperty c:gp, c:ns .
c:gp c:propertyKey "store.graph_prefix" ; c:propertyValue "urn:chorus:domains:" .
c:ns c:propertyKey "model.namespace" ; c:propertyValue "https://jeffbridwell.com/chorus#" .
c:clearing a c:Product ; c:hasDomain c:cards .
c:awareness c:definesVocabulary c:RoleState .
c:cards c:definesVocabulary c:Card .
c:RoleStateShape a sh:NodeShape ; sh:targetClass c:RoleState ;
  sh:property [ sh:path c:stateValue ; sh:datatype xsd:string ; c:inHeader true ] .
c:CardShape a sh:NodeShape ; sh:targetClass c:Card ; c:instancesGraph "urn:chorus:domains:cards" ;
  sh:property [ sh:path c:title ; sh:datatype xsd:string ] ;
  sh:property [ sh:path c:status ; sh:datatype xsd:string ; c:inHeader true ] .
"#;

fn fixture(name: &str, ttl: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("athena-gen-4744-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("m.ttl");
    std::fs::write(&f, ttl).unwrap();
    f
}

fn pulse(ttl: &str, name: &str) -> athena_gen::model::Product {
    let (store, _) = load(&[fixture(name, ttl)]).unwrap();
    product(&store, "pulse").unwrap()
}

#[test]
fn the_header_is_the_marked_fields_of_every_owned_and_composed_domain() {
    let p = pulse(FIXTURE, "ok");
    assert_eq!(header(&p), ["awareness.rolestates.stateValue", "cards.cards.status"]);
}

#[test]
fn the_product_root_route_is_served_when_any_domain_puts_a_field_on_the_header() {
    let p = pulse(FIXTURE, "root");
    assert!(routes(&p).contains(&"/pulse".to_string()), "{:?}", routes(&p));
}

/// NEGATIVE PROOF: drop the composition and the cards line leaves the header with no code edit.
#[test]
fn negative_proof_a_domain_pulse_stops_composing_leaves_the_header() {
    let p = pulse(&FIXTURE.replace(" c:composes c:cards ;", ""), "nocards");
    assert_eq!(header(&p), ["awareness.rolestates.stateValue"]);
}

/// NEGATIVE PROOF: a field that is not marked never reaches the header.
#[test]
fn negative_proof_an_unmarked_field_is_not_on_the_header() {
    let p = pulse(&FIXTURE.replace("c:status ; sh:datatype xsd:string ; c:inHeader true", "c:status ; sh:datatype xsd:string"), "unmarked");
    assert_eq!(header(&p), ["awareness.rolestates.stateValue"]);
}

/// With no marked field anywhere there is no header, and no root route pretending to be one.
#[test]
fn no_marked_field_means_no_root_route() {
    let p = pulse(&FIXTURE.replace(" ; c:inHeader true", ""), "none");
    assert!(header(&p).is_empty());
    assert!(!routes(&p).contains(&"/pulse".to_string()));
}
