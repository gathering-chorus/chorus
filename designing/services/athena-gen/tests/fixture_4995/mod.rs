// @domain: domains
//! #4995 — the principles domain fixture the #4995 tests share.
#![allow(dead_code)]
use std::path::PathBuf;

/// The principles domain with its own settings and NO product row naming it.
pub const FIXTURE: &str = r#"
@prefix c: <https://jeffbridwell.com/chorus#> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
c:principles a c:Domain ; c:definesVocabulary c:Principle ;
  c:hasProperty c:p-bind, c:p-gp, c:p-ns, c:p-pfx, c:p-kind, c:p-ep, c:p-path, c:p-uv, c:p-pv, c:p-iss, c:p-igp, c:p-ev .
c:p-bind c:propertyKey "server.bind" ; c:propertyValue "127.0.0.1:0" .
c:p-gp   c:propertyKey "store.graph_prefix" ; c:propertyValue "urn:chorus:domains:" .
c:p-ns   c:propertyKey "model.namespace" ; c:propertyValue "https://jeffbridwell.com/chorus#" .
c:p-pfx  c:propertyKey "model.prefix" ; c:propertyValue "chorus" .
c:p-kind c:propertyKey "store.kind" ; c:propertyValue "sparql" .
c:p-ep   c:propertyKey "store.endpoint" ; c:propertyValue "http://localhost:3030/pods" .
c:p-path c:propertyKey "store.path" ; c:propertyValue "" .
c:p-uv   c:propertyKey "store.user_var" ; c:propertyValue "FUSEKI_ADMIN_USER" .
c:p-pv   c:propertyKey "store.password_var" ; c:propertyValue "FUSEKI_ADMIN_PASSWORD" .
c:p-iss  c:propertyKey "identity.issuer" ; c:propertyValue "https://id.example/" .
c:p-igp  c:propertyKey "identity.graph_prefix" ; c:propertyValue "urn:chorus:domains:" .
c:p-ev   c:propertyKey "events.command" ; c:propertyValue "" .
c:values a c:Domain ; c:definesVocabulary c:Value .
c:PrincipleShape a sh:NodeShape ; sh:targetClass c:Principle ; c:instancesGraph "urn:chorus:domains:principles" ;
  sh:property [ sh:path c:statement ; sh:datatype xsd:string ; sh:minCount 1 ] .
c:ValueShape a sh:NodeShape ; sh:targetClass c:Value ;
  sh:property [ sh:path c:title ; sh:datatype xsd:string ] .
"#;

pub fn fixture(tag: &str, ttl: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("athena-gen-4995-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("m.ttl");
    std::fs::write(&f, ttl).unwrap();
    f
}

