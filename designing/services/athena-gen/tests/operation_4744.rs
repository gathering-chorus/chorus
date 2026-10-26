// @domain: domains
// @test-type: unit — model slice read from a TTL fixture; no store, no services
//! #4744 — Borg Queen's brief, Jeff 10-26: an analytical operation (a live computation,
//! not stored rows) is declared in the model as an Operation row: its product, its home
//! domain, its route segment and its handler. athena-gen routes it and compiles the
//! handler in; an operation homed outside the product, or a handler that is not in the
//! repo, is refused by name, never generated as a hollow route.
//! Covers src/model (Operation), src/emit (routes, handler_path, ops_rs).
use athena_gen::emit::{handler_path, ops_rs, routes};
use athena_gen::model::{load, product};
use std::path::PathBuf;

const FIXTURE: &str = r#"
@prefix c: <https://jeffbridwell.com/chorus#> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
c:pulse a c:Product ; c:hasDomain c:awareness ; c:hasProperty c:gp, c:ns ; c:hasOperation c:tree .
c:gp c:propertyKey "store.graph_prefix" ; c:propertyValue "urn:chorus:domains:" .
c:ns c:propertyKey "model.namespace" ; c:propertyValue "https://jeffbridwell.com/chorus#" .
c:awareness c:definesVocabulary c:RoleState .
c:RoleStateShape a sh:NodeShape ; sh:targetClass c:RoleState ;
  sh:property [ sh:path c:stateValue ; sh:datatype xsd:string ] .
c:tree a c:Operation ; c:homeDomain c:awareness ; c:operationPath "tree" ; c:handlerPath "ops/tree.rs" .
"#;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("athena-gen-op-4744-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn pulse(ttl: &str, name: &str) -> Result<athena_gen::model::Product, String> {
    let f = dir(name).join("m.ttl");
    std::fs::write(&f, ttl).unwrap();
    let (store, _) = load(&[f]).unwrap();
    product(&store, "pulse").map_err(|e| e.0)
}

#[test]
fn an_operation_row_is_a_route_of_its_product() {
    let p = pulse(FIXTURE, "ok").unwrap();
    assert_eq!(p.operations.len(), 1);
    assert_eq!((p.operations[0].name.as_str(), p.operations[0].domain.as_str()), ("tree", "awareness"));
    assert!(routes(&p).contains(&"/pulse/awareness/tree".to_string()), "{:?}", routes(&p));
}

/// NEGATIVE PROOF: an operation homed in a domain the product neither owns nor composes is refused by name.
#[test]
fn negative_proof_an_operation_outside_the_product_is_refused() {
    let err = pulse(&FIXTURE.replace("c:homeDomain c:awareness", "c:homeDomain c:cards"), "outside").unwrap_err();
    assert!(err.contains("tree") && err.contains("cards") && err.contains("neither owns nor composes"), "{err}");
}

/// NEGATIVE PROOF: a handler that is not in the repo is refused by name; one that is resolves from the crate's src/.
#[test]
fn negative_proof_a_missing_handler_is_refused_and_a_present_one_is_compiled_in() {
    let p = pulse(FIXTURE, "handler").unwrap();
    let root = dir("repo");
    let out = root.join("directing/products/pulse/api");
    let err = handler_path(&root, &out, &p.operations[0]).unwrap_err();
    assert!(err.contains("tree") && err.contains("ops/tree.rs") && err.contains("does not exist"), "{err}");
    std::fs::create_dir_all(root.join("ops")).unwrap();
    std::fs::write(root.join("ops/tree.rs"), "").unwrap();
    let rel = handler_path(&root, &out, &p.operations[0]).unwrap();
    assert_eq!(rel, "../../../../../ops/tree.rs");
    let ops = ops_rs(&p, &[("tree".into(), rel)]);
    assert!(ops.contains("#[path = \"../../../../../ops/tree.rs\"]\npub mod tree;"), "{ops}");
    assert!(ops.contains("(\"awareness\", \"tree\") => Some(tree::run(ctx, rest, query))"), "{ops}");
}

/// A product with no operation still gets an ops.rs that routes nothing.
#[test]
fn no_operation_routes_nothing() {
    let p = pulse(&FIXTURE.replace(" ; c:hasOperation c:tree", ""), "none").unwrap();
    assert!(p.operations.is_empty());
    let ops = ops_rs(&p, &[]);
    assert!(!ops.contains("pub mod"), "{ops}");
    assert!(ops.contains("let ops: &[(&str, &str)] = &[];"), "{ops}");
}
