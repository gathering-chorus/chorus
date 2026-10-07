//! #4358 — SHACL sh:pattern is enforced from the model, at every place sh:in is:
//! the door create (`write` → plan_writes), the field set (`set_field`), and the
//! bulk seed (`seed`). Property-level patterns test each value; a NodeShape's own
//! sh:pattern tests the row's full IRI (standard SHACL: STR(focus node)).
//!
//! Every accept test has a NEGATIVE PROOF beside it (#3734): a fixture that
//! violates the pattern and the write is shown to be REFUSED with nothing written.
//! A regex the store cannot evaluate refuses too — a pattern nobody evaluated
//! must never read as a pattern the value satisfied.
//!
//! The DAL does not match regexes itself: SHACL defines sh:pattern as SPARQL
//! REGEX, so it asks the store. This stub stands in for the store's REGEX with a
//! table of canned verdicts keyed by (value, regex, flags) — parsed back out of
//! the query, unescaped, so the table also proves what the DAL sent. A query the
//! table does not know gets no answer, which the DAL must treat as a refusal.
//!
//! Hermetic: the stub answers shape SELECTs by query content; no live Fuseki.

use athena_model::{seed, set_field, verify_identity, write, Identity, Store, WriteReq, R};
use std::cell::RefCell;

const NS: &str = "https://jeffbridwell.com/chorus#";

/// What the stub's REGEX says for one (value, regex, flags).
#[derive(Clone)]
enum Says {
    Match,
    Miss,
    /// The store's expression error (a malformed regex is one).
    StoreError,
    /// An unbound / empty answer.
    Nothing,
}

type Verdict = (String, String, String, Says);

struct Cfg {
    /// prop|flags|regex rows (property-level sh:pattern)
    prop_patterns: Vec<String>,
    /// flags|regex rows (node-level sh:pattern on the NodeShape)
    node_patterns: Vec<String>,
    /// (value, regex, flags) → verdict
    regex: Vec<Verdict>,
    /// every REGEX query the DAL sent, as (value, regex, flags)
    asked: RefCell<Vec<(String, String, String)>>,
    exists: Vec<String>,
    updates: RefCell<Vec<String>>,
}

/// Read the three string arguments back out of `REGEX("v", "re", "fl")`,
/// undoing the SPARQL escapes.
fn regex_args(q: &str) -> Vec<String> {
    let body = &q[q.find("REGEX(").expect("REGEX call") + 6..];
    let mut out = Vec::new();
    let mut cur = String::new();
    let (mut inside, mut escaped) = (false, false);
    for c in body.chars() {
        if out.len() == 3 {
            break;
        }
        if !inside {
            if c == '"' {
                inside = true;
            }
            continue;
        }
        if escaped {
            cur.push(match c {
                'n' => '\n',
                'r' => '\r',
                other => other,
            });
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            out.push(std::mem::take(&mut cur));
            inside = false;
        } else {
            cur.push(c);
        }
    }
    out
}

