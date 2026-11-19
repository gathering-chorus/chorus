// @domain: domains
// @test-type: unit — model slice read from TTL fixtures; no store, no services
//! #4995 (AC2) — NEGATIVE: a domain with no classes, or no settings, is refused by name;
//! nothing is generated for it. The control: the same domain with both is built.
//! Covers src/model (domain).
use athena_gen::model::{domain, load};

mod fixture_4995;
use fixture_4995 as unit;

fn refusal(tag: &str, ttl: &str) -> String {
    let (store, _) = load(&[unit::fixture(tag, ttl)]).unwrap();
    domain(&store, "principles").expect_err("must be refused").0
}

#[test]
fn control_the_full_domain_is_built() {
    let (store, _) = load(&[unit::fixture("refused-control", unit::FIXTURE)]).unwrap();
    assert!(domain(&store, "principles").is_ok());
}

#[test]
fn a_domain_with_no_classes_is_refused_by_name() {
    let ttl = unit::FIXTURE.replace("c:definesVocabulary c:Principle ;", "");
    let e = refusal("no-classes", &ttl);
    assert!(e.contains("'principles' defines no classes"), "{e}");
}

#[test]
fn a_domain_with_no_settings_is_refused_by_name() {
    let start = unit::FIXTURE.find("  c:hasProperty").unwrap();
    let end = unit::FIXTURE[start..].find(" .\n").unwrap() + start;
    let ttl = format!("{} .\n{}", &unit::FIXTURE[..start].trim_end_matches([' ', ';', '\n']), &unit::FIXTURE[end + 3..]);
    let e = refusal("no-settings", &ttl);
    assert!(e.contains("'principles' declares no settings"), "{e}");
}

#[test]
fn a_domain_not_in_the_model_is_refused_by_name() {
    let (store, _) = load(&[unit::fixture("refused-absent", unit::FIXTURE)]).unwrap();
    let e = domain(&store, "nosuch").expect_err("must be refused").0;
    assert!(e.contains("'nosuch' is not in the model"), "{e}");
}
