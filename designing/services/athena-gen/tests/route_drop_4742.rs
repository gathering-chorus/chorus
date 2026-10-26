// @domain: domains
// @test-type: unit — pure functions over generated model.rs text; no store, no services
//! #4742 gap 8 (#4708: a shape change 404'd /v1/domains) — a class that falls out of the
//! Covers src/emit (routes, dropped_routes) and src/main (generate refuses a silent drop).
//! model must not take its generated route with it silently. Regenerating over a crate
//! names every route the old crate served and the new one would not; generate refuses
//! unless each is named with --drop.
use athena_gen::emit::{dropped_routes, model_rs, old_routes, routes};
use athena_gen::model::{Class, Domain, Product};

fn product(classes: &[(&str, &str)]) -> Product {
    let mut domains: Vec<Domain> = Vec::new();
    for (d, seg) in classes {
        if !domains.iter().any(|x| x.name == *d) {
            domains.push(Domain { name: d.to_string(), composed: false, classes: Vec::new() });
        }
        let dom = domains.iter_mut().find(|x| x.name == *d).unwrap();
        dom.classes.push(Class { name: seg.to_string(), segment: seg.to_string(), graph: format!("urn:g:{d}"), fields: Vec::new() });
    }
    Product { name: "pulse".into(), domains }
}

const OLD: &str = r#"pub const CLASSES: &[ClassDef] = &[
    ClassDef {
        domain: "awareness", name: "Turn", kind: "turn", plural: "Turns", segment: "turns", graph: "urn:g", composed: false,
        fields: &[],
    },
    ClassDef {
        domain: "streams", name: "StreamEvent", kind: "stream-event", plural: "StreamEvents", segment: "events", graph: "urn:g",
        fields: &[],
    },
];"#;

#[test]
fn routes_are_product_domain_collection() {
    assert_eq!(routes(&product(&[("awareness", "turns")])), ["/pulse/awareness/turns"]);
}

#[test]
fn a_route_the_new_model_still_serves_is_not_dropped() {
    let p = product(&[("awareness", "turns"), ("streams", "events"), ("cards", "cards")]);
    assert!(dropped_routes(OLD, &p).is_empty());
}

/// NEGATIVE PROOF: streams falls out of the model, and its route is named as dropped.
#[test]
fn negative_proof_a_class_that_left_the_model_is_named() {
    let p = product(&[("awareness", "turns")]);
    assert_eq!(dropped_routes(OLD, &p), ["/pulse/streams/events"]);
}

/// The old crate's routes before #4742 (version first, no composed flag) read the same way.
#[test]
fn an_older_crate_is_read_too() {
    let old = OLD.replace(", composed: false", "");
    assert_eq!(dropped_routes(&old, &product(&[("awareness", "turns")])), ["/pulse/streams/events"]);
}

/// Kade + Abby review 10-25: the scanner reads the generator's own output, so it is proven on
/// that output — a change to how model.rs is written breaks this test instead of emptying the guard.
#[test]
fn the_scanner_reads_what_the_generator_writes_today() {
    let both = product(&[("awareness", "turns"), ("streams", "events")]);
    let written = model_rs(&both, "1", &[]);
    assert_eq!(old_routes(&written, "pulse"), routes(&both));
    assert_eq!(dropped_routes(&written, &product(&[("awareness", "turns")])), ["/pulse/streams/events"]);
}

/// NEGATIVE PROOF (#3734: a guard whose target moved must fail loudly): a model.rs the
/// scanner cannot read yields no routes, and generate refuses on that (see src/main).
#[test]
fn negative_proof_an_unreadable_model_rs_yields_no_routes() {
    assert!(old_routes("pub const CLASSES: &[ClassDef] = &[ ClassDef { domain = awareness } ];", "pulse").is_empty());
}
