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
    unique_within: Vec<(String, String)>, // #3681 — path|partition rows (chorus:uniqueWithin)
    unique_global: Vec<String>,          // #3681 — paths declared chorus:uniqueGlobal true
    dup: bool,                           // #3681 — a conflicting sibling exists (uniqueness ASK → this)
    updates: RefCell<Vec<String>>,
}
impl Store for Cfg {
    fn ask(&self, sparql: &str) -> R<bool> {
        if sparql.contains("urn:chorus:domains:security") {
            return Ok(true); // identity is not the variable in these tests (see identity_gate.rs)
        }
        if sparql.contains("?other") {
            return Ok(self.dup); // #3681 — the uniqueness ASK (only query using ?other)
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
        } else if sparql.contains("uniqueWithin") {
            Ok(self.unique_within.iter().map(|(p, q)| format!("{}|{}", p, q)).collect())
        } else if sparql.contains("uniqueGlobal") {
            Ok(self.unique_global.clone())
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
    Cfg { required: vec![], datatypes: vec![], edge_classes: vec![], exists: vec![], typed: vec![], unique_within: vec![], unique_global: vec![], dup: false, updates: RefCell::new(vec![]) }
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

// ── #3681 — uniqueness-within-scope enforcement (uniqueWithin / uniqueGlobal) ──

#[test]
fn write_refuses_duplicate_value_within_partition() {
    // shape: `rank` uniqueWithin `inChunk`; a sibling with the same rank in the same
    // partition already exists (dup=true) → refuse, naming the property, before insert.
    let mut s = cfg();
    s.unique_within = vec![("rank".into(), "inChunk".into())];
    s.dup = true;
    let mut req = WriteReq {
        kind: "domain".into(),
        name: "m1".into(),
        edges: vec![("inChunk".into(), "domain".into(), "chunkx".into())],
        ..Default::default()
    };
    req.fields.insert("rank".into(), "1".into());
    let e = write(&s, &req, &vid(&s)).unwrap_err();
    assert!(e.starts_with("shape-violation") && e.contains("rank") && e.contains("uniqueWithin"), "{}", e);
    assert!(s.updates.borrow().is_empty(), "nothing written on a uniqueness violation");
}

#[test]
fn write_refuses_duplicate_value_globally() {
    // shape: `loomSequence` uniqueGlobal; another instance already carries the value → refuse.
    let mut s = cfg();
    s.unique_global = vec!["loomSequence".into()];
    s.dup = true;
    let mut req = WriteReq { kind: "domain".into(), name: "c1".into(), ..Default::default() };
    req.fields.insert("loomSequence".into(), "1".into());
    let e = write(&s, &req, &vid(&s)).unwrap_err();
    assert!(e.starts_with("shape-violation") && e.contains("loomSequence") && e.contains("uniqueGlobal"), "{}", e);
    assert!(s.updates.borrow().is_empty(), "nothing written on a global uniqueness violation");
}

#[test]
fn write_allows_unique_value_when_no_sibling() {
    // same annotation, but no conflicting sibling (dup=false) → the write proceeds.
    let mut s = cfg();
    s.unique_within = vec![("rank".into(), "inChunk".into())];
    s.dup = false;
    s.exists = vec![format!("{}chunkx", NS)]; // inChunk target resolves (domain is bare-grain → NS#chunkx)
    let mut req = WriteReq {
        kind: "domain".into(),
        name: "m2".into(),
        edges: vec![("inChunk".into(), "domain".into(), "chunkx".into())],
        ..Default::default()
    };
    req.fields.insert("rank".into(), "2".into());
    assert!(write(&s, &req, &vid(&s)).is_ok(), "a unique value with no sibling passes");
}

#[test]
fn write_unaffected_when_no_uniqueness_annotation() {
    // AC5: a shape with no uniqueWithin/uniqueGlobal is not touched — even a would-be
    // duplicate (dup=true) writes, because nothing declares the value unique.
    let mut s = cfg();
    s.dup = true; // a sibling exists, but no annotation consults it
    let mut req = WriteReq { kind: "domain".into(), name: "d1".into(), ..Default::default() };
    req.fields.insert("rank".into(), "1".into());
    assert!(write(&s, &req, &vid(&s)).is_ok(), "no annotation → uniqueness never checked");
}
