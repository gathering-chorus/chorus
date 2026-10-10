<<<<<<< HEAD
<<<<<<< HEAD
// @domain: domains
//! #4490 — the generator reads one product's slice of the model from TTL alone.
use athena_gen::model::{load, missing_settings, product, segment, settings, Kind};
=======
=======
// @domain: domains
>>>>>>> 3795eee59 (wren: #4490 — athena-gen generate writes the pulse-api crate (RowStore trait, remote SPARQL backend, settings env then file then default, versioned envelope); pulse-api compiles and serves the 5 pulse collections read-only on :3392; emitter tests incl. same-model-same-bytes and a model-change negative proof)
//! #4490 — the generator reads one product's slice of the model from TTL alone.
use athena_gen::model::{load, product, segment, Kind};
>>>>>>> a0f8b1ded (wren: #4490 — athena-gen reads a product's slice of the model from TTL (in-memory Oxigraph, no Fuseki): domains, classes, fields, sh:in in order, v1 path segments; 4 tests incl. 2 negative proofs)
use std::path::PathBuf;

const FIXTURE: &str = r#"
@prefix c: <https://jeffbridwell.com/chorus#> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
c:pulse a c:Product ; c:hasDomain c:awareness, c:streams .
c:awareness c:definesVocabulary c:RoleState .
c:streams c:definesVocabulary c:StreamEvent .
c:RoleStateShape a sh:NodeShape ; sh:targetClass c:RoleState ;
  c:instancesGraph "urn:chorus:domains:awareness" ;
  sh:property [ sh:path c:stateValue ; sh:minCount 1 ; sh:maxCount 1 ; sh:datatype xsd:string ;
                sh:in ( "idle" "building" "waiting" "blocked" ) ] ;
  sh:property [ sh:path c:ownedBy ; sh:minCount 1 ; sh:maxCount 1 ; sh:nodeKind sh:IRI ; sh:class c:Principal ] .
c:StreamEventShape a sh:NodeShape ; sh:targetClass c:StreamEvent ;
  sh:property [ sh:path c:streamTs ; sh:datatype xsd:dateTime ] .
"#;

fn fixture(name: &str, ttl: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("athena-gen-4490-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("m.ttl");
    std::fs::write(&f, ttl).unwrap();
    f
}

#[test]
fn reads_domains_classes_fields_in_model_order() {
    let (store, skipped) = load(&[fixture("ok", FIXTURE)]).unwrap();
    assert!(skipped.is_empty(), "{skipped:?}");
    let p = product(&store, "pulse").unwrap();
    let names: Vec<_> = p.domains.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names, ["awareness", "streams"]);
    let rs = &p.domains[0].classes[0];
    assert_eq!((rs.name.as_str(), rs.segment.as_str(), rs.graph.as_str()), ("RoleState", "rolestates", "urn:chorus:domains:awareness"));
    let sv = rs.fields.iter().find(|f| f.name == "stateValue").unwrap();
    assert_eq!(sv.allowed, ["idle", "building", "waiting", "blocked"], "sh:in must keep the model's order");
    assert!(sv.required && !sv.many);
    let owner = rs.fields.iter().find(|f| f.name == "ownedBy").unwrap();
    assert_eq!(owner.kind, Kind::Link { class: Some("Principal".into()) });
<<<<<<< HEAD
<<<<<<< HEAD
    // a class with no instancesGraph lives in its domain's graph; an uncapped LITERAL is
    // served as one value (v1's rule), only an uncapped link is many
    let ev = &p.domains[1].classes[0];
    assert_eq!((ev.segment.as_str(), ev.graph.as_str()), ("events", "urn:chorus:domains:streams"));
    assert!(!ev.fields[0].many && !ev.fields[0].required);
=======
    // a class with no instancesGraph lives in its domain's graph; no maxCount means many
    let ev = &p.domains[1].classes[0];
    assert_eq!((ev.segment.as_str(), ev.graph.as_str()), ("events", "urn:chorus:domains:streams"));
    assert!(ev.fields[0].many && !ev.fields[0].required);
>>>>>>> a0f8b1ded (wren: #4490 — athena-gen reads a product's slice of the model from TTL (in-memory Oxigraph, no Fuseki): domains, classes, fields, sh:in in order, v1 path segments; 4 tests incl. 2 negative proofs)
=======
    // a class with no instancesGraph lives in its domain's graph; an uncapped LITERAL is
    // served as one value (v1's rule), only an uncapped link is many
    let ev = &p.domains[1].classes[0];
    assert_eq!((ev.segment.as_str(), ev.graph.as_str()), ("events", "urn:chorus:domains:streams"));
    assert!(!ev.fields[0].many && !ev.fields[0].required);
>>>>>>> 3795eee59 (wren: #4490 — athena-gen generate writes the pulse-api crate (RowStore trait, remote SPARQL backend, settings env then file then default, versioned envelope); pulse-api compiles and serves the 5 pulse collections read-only on :3392; emitter tests incl. same-model-same-bytes and a model-change negative proof)
}

#[test]
fn paths_match_v1_segments() {
    assert_eq!(segment("streams", "StreamEvent"), "events");
    assert_eq!(segment("awareness", "WaitingOn"), "waitingons");
    assert_eq!(segment("tests", "TestResult"), "results");
    assert_eq!(segment("tests", "Test"), "tests");
    assert_eq!(segment("value-streams", "ValueStreamStep"), "steps");
}

#[test]
fn negative_proof_a_product_with_no_domains_is_refused_not_empty() {
    let (store, _) = load(&[fixture("none", "@prefix c: <https://jeffbridwell.com/chorus#> . c:ghost a c:Product .")]).unwrap();
    let e = product(&store, "ghost").unwrap_err();
    assert!(e.0.contains("has no domains"), "{e}");
}

#[test]
fn negative_proof_a_file_that_does_not_parse_is_named_never_half_loaded() {
    let (_, skipped) = load(&[fixture("bad", "this is not turtle .")]).unwrap();
    assert_eq!(skipped.len(), 1);
    assert!(skipped[0].contains("m.ttl"));
}
<<<<<<< HEAD

#[test]
fn a_class_serves_its_parents_fields_as_v1_does() {
    let ttl = r#"
@prefix c: <https://jeffbridwell.com/chorus#> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
c:pulse a c:Product ; c:hasDomain c:awareness .
c:awareness c:definesVocabulary c:RoleState .
c:RoleState rdfs:subClassOf c:Memory .
c:MemoryShape sh:targetClass c:Memory ; sh:property [ sh:path c:comment ; sh:maxCount 1 ] .
c:RoleStateShape sh:targetClass c:RoleState ; sh:property [ sh:path c:stateValue ; sh:maxCount 1 ] .
"#;
    let (store, _) = load(&[fixture("inherit", ttl)]).unwrap();
    let p = product(&store, "pulse").unwrap();
    let names: Vec<_> = p.domains[0].classes[0].fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["comment", "stateValue"], "the parent's field is served too");
}

#[test]
fn settings_come_from_the_products_property_rows() {
    let ttl = r#"
@prefix c: <https://jeffbridwell.com/chorus#> .
c:pulse a c:Product ; c:hasProperty c:p1, c:p2 .
c:p1 c:propertyKey "store.kind" ; c:propertyValue "sparql" .
c:p2 c:propertyKey "server.bind" ; c:propertyValue "127.0.0.1:3392" .
"#;
    let (store, _) = load(&[fixture("settings", ttl)]).unwrap();
    let s = settings(&store, "pulse").unwrap();
    assert_eq!(s, [("server.bind".to_string(), "127.0.0.1:3392".to_string()), ("store.kind".into(), "sparql".into())]);
    // NEGATIVE PROOF: a product missing runtime settings is named, not defaulted
    let missing = missing_settings(&s);
    assert!(missing.contains(&"identity.issuer") && missing.contains(&"store.endpoint"), "{missing:?}");
}
=======
>>>>>>> a0f8b1ded (wren: #4490 — athena-gen reads a product's slice of the model from TTL (in-memory Oxigraph, no Fuseki): domains, classes, fields, sh:in in order, v1 path segments; 4 tests incl. 2 negative proofs)