impl Store for Cfg {
    fn ask(&self, sparql: &str) -> R<bool> {
        if sparql.contains("urn:chorus:domains:security") {
            return Ok(true); // identity is not the variable here (identity_gate.rs)
        }
        Ok(self.exists.iter().any(|e| sparql.contains(e.as_str())))
    }
    fn select_v(&self, sparql: &str) -> R<Vec<String>> {
        if sparql.contains("# athena-model sh:pattern check") {
            let a = regex_args(sparql);
            let key = (a[0].clone(), a[1].clone(), a[2].clone());
            self.asked.borrow_mut().push(key.clone());
            let hit = self.regex.iter().find(|(v, r, f, _)| (v, r, f) == (&key.0, &key.1, &key.2));
            return match hit.map(|h| &h.3) {
                Some(Says::Match) => Ok(vec!["1".into()]),
                Some(Says::Miss) => Ok(vec!["0".into()]),
                Some(Says::StoreError) => Err("Regex pattern exception: Unclosed character class".into()),
                Some(Says::Nothing) | None => Ok(vec![]),
            };
        }
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
    Cfg {
        prop_patterns: vec![],
        node_patterns: vec![],
        regex: vec![],
        asked: RefCell::new(vec![]),
        exists: vec![],
        updates: RefCell::new(vec![]),
    }
}

/// One canned verdict.
fn says(value: &str, re: &str, flags: &str, v: Says) -> Verdict {
    (value.into(), re.into(), flags.into(), v)
}

fn vid(s: &Cfg) -> Identity {
    verify_identity(Some("kade"), s).unwrap()
}

fn req_with(name: &str, prop: &str, value: &str) -> WriteReq {
    let mut req = WriteReq { kind: "domain".into(), name: name.into(), ..Default::default() };
    req.fields.insert(prop.into(), value.into());
    req
}

const VERSION_RE: &str = r"^v[0-9]+\.[0-9]+$";

// ── door create (write) ─────────────────────────────────────────────────────

#[test]
fn write_accepts_a_value_on_the_property_pattern() {
    let mut s = cfg();
    s.prop_patterns = vec![format!("version||{VERSION_RE}")];
    s.regex = vec![says("v1.8", VERSION_RE, "", Says::Match)];
    write(&s, &req_with("x", "version", "v1.8"), &vid(&s)).unwrap();
    assert_eq!(s.updates.borrow().len(), 1);
    assert_eq!(*s.asked.borrow(), vec![("v1.8".to_string(), VERSION_RE.to_string(), String::new())], "the store was asked");
}

/// NEGATIVE PROOF: a value off the property's sh:pattern is refused, nothing written.
#[test]
fn negative_proof_write_refuses_a_value_off_the_property_pattern() {
    let mut s = cfg();
    s.prop_patterns = vec![format!("version||{VERSION_RE}")];
    s.regex = vec![says("1.8", VERSION_RE, "", Says::Miss)];
    let e = write(&s, &req_with("x", "version", "1.8"), &vid(&s)).unwrap_err();
    assert_eq!(e, format!("shape-violation: '1.8' does not match sh:pattern {VERSION_RE} for version"), "{e}");
    assert!(s.updates.borrow().is_empty(), "nothing written on a pattern violation");
}

/// The regex goes last in the row, so a pattern containing '|' survives intact.
#[test]
fn write_reads_a_pattern_that_contains_a_pipe() {
    let mut s = cfg();
    s.prop_patterns = vec!["state||^(open|closed)$".into()];
    s.regex = vec![says("open", "^(open|closed)$", "", Says::Match), says("ajar", "^(open|closed)$", "", Says::Miss)];
    assert!(write(&s, &req_with("x", "state", "open"), &vid(&s)).is_ok());
    let e = write(&s, &req_with("y", "state", "ajar"), &vid(&s)).unwrap_err();
    assert!(e.contains("does not match sh:pattern ^(open|closed)$ for state"), "{e}");
}

/// sh:flags travel to the store's REGEX as its third argument.
#[test]
fn write_passes_sh_flags_to_the_store() {
    let mut s = cfg();
    s.prop_patterns = vec!["code|i|^ab$".into()];
    s.regex = vec![says("AB", "^ab$", "i", Says::Match)];
    write(&s, &req_with("x", "code", "AB"), &vid(&s)).unwrap();
    assert_eq!(s.asked.borrow()[0].2, "i");
}

/// A value carrying a quote, a backslash and a newline reaches the store's REGEX
/// intact — escaped into the query, not breaking out of the literal.
#[test]
fn write_escapes_the_value_and_the_regex_into_the_query() {
    let mut s = cfg();
    let value = "a\"b\\c\nd";
    let re = "^a\"b\\\\c";
    s.prop_patterns = vec![format!("note||{re}")];
    s.regex = vec![says(value, re, "", Says::Match)];
    write(&s, &req_with("x", "note", value), &vid(&s)).unwrap();
    assert_eq!(*s.asked.borrow(), vec![(value.to_string(), re.to_string(), String::new())]);
}

/// A class with no pattern asks the store nothing.
#[test]
fn a_class_without_patterns_pays_no_regex_query() {
    let s = cfg();
    write(&s, &req_with("x", "version", "anything"), &vid(&s)).unwrap();
    assert!(s.asked.borrow().is_empty());
}

#[test]
fn write_accepts_a_row_iri_on_the_node_pattern() {
    let mut s = cfg();
    let iri = format!("{NS}icd");
    s.node_patterns = vec!["|#[a-z0-9-]+$".into()];
    s.regex = vec![says(&iri, "#[a-z0-9-]+$", "", Says::Match)];
    assert!(write(&s, &WriteReq { kind: "domain".into(), name: "icd".into(), ..Default::default() }, &vid(&s)).is_ok());
}

/// NEGATIVE PROOF: the row IRI off the NodeShape's sh:pattern is refused.
#[test]
fn negative_proof_write_refuses_a_row_iri_off_the_node_pattern() {
    let mut s = cfg();
    let iri = format!("{NS}icd");
    s.node_patterns = vec!["|#zz-[a-z]+$".into()];
    s.regex = vec![says(&iri, "#zz-[a-z]+$", "", Says::Miss)];
    let e = write(&s, &WriteReq { kind: "domain".into(), name: "icd".into(), ..Default::default() }, &vid(&s)).unwrap_err();
    assert_eq!(e, format!("shape-violation: row {iri} does not match sh:pattern #zz-[a-z]+$"));
    assert!(s.updates.borrow().is_empty());
}

/// NEGATIVE PROOF: a regex the store cannot evaluate (its REGEX errors on a
/// malformed pattern) refuses loudly — never a pass. So does an empty answer.
#[test]
fn negative_proof_an_unevaluable_regex_refuses_the_write() {
    let mut s = cfg();
    s.prop_patterns = vec!["version||^v[0-9+$".into()];
    s.regex = vec![says("v1", "^v[0-9+$", "", Says::StoreError)];
    let e = write(&s, &req_with("x", "version", "v1"), &vid(&s)).unwrap_err();
    assert_eq!(e, "shape-model-error: sh:pattern ^v[0-9+$ on https://jeffbridwell.com/chorus#Domain version could not be evaluated — refused, not skipped");
    assert!(s.updates.borrow().is_empty());

    let mut s = cfg();
    let iri = format!("{NS}icd");
    s.node_patterns = vec!["|(unclosed".into()];
    s.regex = vec![says(&iri, "(unclosed", "", Says::Nothing)];
    let e = write(&s, &WriteReq { kind: "domain".into(), name: "icd".into(), ..Default::default() }, &vid(&s)).unwrap_err();
    assert!(e.starts_with("shape-model-error: sh:pattern (unclosed on ") && e.ends_with("could not be evaluated — refused, not skipped"), "{e}");
    assert!(s.updates.borrow().is_empty());
}

/// NEGATIVE PROOF: a flag outside SPARQL REGEX's set refuses, never silently drops.
#[test]
fn negative_proof_an_unknown_sh_flag_refuses() {
    let mut s = cfg();
    s.prop_patterns = vec!["code|q|^ab$".into()];
    let e = write(&s, &req_with("x", "code", "ab"), &vid(&s)).unwrap_err();
    assert!(e.starts_with("shape-model-error: sh:flags 'q'"), "{e}");
    assert!(s.asked.borrow().is_empty(), "refused before any evaluation");
}

// ── set_field ───────────────────────────────────────────────────────────────

#[test]
fn set_accepts_a_value_on_the_pattern_and_refuses_one_off_it() {
    let mut s = cfg();
    s.exists = vec![format!("{}x", NS)];
    s.prop_patterns = vec!["version||^v[0-9]+$".into()];
    s.regex = vec![says("v2", "^v[0-9]+$", "", Says::Match), says("two", "^v[0-9]+$", "", Says::Miss)];
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
    let iri = format!("{NS}x");
    s.exists = vec![iri.clone()];
    s.node_patterns = vec!["|#dom-".into()];
    s.regex = vec![says(&iri, "#dom-", "", Says::Miss)];
    let id = vid(&s);
    let e = set_field(&s, "domain", "x", "version", "v2", None, &id).unwrap_err();
    assert_eq!(e, format!("shape-violation: row {iri} does not match sh:pattern #dom-"));
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
    s.regex = vec![says("v3", "^v[0-9]+$", "", Says::Match), says(&format!("{NS}icd"), "#icd$", "", Says::Match)];
    let id = vid(&s);
    seed(&s, "domain", &seed_triples("icd", "v3"), "migrated", Some("urn:chorus:domains:icd"), &id).unwrap();
    assert_eq!(s.updates.borrow().len(), 1);
}

/// NEGATIVE PROOF: seed refuses a literal off the property pattern, whole batch.
#[test]
fn negative_proof_seed_refuses_a_value_off_the_property_pattern() {
    let mut s = cfg();
    s.prop_patterns = vec!["version||^v[0-9]+$".into()];
    s.regex = vec![says("three", "^v[0-9]+$", "", Says::Miss)];
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
    s.regex = vec![says(&format!("{NS}convergence"), "#icd$", "", Says::Miss)];
    let id = vid(&s);
    let e = seed(&s, "domain", &seed_triples("convergence", "v1"), "migrated", Some("urn:chorus:domains:icd"), &id).unwrap_err();
    assert_eq!(e, format!("shape-violation: row {}convergence does not match sh:pattern #icd$", NS));
    assert!(s.updates.borrow().is_empty());
}
