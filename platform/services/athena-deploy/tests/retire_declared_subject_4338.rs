//! #4338 — a staged retirement must not delete a subject the model set declares.
//! The deploy merges the model set into urn:chorus:ontology, then the ledger runs;
//! a staged `retire_subject` naming a declared subject deletes it on every deploy.
//! Ledger line 277 (#4216) did this to chorus:expresses, an owl:ObjectProperty in
//! chorus.ttl, leaving 72 uses of an undeclared property.
use athena_deploy::{model_set, parse_retirement, retirement_action, to_ntriples, RetireAction};

const TBOX: [&str; 6] = [
    "<http://www.w3.org/2002/07/owl#ObjectProperty>", "<http://www.w3.org/2002/07/owl#DatatypeProperty>",
    "<http://www.w3.org/2002/07/owl#AnnotationProperty>", "<http://www.w3.org/1999/02/22-rdf-syntax-ns#Property>",
    "<http://www.w3.org/2002/07/owl#Class>", "<http://www.w3.org/ns/shacl#NodeShape>",
];

/// Staged `retire_subject` lines on urn:chorus:ontology whose subject the model set
/// (`nt`, as N-Triples) declares as a schema term: class, property or shape.
fn retired_but_declared(ledger: &str, nt: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in ledger.lines() {
        let r = parse_retirement(line).unwrap_or_else(|e| panic!("unreadable ledger line: {e}"));
        let Some(r) = r else { continue };
        // the same decision the deploy makes: only a staged line executes
        let RetireAction::Subject { iri, graph } = retirement_action(&r, "urn:chorus:ontology") else { continue };
        if graph != "urn:chorus:ontology" { continue; }
        // a SCHEMA term the model set declares. Instance rows authored in a model
        // file and retired from the ontology on purpose (#4216's moves) are not this.
        let subj = format!("<{iri}> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ");
        let schema = nt.lines().any(|t| t.starts_with(&subj) && TBOX.iter().any(|k| t.contains(k)));
        if schema && !out.contains(&iri) {
            out.push(iri);
        }
    }
    out
}

const NT: &str = "<https://jeffbridwell.com/chorus#expresses> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#ObjectProperty> .\n";

#[test]
fn negative_proof_a_staged_retirement_of_a_declared_subject_is_named() {
    let ledger = r#"{"retire_subject": "https://jeffbridwell.com/chorus#expresses", "graph": "urn:chorus:ontology", "status": "staged"}"#;
    assert_eq!(retired_but_declared(ledger, NT), vec!["https://jeffbridwell.com/chorus#expresses"]);
}

#[test]
fn an_instance_row_moved_out_of_the_ontology_is_not_a_conflict() {
    let ledger = r#"{"retire_subject": "https://jeffbridwell.com/chorus#clearing", "graph": "urn:chorus:ontology", "status": "staged"}"#;
    let nt = "<https://jeffbridwell.com/chorus#clearing> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://jeffbridwell.com/chorus#Product> .\n";
    assert!(retired_but_declared(ledger, nt).is_empty());
}

#[test]
fn a_superseded_line_or_another_graph_is_not_a_conflict() {
    let sup = r#"{"retire_subject": "https://jeffbridwell.com/chorus#expresses", "graph": "urn:chorus:ontology", "status": "superseded"}"#;
    let other = r#"{"retire_subject": "https://jeffbridwell.com/chorus#expresses", "graph": "urn:chorus:instances", "status": "staged"}"#;
    assert!(retired_but_declared(sup, NT).is_empty());
    assert!(retired_but_declared(other, NT).is_empty());
}

#[test]
fn the_shipped_ledger_retires_nothing_the_model_set_declares() {
    let root = format!("{}/../../..", std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let nt: String = model_set(&root, None).iter()
        .map(|p| to_ntriples(p).unwrap_or_else(|e| panic!("{e}"))).collect();
    // a guard whose input vanished must fail, never pass on nothing
    assert!(nt.lines().count() > 1000, "model set read as {} triples", nt.lines().count());
    let ledger = std::fs::read_to_string(format!("{root}/designing/schemas/model-retirements.jsonl")).unwrap();
    assert!(ledger.lines().count() > 100);
    assert_eq!(retired_but_declared(&ledger, &nt), Vec::<String>::new());
}
