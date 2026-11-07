// @domain: domains — placed by chorus-crawl's domain rules (#4538)
// @card: #4715
// @owner: kade
//! #4715 AC1 — src/lib.rs `apply` + `DeferredStore`, the `apply` verb in
//! src/main.rs. The door patched one DAL process per field, so a refusal at
//! field 3 left fields 1-2 changed. `apply` runs every governed check unchanged
//! and sends everything as ONE update: the row changes whole or not at all.
use athena_model::{apply, parse_apply_ndjson, verify_identity, ApplyOp, DeferredStore, Store, R};
use std::cell::RefCell;

const NS: &str = "https://jeffbridwell.com/chorus#";

struct Cfg { datatypes: Vec<(String, String)>, updates: RefCell<Vec<String>> }
impl Store for Cfg {
    fn ask(&self, _sparql: &str) -> R<bool> { Ok(true) } // the row, the identity and every target exist
    fn select_v(&self, sparql: &str) -> R<Vec<String>> {
        if sparql.contains("sh:datatype") {
            Ok(self.datatypes.iter().map(|(p, d)| format!("{}|{}", p, d)).collect())
        } else if sparql.contains("definesVocabulary") {
            Ok(vec!["tests".to_string()])
        } else {
            Ok(vec![])
        }
    }
    fn update(&self, s: &str) -> R<()> { self.updates.borrow_mut().push(s.to_string()); Ok(()) }
}
fn cfg() -> Cfg { Cfg { datatypes: vec![("ownerSequence".into(), "integer".into())], updates: RefCell::new(vec![]) } }
fn set(f: &str, v: &str) -> ApplyOp { ApplyOp::Set { field: f.into(), value: v.into() } }

#[test]
fn three_fields_land_in_one_update() {
    let s = cfg();
    let id = verify_identity(Some("kade"), &s).unwrap();
    let n = apply(&s, "test", "loom", "urn:chorus:domains:tests", None, &[set("comment", "a"), set("status", "b"), set("ownerSequence", "3")], &id).unwrap();
    assert_eq!(s.updates.borrow().len(), 1, "one request to the store: {:?}", s.updates.borrow());
    assert_eq!(n, 3);
    let u = &s.updates.borrow()[0];
    assert!(u.contains(&format!("{NS}comment")) && u.contains(&format!("{NS}status")) && u.contains(&format!("{NS}ownerSequence")), "{u}");
}

// negative proof: field 3 is refused (not an integer). The old per-field path had
// already written fields 1 and 2; apply writes nothing at all.
#[test]
fn negative_proof_a_refusal_at_field_three_writes_nothing() {
    let s = cfg();
    let id = verify_identity(Some("kade"), &s).unwrap();
    let e = apply(&s, "test", "loom", "urn:chorus:domains:tests", None, &[set("comment", "a"), set("status", "b"), set("ownerSequence", "not-a-number")], &id).unwrap_err();
    assert!(e.contains("shape-violation"), "{e}");
    assert!(s.updates.borrow().is_empty(), "nothing may reach the store: {:?}", s.updates.borrow());
}

#[test]
fn the_deferred_store_reads_through_and_holds_writes() {
    let s = cfg();
    let d = DeferredStore::new(&s);
    assert!(d.ask("ASK {}").unwrap());
    d.update("DELETE WHERE { GRAPH <g> { <s> <p> ?o } } ;").unwrap();
    d.update("INSERT DATA { GRAPH <g> { <s> <p> \"v\" } }").unwrap();
    assert_eq!(d.held(), 2);
    assert!(s.updates.borrow().is_empty(), "held, not sent");
    d.commit().unwrap();
    assert_eq!(s.updates.borrow().len(), 1);
    assert!(!s.updates.borrow()[0].contains(";\n;"), "no empty statement between held updates");
}

#[test]
fn the_stdin_form_parses_every_op_kind() {
    let input = r#"{"set":{"field":"comment","value":"x"}}
{"link":[["ownedBy","principal","kade"]]}
{"del":["<s>","<p>","?o"]}
{"ins":["<s>","<p>","<o>"]}"#;
    let (rev, ops) = parse_apply_ndjson(input).unwrap();
    assert!(rev.is_none());
    assert_eq!(ops.len(), 3, "{ops:?}");
    assert!(matches!(&ops[0], ApplyOp::Triples { deletes, inserts } if deletes.len() == 1 && inserts.len() == 1));
    assert!(parse_apply_ndjson(r#"{"nope":1}"#).is_err(), "an unknown line is refused, not skipped");
}
