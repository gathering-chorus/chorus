//! #3686 — the `set` verb: field-level single-predicate update. Red-first
//! (DEC-1674) against Silas's three correctness bars (blessed 2026-07-25):
//!   1. single-predicate DELETE — sibling props survive (never `?p ?o` on the subject)
//!   2. same gates as add; the uniqueness ASK excludes self (re-set same value ≠ dup)
//!   3. idempotent — set k=v when already k=v is a clean success
//! Plus the one designed difference from add: the uniqueWithin partition value
//! comes FROM THE STORE (the subject's existing ownedBy) since a set carries no
//! edges; a subject missing its partition edge is refused, not unscoped.
//! Hermetic: the Cfg stub answers shape SELECTs / ASKs / the partition read.

use chorus_model::{set_field, verify_identity, Identity, Store, R};
use std::cell::RefCell;

struct Cfg {
    datatypes: Vec<(String, String)>,     // path|xsd rows (sh:datatype)
    unique_within: Vec<(String, String)>, // prop|partition rows (chorus:uniqueWithin)
    unique_global: Vec<String>,           // props (chorus:uniqueGlobal)
    exists: Vec<String>,                  // IRIs that exist (subject ASK)
    partition_target: Option<String>,     // store's answer for the subject's ownedBy
    sibling_dup: bool,                    // sibling-duplicate ASK verdict
    asks: RefCell<Vec<String>>,
    updates: RefCell<Vec<String>>,
}
impl Store for Cfg {
    fn ask(&self, sparql: &str) -> R<bool> {
        if sparql.contains("urn:chorus:domains:security") {
            return Ok(true); // identity registry — identity_gate.rs owns that variable
        }
        self.asks.borrow_mut().push(sparql.to_string());
        if sparql.contains("?other") {
            return Ok(self.sibling_dup);
        }
        Ok(self.exists.iter().any(|e| sparql.contains(e.as_str())))
    }
    fn select_v(&self, sparql: &str) -> R<Vec<String>> {
        if sparql.contains("uniqueWithin") {
            Ok(self.unique_within.iter().map(|(p, s)| format!("{}|{}", p, s)).collect())
        } else if sparql.contains("uniqueGlobal") {
            Ok(self.unique_global.clone())
        } else if sparql.contains("sh:datatype") {
            Ok(self.datatypes.iter().map(|(p, d)| format!("{}|{}", p, d)).collect())
        } else if sparql.contains("ownedBy") {
            Ok(self.partition_target.iter().cloned().collect())
        } else {
            Ok(vec![])
        }
    }
    fn update(&self, s: &str) -> R<()> {
        self.updates.borrow_mut().push(s.to_string());
        Ok(())
    }
}

const NS: &str = "https://jeffbridwell.com/chorus#";

fn cfg() -> Cfg {
    Cfg {
        datatypes: vec![("ownerSequence".into(), "integer".into())],
        unique_within: vec![("ownerSequence".into(), "ownedBy".into())],
        unique_global: vec![],
        exists: vec![format!("{}loom", NS)],
        partition_target: Some(format!("{}role-wren", NS)),
        sibling_dup: false,
        asks: RefCell::new(vec![]),
        updates: RefCell::new(vec![]),
    }
}
fn vid(s: &Cfg) -> Identity {
    verify_identity(Some("wren"), s).unwrap()
}

// ── bar 1: single-predicate DELETE — siblings survive ────────────────────────
#[test]
fn set_touches_only_the_named_predicate_never_the_whole_subject() {
    let s = cfg();
    let id = vid(&s);
    set_field(&s, "domain", "loom", "ownerSequence", "2", None, &id).unwrap();
    let ups = s.updates.borrow();
    assert_eq!(ups.len(), 1, "one tx");
    let u = &ups[0];
    assert!(u.contains(&format!("<{}ownerSequence>", NS)), "targets the named predicate: {}", u);
    assert!(!u.contains("?p ?o"), "must NEVER delete the whole subject (#3587 wipe class): {}", u);
    assert!(u.contains("\"2\""), "inserts the new value: {}", u);
}

