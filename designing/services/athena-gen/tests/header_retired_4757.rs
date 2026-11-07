// @domain: domains
// @test-type: unit — model slice read from a TTL fixture; no store, no services
//! #4757 — the generated root header is retired: pulse serves its header from its sources
//! (:3475/pulse, Jeff 10-26 "we already have messages api"), so a generated crate never
//! serves /<product> again, even if an old c:inHeader mark is left in a model.
//! Covers src/emit (routes), src/model (Field).
use athena_gen::emit::routes;
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
    let dir = std::env::temp_dir().join(format!("athena-gen-4757-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("m.ttl");
    std::fs::write(&f, ttl).unwrap();
    f
}

fn pulse(ttl: &str, name: &str) -> athena_gen::model::Product {
    let (store, _) = load(&[fixture(name, ttl)]).unwrap();
    product(&store, "pulse").unwrap()
}

/// The collections are still served; the root route is not, marks or no marks.
#[test]
fn a_marked_model_serves_its_collections_and_no_root_header() {
    let r = routes(&pulse(FIXTURE, "marked"));
    assert!(r.contains(&"/pulse/awareness/rolestates".to_string()) && r.contains(&"/pulse/cards/cards".to_string()), "{r:?}");
    assert!(!r.contains(&"/pulse".to_string()), "{r:?}");
}

/// NEGATIVE PROOF: the routes are read from this model, not a fixed list — drop the
/// composition and the cards collection leaves.
#[test]
fn negative_proof_routes_follow_the_model() {
    let r = routes(&pulse(&FIXTURE.replace(" ; c:composes c:cards", ""), "nocompose"));
    assert!(!r.contains(&"/pulse/cards/cards".to_string()), "{r:?}");
}
