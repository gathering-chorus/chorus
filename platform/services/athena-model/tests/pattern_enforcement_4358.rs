//! #4358 — SHACL sh:pattern is enforced from the model, at every place sh:in is:
//! the door create (`write` → plan_writes), the field set (`set_field`), and the
//! bulk seed (`seed`). Property-level patterns test each value; a NodeShape's own
//! sh:pattern tests the row's full IRI (standard SHACL: STR(focus node)).
//!
//! Every accept test has a NEGATIVE PROOF beside it (#3734): a fixture that
//! violates the pattern and the write is shown to be REFUSED with nothing written.
//! A malformed regex in the model refuses too — a pattern the DAL cannot compile
//! must never read as a pattern the value satisfied.
//!
//! Hermetic: the stub answers shape SELECTs by query content; no live Fuseki.

use athena_model::{seed, set_field, verify_identity, write, Identity, Store, WriteReq, R};
use std::cell::RefCell;

const NS: &str = "https://jeffbridwell.com/chorus#";

struct Cfg {
    /// prop|flags|regex rows (property-level sh:pattern)
    prop_patterns: Vec<String>,
    /// flags|regex rows (node-level sh:pattern on the NodeShape)
    node_patterns: Vec<String>,
    exists: Vec<String>,
    updates: RefCell<Vec<String>>,
}

