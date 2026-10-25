// @domain: domains
// @test-type: unit — model slice read from a TTL fixture; no store, no services
//! #4742 — Jeff 10-25: "the top level route is /pulse and each associated domain is a
//! Covers src/model (product reads composes), src/emit (ClassDef composed), templates/main (405 composed),
//! templates/rules (canonical decimal), templates/requirements_tests.
//! child route"; the children are the domains the product owns (hasDomain) and the
//! domains it composes (composes), and the top level is a composition: a composed
//! domain is served read-only from its owner's graph, never written through the product.
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
  sh:property [ sh:path c:stateValue ; sh:datatype xsd:string ] .
c:CardShape a sh:NodeShape ; sh:targetClass c:Card ; c:instancesGraph "urn:chorus:domains:cards" ;
  sh:property [ sh:path c:title ; sh:datatype xsd:string ] .
"#;

fn fixture(name: &str, ttl: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("athena-gen-4742-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("m.ttl");
    std::fs::write(&f, ttl).unwrap();
    f
}

#[test]
fn a_composed_domain_is_a_child_of_the_product_and_marked_composed() {
    let (store, _) = load(&[fixture("ok", FIXTURE)]).unwrap();
    let p = product(&store, "pulse").unwrap();
    let got: Vec<_> = p.domains.iter().map(|d| (d.name.as_str(), d.composed)).collect();
    assert_eq!(got, [("awareness", false), ("cards", true)]);
    // read from the owner's graph, the same rows the owner serves
    assert_eq!(p.domains[1].classes[0].graph, "urn:chorus:domains:cards");
}

/// NEGATIVE PROOF: without the composes triple, cards is not a child of pulse.
#[test]
fn negative_proof_a_domain_pulse_does_not_compose_is_not_a_child() {
    let ttl = FIXTURE.replace(" c:composes c:cards ;", "");
    let (store, _) = load(&[fixture("none", &ttl)]).unwrap();
    let p = product(&store, "pulse").unwrap();
    let names: Vec<_> = p.domains.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names, ["awareness"]);
}

/// A domain both owned and composed is owned: one home, never served twice.
#[test]
fn an_owned_domain_listed_as_composed_too_is_served_once_as_owned() {
    let ttl = FIXTURE.replace("c:composes c:cards", "c:composes c:cards, c:awareness");
    let (store, _) = load(&[fixture("both", &ttl)]).unwrap();
    let p = product(&store, "pulse").unwrap();
    let got: Vec<_> = p.domains.iter().map(|d| (d.name.as_str(), d.composed)).collect();
    assert_eq!(got, [("awareness", false), ("cards", true)]);
}
