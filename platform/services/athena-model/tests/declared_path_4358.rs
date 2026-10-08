//! #4358 — a door write puts each field at the path its shape names.
//!
//! The door wrote every field at chorus:<name>. PrincipleShape's comment field
//! names rdfs:comment, so the 14 principle rewrites on 2026-10-07 left every row
//! without the property its own shape requires (nightly principles-graph: 0 of
//! 14). Each test runs the same write with and without the declared path, so
//! the check can tell the two states apart (#3734).
//!
//! Hermetic: the stub answers shape SELECTs by query content; no live Fuseki.

use athena_model::{set_field, verify_identity, write, Identity, Store, WriteReq, R};
use std::cell::RefCell;

const RDFS_COMMENT: &str = "http://www.w3.org/2000/01/rdf-schema#comment";

struct Stub {
    /// rows for the declared-field-paths query: "comment|<iri>"
    paths: Vec<String>,
    updates: RefCell<Vec<String>>,
}

impl Store for Stub {
    fn ask(&self, sparql: &str) -> R<bool> {
        // identity is not the variable here; the row exists for set_field
        Ok(sparql.contains("urn:chorus:domains:security") || sparql.contains("ASK { GRAPH"))
    }
    fn select_v(&self, sparql: &str) -> R<Vec<String>> {
        if sparql.contains("# athena-model declared field paths") {
            Ok(self.paths.clone())
        } else if sparql.contains("definesVocabulary") {
            Ok(vec!["tests".to_string()])
        } else {
            Ok(vec![])
        }
    }
    fn update(&self, s: &str) -> R<()> {
        self.updates.borrow_mut().push(s.to_string());
        Ok(())
    }
}

fn stub(declared: bool) -> Stub {
    let paths = if declared { vec![format!("comment|{RDFS_COMMENT}")] } else { vec![] };
    Stub { paths, updates: RefCell::new(vec![]) }
}

fn vid(s: &Stub) -> Identity {
    verify_identity(Some("wren"), s).unwrap()
}

fn req() -> WriteReq {
    let mut req = WriteReq { kind: "domain".into(), name: "x".into(), ..Default::default() };
    req.fields.insert("comment".into(), "What the principle says.".into());
    req
}

#[test]
fn write_puts_comment_at_the_declared_rdfs_comment() {
    let s = stub(true);
    write(&s, &req(), &vid(&s)).unwrap();
    let u = s.updates.borrow().join("\n");
    assert!(u.contains(&format!("<{RDFS_COMMENT}> \"What the principle says.\"")), "{u}");
    assert!(u.contains("<https://jeffbridwell.com/chorus#comment> \"What the principle says.\""), "chorus:comment kept for readers: {u}");
}

/// NEGATIVE PROOF: a shape that declares no outside path gets no rdfs:comment.
#[test]
fn negative_proof_no_declared_path_writes_no_rdfs_comment() {
    let s = stub(false);
    write(&s, &req(), &vid(&s)).unwrap();
    assert!(!s.updates.borrow().join("\n").contains(RDFS_COMMENT));
}

#[test]
fn set_field_replaces_the_declared_path_too() {
    let s = stub(true);
    let _ = set_field(&s, "domain", "x", "comment", "New words.", None, &vid(&s));
    let u = s.updates.borrow().join("\n");
    assert!(u.contains("DELETE WHERE { GRAPH") && u.contains(&format!("<{RDFS_COMMENT}> ?o")), "{u}");
    assert!(u.contains(&format!("<{RDFS_COMMENT}> \"New words.\"")), "{u}");
}

/// A declared path that is not a clean IRI is ignored, never spliced into SPARQL.
#[test]
fn a_malformed_declared_path_is_ignored() {
    let mut s = stub(false);
    s.paths = vec!["comment|http://x> } ; DROP ALL ; <y".into()];
    write(&s, &req(), &vid(&s)).unwrap();
    assert!(!s.updates.borrow().join("\n").contains("DROP ALL"));
}