impl Store for Cfg {
    fn ask(&self, sparql: &str) -> R<bool> {
        if sparql.contains("urn:chorus:domains:security") {
            return Ok(true); // identity is not the variable here (identity_gate.rs)
        }
        Ok(self.exists.iter().any(|e| sparql.contains(e.as_str())))
    }
    fn select_v(&self, sparql: &str) -> R<Vec<String>> {
        if sparql.contains("?p sh:path ?path ; sh:pattern ?re") {
            Ok(self.prop_patterns.clone())
        } else if sparql.contains("; sh:pattern ?re") {
            Ok(self.node_patterns.clone())
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

fn cfg() -> Cfg {
    Cfg { prop_patterns: vec![], node_patterns: vec![], exists: vec![], updates: RefCell::new(vec![]) }
}

fn vid(s: &Cfg) -> Identity {
    verify_identity(Some("kade"), s).unwrap()
}

fn req_with(name: &str, prop: &str, value: &str) -> WriteReq {
    let mut req = WriteReq { kind: "domain".into(), name: name.into(), ..Default::default() };
    req.fields.insert(prop.into(), value.into());
    req
}

// ── door create (write) ─────────────────────────────────────────────────────

#[test]
fn write_accepts_a_value_on_the_property_pattern() {
    let mut s = cfg();
    s.prop_patterns = vec!["version||^v[0-9]+\\.[0-9]+$".into()];
    write(&s, &req_with("x", "version", "v1.8"), &vid(&s)).unwrap();
    assert_eq!(s.updates.borrow().len(), 1);
}

/// NEGATIVE PROOF: a value off the property's sh:pattern is refused, nothing written.
#[test]
fn negative_proof_write_refuses_a_value_off_the_property_pattern() {
    let mut s = cfg();
    s.prop_patterns = vec!["version||^v[0-9]+\\.[0-9]+$".into()];
    let e = write(&s, &req_with("x", "version", "1.8"), &vid(&s)).unwrap_err();
    assert_eq!(e, "shape-violation: '1.8' does not match sh:pattern ^v[0-9]+\\.[0-9]+$ for version", "{e}");
    assert!(s.updates.borrow().is_empty(), "nothing written on a pattern violation");
}

/// The regex goes last in the row, so a pattern containing '|' survives intact.
#[test]
fn write_reads_a_pattern_that_contains_a_pipe() {
    let mut s = cfg();
    s.prop_patterns = vec!["state||^(open|closed)$".into()];
    assert!(write(&s, &req_with("x", "state", "open"), &vid(&s)).is_ok());
    let e = write(&s, &req_with("y", "state", "ajar"), &vid(&s)).unwrap_err();
    assert!(e.contains("does not match sh:pattern ^(open|closed)$ for state"), "{e}");
}

/// sh:flags "i" is honoured; without it the same value is refused.
#[test]
fn write_honours_sh_flags() {
    let mut s = cfg();
    s.prop_patterns = vec!["code|i|^ab$".into()];
    assert!(write(&s, &req_with("x", "code", "AB"), &vid(&s)).is_ok());
    s.prop_patterns = vec!["code||^ab$".into()];
    assert!(write(&s, &req_with("x", "code", "AB"), &vid(&s)).is_err());
}

#[test]
fn write_accepts_a_row_iri_on_the_node_pattern() {
    let mut s = cfg();
    s.node_patterns = vec![format!("|^{}[a-z0-9-]+$", regex_escape(NS))];
    assert!(write(&s, &WriteReq { kind: "domain".into(), name: "icd".into(), ..Default::default() }, &vid(&s)).is_ok());
}

/// NEGATIVE PROOF: the row IRI off the NodeShape's sh:pattern is refused.
#[test]
fn negative_proof_write_refuses_a_row_iri_off_the_node_pattern() {
    let mut s = cfg();
    s.node_patterns = vec!["|#zz-[a-z]+$".into()];
    let e = write(&s, &WriteReq { kind: "domain".into(), name: "icd".into(), ..Default::default() }, &vid(&s)).unwrap_err();
    assert!(e.starts_with("shape-violation: row ") && e.ends_with(" does not match sh:pattern #zz-[a-z]+$"), "{e}");
    assert!(e.contains(&format!("{}icd", NS)), "names the row: {e}");
    assert!(s.updates.borrow().is_empty());
}

/// NEGATIVE PROOF: a malformed regex in the model refuses loudly — never a pass.
#[test]
fn negative_proof_a_malformed_model_regex_refuses_the_write() {
    let mut s = cfg();
    s.prop_patterns = vec!["version||^v[0-9+$".into()];
    let e = write(&s, &req_with("x", "version", "v1"), &vid(&s)).unwrap_err();
    assert!(e.starts_with("shape-model-error: sh:pattern ^v[0-9+$") && e.contains("does not compile"), "{e}");
    assert!(s.updates.borrow().is_empty());
    let mut s = cfg();
    s.node_patterns = vec!["|(unclosed".into()];
    let e = write(&s, &WriteReq { kind: "domain".into(), name: "icd".into(), ..Default::default() }, &vid(&s)).unwrap_err();
    assert!(e.starts_with("shape-model-error: sh:pattern (unclosed"), "{e}");
}

/// NEGATIVE PROOF: a flag the DAL cannot honour refuses, never silently drops.
#[test]
fn negative_proof_an_unknown_sh_flag_refuses() {
    let mut s = cfg();
    s.prop_patterns = vec!["code|q|^ab$".into()];
    let e = write(&s, &req_with("x", "code", "ab"), &vid(&s)).unwrap_err();
    assert!(e.starts_with("shape-model-error: sh:flags 'q'"), "{e}");
}

// ── set_field ───────────────────────────────────────────────────────────────

#[test]
fn set_accepts_a_value_on_the_pattern_and_refuses_one_off_it() {
    let mut s = cfg();
    s.exists = vec![format!("{}x", NS)];
    s.prop_patterns = vec!["version||^v[0-9]+$".into()];
    let id = vid(&s);
    assert!(set_field(&s, "domain", "x", "version", "v2", None, &id).is_ok());
    let before = s.updates.borrow().len();
    // NEGATIVE PROOF on the set path.
    let e = set_field(&s, "domain", "x", "version", "two", None, &id).unwrap_err();
    assert_eq!(e, "shape-violation: 'two' does not match sh:pattern ^v[0-9]+$ for version");
    assert_eq!(s.updates.borrow().len(), before, "nothing written on a refused set");
}

/// NEGATIVE PROOF: set on a subject whose IRI is off the node pattern is refused.
#[test]
fn negative_proof_set_refuses_a_row_iri_off_the_node_pattern() {
    let mut s = cfg();
    s.exists = vec![format!("{}x", NS)];
    s.node_patterns = vec!["|#dom-".into()];
    let id = vid(&s);
    let e = set_field(&s, "domain", "x", "version", "v2", None, &id).unwrap_err();
    assert!(e.starts_with("shape-violation: row ") && e.contains("does not match sh:pattern #dom-"), "{e}");
    assert!(s.updates.borrow().is_empty());
}

// ── seed ────────────────────────────────────────────────────────────────────

fn seed_triples(local: &str, version: &str) -> Vec<(String, String, String)> {
    vec![
        (format!("<{}{}>", NS, local), format!("<{}label>", NS), "\"L\"".to_string()),
        (format!("<{}{}>", NS, local), format!("<{}version>", NS), format!("\"{}\"", version)),
        (
            format!("<{}{}>", NS, local),
            "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>".to_string(),
            format!("<{}Domain>", NS),
        ),
    ]
}

#[test]
fn seed_accepts_values_and_iris_on_their_patterns() {
    let mut s = cfg();
    s.prop_patterns = vec!["version||^v[0-9]+$".into()];
    s.node_patterns = vec!["|#icd$".into()];
    let id = vid(&s);
    seed(&s, "domain", &seed_triples("icd", "v3"), "migrated", Some("urn:chorus:domains:icd"), &id).unwrap();
    assert_eq!(s.updates.borrow().len(), 1);
}

/// NEGATIVE PROOF: seed refuses a literal off the property pattern, whole batch.
#[test]
fn negative_proof_seed_refuses_a_value_off_the_property_pattern() {
    let mut s = cfg();
    s.prop_patterns = vec!["version||^v[0-9]+$".into()];
    let id = vid(&s);
    let e = seed(&s, "domain", &seed_triples("icd", "three"), "migrated", Some("urn:chorus:domains:icd"), &id).unwrap_err();
    assert_eq!(e, "shape-violation: 'three' does not match sh:pattern ^v[0-9]+$ for version");
    assert!(s.updates.borrow().is_empty());
}

/// NEGATIVE PROOF: seed refuses a row IRI off the node pattern.
#[test]
fn negative_proof_seed_refuses_a_row_iri_off_the_node_pattern() {
    let mut s = cfg();
    s.node_patterns = vec!["|#icd$".into()];
    let id = vid(&s);
    let e = seed(&s, "domain", &seed_triples("convergence", "v1"), "migrated", Some("urn:chorus:domains:icd"), &id).unwrap_err();
    assert_eq!(e, format!("shape-violation: row {}convergence does not match sh:pattern #icd$", NS));
    assert!(s.updates.borrow().is_empty());
}

fn regex_escape(s: &str) -> String {
    s.chars()
        .map(|c| if ".+*?()[]{}|^$\\/#:".contains(c) { format!("\\{c}") } else { c.to_string() })
        .collect()
}
