// @domain: domains — placed by chorus-crawl's domain rules (#4538)
// @card: #4716
// @owner: kade
//! #4716 — src/lib.rs `RetireAction::Triple`. Hand SPARQL existed because one
//! fact (`chorus hasDomain tests`, urn:chorus:ontology) had no governed way out:
//! a subject retirement would take every triple of `chorus` with it. A triple
//! entry retires exactly one statement, in the graph it names.
use athena_deploy::{parse_retirement, retirement_action, RetireAction};

fn r(line: &str) -> athena_deploy::Retirement {
    parse_retirement(line).expect("parses").expect("not blank")
}
const C: &str = "https://jeffbridwell.com/chorus#";

#[test]
fn a_triple_entry_retires_exactly_one_statement() {
    let e = r(&format!(r#"{{"triple_subject":"{C}chorus","triple_predicate":"{C}hasDomain","triple_object":"{C}tests","graph":"urn:chorus:ontology","status":"staged"}}"#));
    assert_eq!(
        retirement_action(&e, "urn:chorus:ontology"),
        RetireAction::Triple { s: format!("{C}chorus"), p: format!("{C}hasDomain"), o: format!("{C}tests"), graph: "urn:chorus:ontology".into() }
    );
}

// negative proof: a triple entry missing a part must never widen into a subject
// (or any broader) retirement; it is refused as empty.
#[test]
fn negative_proof_a_triple_entry_missing_its_object_never_becomes_a_wider_retirement() {
    let e = r(&format!(r#"{{"triple_subject":"{C}chorus","triple_predicate":"{C}hasDomain","graph":"urn:chorus:ontology","status":"staged"}}"#));
    let a = retirement_action(&e, "urn:chorus:ontology");
    assert!(matches!(a, RetireAction::Triple { ref o, .. } if o.is_empty()), "{a:?}");
}
