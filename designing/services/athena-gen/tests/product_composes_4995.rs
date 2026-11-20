// @domain: domains
// @test-type: unit — model slices read from TTL fixtures and the repo's model; no store, no services
//! #4995 (AC4) — Jeff 11-17: "it shouldn't be doing both interchangeably". A product's API
//! is a composition of its domains: `--product` names which domains it owns and composes
//! and holds no class or shape logic of its own. One path builds a domain, whichever unit
//! asks. Pulse, built by composition, serves exactly the routes its committed crate serves.
//! NEGATIVE PROOF: a product row that defines a class itself is refused by name.
//! Covers src/model (product, domain, domain_slice).
use athena_gen::model::{domain, domain_slice, load, product, settings};

mod fixture_4995;
use fixture_4995::{fixture, FIXTURE};

/// Loom owns principles; both carry the same settings, so the same domain built for
/// either unit must come out identical.
fn with_loom(extra: &str) -> String {
    format!("{FIXTURE}\n@prefix c: <https://jeffbridwell.com/chorus#> .\n\
             c:loom a c:Product ; c:hasDomain c:principles ;\n  \
             c:hasProperty c:p-bind, c:p-gp, c:p-ns, c:p-pfx, c:p-kind, c:p-ep, c:p-path, c:p-uv, c:p-pv, c:p-iss, c:p-igp, c:p-ev .\n{extra}")
}

#[test]
fn a_domain_is_the_same_whichever_unit_builds_it() {
    let (store, _) = load(&[fixture("compose-same", &with_loom(""))]).unwrap();
    let by_product = product(&store, "loom").unwrap();
    let by_domain = domain(&store, "principles").unwrap();
    assert_eq!(by_product.domains, by_domain.domains, "one path builds the domain");
}

#[test]
fn every_pulse_domain_is_its_domain_slice() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let (store, _) = load(&athena_gen::model::model_files(&root)).unwrap();
    let p = product(&store, "pulse").unwrap();
    let s = settings(&store, "pulse").unwrap();
    assert!(!p.domains.is_empty());
    for d in &p.domains {
        assert_eq!(d, &domain_slice(&store, &d.name, d.composed, "pulse", &s).unwrap(), "{} is not its domain slice", d.name);
    }
}

#[test]
fn pulse_by_composition_serves_the_routes_its_crate_serves() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let (store, _) = load(&athena_gen::model::model_files(&root)).unwrap();
    // the committed model.rs lists class routes (operations live in ops.rs)
    let committed = std::fs::read_to_string(root.join("directing/products/pulse/api/src/model.rs")).unwrap();
    let mut was = athena_gen::emit::old_routes(&committed, "pulse");
    let p = product(&store, "pulse").unwrap();
    let mut now = athena_gen::emit::routes(&athena_gen::model::Product { operations: Vec::new(), ..p });
    was.sort();
    now.sort();
    assert!(!was.is_empty());
    assert_eq!(now, was);
}

#[test]
fn a_product_that_defines_a_class_itself_is_refused_by_name() {
    let (store, _) = load(&[fixture("compose-rogue", &with_loom("c:loom c:definesVocabulary c:Rogue .\n"))]).unwrap();
    let e = product(&store, "loom").expect_err("a product holds no classes").0;
    assert!(e.contains("loom defines Rogue itself"), "{e}");
}
