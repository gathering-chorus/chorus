//! #3467 (B scope) — the generated write surface must be constraint-SAFE, not just
//! well-formed. The DAL (chorus-model) is the one write authority, so datatype +
//! edge-target-type enforcement lives here, derived from the shape (no hardcoded
//! per-domain rules). Red-first (DEC-1674). This file pins the PURE datatype check;
//! edge-target-type enforcement is integration-proven (needs the store).

use chorus_model::{datatype_ok, verify_identity, write, Identity, Store, WriteReq, R};
use std::cell::RefCell;

/// Configurable stub: answers shape SELECTs by query content (minCount→required,
/// sh:datatype→datatypes, sh:class→edge_classes) and ASKs by an existence set +
/// a typed set (so we can distinguish "exists" from "is a Product").
struct Cfg {
    required: Vec<String>,
    datatypes: Vec<(String, String)>,   // path|xsd rows
    edge_classes: Vec<(String, String)>, // path|class rows
    exists: Vec<String>,                 // IRIs that exist
    typed: Vec<String>,                  // "<iri> a <ns#Class>" assertions present
    updates: RefCell<Vec<String>>,
}
impl Store for Cfg {
    fn ask(&self, sparql: &str) -> R<bool> {
        if sparql.contains("urn:chorus:domains:security") {
            return Ok(true); // identity is not the variable in these tests (see identity_gate.rs)
        }
        if sparql.contains(" a <") {
            // type ASK — true only if a matching typed assertion is present
            Ok(self.typed.iter().any(|t| sparql.contains(t)))
        } else {
            Ok(self.exists.iter().any(|e| sparql.contains(e.as_str())))
        }
    }
    fn select_v(&self, sparql: &str) -> R<Vec<String>> {
        if sparql.contains("sh:minCount") {
            Ok(self.required.clone())
        } else if sparql.contains("sh:datatype") {
            Ok(self.datatypes.iter().map(|(p, d)| format!("{}|{}", p, d)).collect())
        } else if sparql.contains("sh:class") {
            Ok(self.edge_classes.iter().map(|(p, c)| format!("{}|{}", p, c)).collect())
        } else {
            Ok(vec![])
        }
    }
    fn update(&self, s: &str) -> R<()> {
        self.updates.borrow_mut().push(s.to_string());
        Ok(())
    }
}
/// #3651 — mutating verbs need a verified Identity; these tests verify one
/// against the permissive registry route above.
fn vid(s: &Cfg) -> Identity { verify_identity(Some("kade"), s).unwrap() }

fn cfg() -> Cfg {
    Cfg { required: vec![], datatypes: vec![], edge_classes: vec![], exists: vec![], typed: vec![], updates: RefCell::new(vec![]) }
}

const NS: &str = "https://jeffbridwell.com/chorus#";

#[test]
fn write_rejects_wrong_datatype_value() {
    // shape says `port` is xsd:integer; a non-numeric value is refused, nothing written.
    let mut s = cfg();
    s.datatypes = vec![("port".into(), "integer".into())];
    let mut req = WriteReq { kind: "domain".into(), name: "x".into(), ..Default::default() };
    req.fields.insert("port".into(), "not-a-number".into());
    let e = write(&s, &req, &vid(&s)).unwrap_err();
    assert!(e.starts_with("shape-violation") && e.contains("xsd:integer"), "{}", e);
    assert!(s.updates.borrow().is_empty(), "nothing written on a datatype violation");
}

#[test]
fn write_accepts_valid_datatype_value() {
    let mut s = cfg();
    s.datatypes = vec![("port".into(), "integer".into())];
    let mut req = WriteReq { kind: "domain".into(), name: "x".into(), ..Default::default() };
    req.fields.insert("port".into(), "8080".into());
    assert!(write(&s, &req, &vid(&s)).is_ok(), "a valid integer passes");
}

#[test]
fn write_rejects_edge_target_of_wrong_type() {
    // partOf must point at a Product; the target EXISTS but is not typed Product → refused.
    let target = format!("{}athena", NS);
    let mut s = cfg();
    s.edge_classes = vec![("partOf".into(), "Product".into())];
    s.exists = vec![target.clone()];          // it exists…
    s.typed = vec![];                          // …but no "a <#Product>" assertion
    let req = WriteReq {
        kind: "domain".into(),
        name: "x".into(),
        edges: vec![("partOf".into(), "product".into(), "athena".into())],
        ..Default::default()
    };
    let e = write(&s, &req, &vid(&s)).unwrap_err();
    assert!(e.starts_with("shape-violation") && e.contains("is not a Product"), "{}", e);
    assert!(s.updates.borrow().is_empty(), "nothing written on edge-target-type violation");
}

#[test]
fn write_accepts_edge_target_of_right_type() {
    let target = format!("{}athena", NS);
    let mut s = cfg();
    s.edge_classes = vec![("partOf".into(), "Product".into())];
    s.exists = vec![target.clone()];
    s.typed = vec![format!("<{}> a <{}Product>", target, NS)]; // correctly typed
    let req = WriteReq {
        kind: "domain".into(),
        name: "x".into(),
        edges: vec![("partOf".into(), "product".into(), "athena".into())],
        ..Default::default()
    };
    assert!(write(&s, &req, &vid(&s)).is_ok(), "a correctly-typed edge target passes");
}

#[test]
fn datatype_ok_enforces_numeric_and_boolean_permissive_on_strings() {
    // integer family: only parseable integers pass
    assert!(datatype_ok("42", "integer"));
    assert!(datatype_ok("-7", "integer"));
    assert!(!datatype_ok("4.2", "integer"), "a decimal is not an integer");
    assert!(!datatype_ok("abc", "integer"));
    assert!(datatype_ok("100", "nonNegativeInteger"));

    // decimal/double/float: any numeric
    assert!(datatype_ok("4.2", "decimal"));
    assert!(datatype_ok("42", "decimal"));
    assert!(datatype_ok("3.14", "double"));
    assert!(!datatype_ok("x", "decimal"));

    // boolean: the XSD lexical space only
    assert!(datatype_ok("true", "boolean"));
    assert!(datatype_ok("false", "boolean"));
    assert!(datatype_ok("1", "boolean"));
    assert!(!datatype_ok("yes", "boolean"), "'yes' is not an xsd:boolean");
    assert!(!datatype_ok("True", "boolean"), "case-sensitive lexical space");

    // string / anyURI / unknown / empty → permissive (any string is valid)
    assert!(datatype_ok("anything goes", "string"));
    assert!(datatype_ok("http://example/x", "anyURI"));
    assert!(datatype_ok("whatever", ""), "no datatype constraint → permissive");
    assert!(datatype_ok("whatever", "someUnknownType"));
}