// ── bar 2a: uniqueness fires on a real sibling dup ───────────────────────────
#[test]
fn set_refuses_duplicate_within_partition_read_from_store() {
    let mut s = cfg();
    s.sibling_dup = true;
    let id = vid(&s);
    let err = set_field(&s, "domain", "loom", "ownerSequence", "2", None, &id).unwrap_err();
    assert!(err.contains("ownerSequence"), "refusal names the prop: {}", err);
    assert!(s.updates.borrow().is_empty(), "no write on refusal");
}

// ── bar 2b: the sibling ASK excludes self + partition came from the store ────
#[test]
fn set_uniqueness_ask_excludes_the_subject_itself() {
    let s = cfg();
    let id = vid(&s);
    set_field(&s, "domain", "loom", "ownerSequence", "2", None, &id).unwrap();
    let asks = s.asks.borrow();
    let sibling = asks.iter().find(|a| a.contains("?other")).expect("sibling ASK ran");
    assert!(
        sibling.contains(&format!("{}loom", NS)) && sibling.contains("!="),
        "ASK self-excludes the subject: {}",
        sibling
    );
    assert!(sibling.contains("role-wren"), "partition value from the STORE's ownedBy read: {}", sibling);
}

// ── bar 2c: missing partition edge = refuse, never unscoped uniqueness ───────
#[test]
fn set_refuses_when_partition_edge_absent_on_subject() {
    let mut s = cfg();
    s.partition_target = None;
    let id = vid(&s);
    let err = set_field(&s, "domain", "loom", "ownerSequence", "2", None, &id).unwrap_err();
    assert!(err.contains("ownedBy"), "names the missing partition: {}", err);
    assert!(s.updates.borrow().is_empty());
}

// ── bar 3: idempotent — same value again is a clean success ──────────────────
#[test]
fn set_same_value_twice_is_a_clean_noop_success() {
    let s = cfg();
    let id = vid(&s);
    set_field(&s, "domain", "loom", "ownerSequence", "2", None, &id).unwrap();
    set_field(&s, "domain", "loom", "ownerSequence", "2", None, &id).unwrap();
    assert_eq!(s.updates.borrow().len(), 2, "both writes succeed (DELETE+INSERT same value)");
}

// ── same gates as add: datatype + existence ──────────────────────────────────
#[test]
fn set_refuses_wrong_datatype_value() {
    let s = cfg();
    let id = vid(&s);
    let err = set_field(&s, "domain", "loom", "ownerSequence", "not-a-number", None, &id).unwrap_err();
    assert!(err.contains("integer"), "datatype gate fired: {}", err);
    assert!(s.updates.borrow().is_empty());
}

#[test]
fn set_refuses_unknown_subject_fail_closed() {
    let mut s = cfg();
    s.exists = vec![];
    let id = vid(&s);
    let err = set_field(&s, "domain", "nonexistent", "ownerSequence", "1", None, &id).unwrap_err();
    assert!(err.contains("not-found") || err.contains("does not exist"), "{}", err);
    assert!(s.updates.borrow().is_empty());
}

// ── uniqueGlobal path (rolePriority): class-wide sibling, still self-excluding ──
#[test]
fn set_enforces_global_uniqueness_for_role_priority() {
    let mk = |dup: bool| Cfg {
        datatypes: vec![("rolePriority".into(), "integer".into())],
        unique_within: vec![],
        unique_global: vec!["rolePriority".into()],
        exists: vec![format!("{}role-wren", NS)],
        partition_target: None, // global needs no partition
        sibling_dup: dup,
        asks: RefCell::new(vec![]),
        updates: RefCell::new(vec![]),
    };
    let s = mk(true);
    let id = vid(&s);
    let err = set_field(&s, "role", "wren", "rolePriority", "1", None, &id).unwrap_err();
    assert!(err.contains("rolePriority"), "{}", err);
    assert!(s.updates.borrow().is_empty());

    let s2 = mk(false);
    let id2 = vid(&s2);
    set_field(&s2, "role", "wren", "rolePriority", "1", None, &id2).unwrap();
    assert_eq!(s2.updates.borrow().len(), 1);
}
