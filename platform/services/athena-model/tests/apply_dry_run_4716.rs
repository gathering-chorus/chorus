// @domain: domains — placed by chorus-crawl's domain rules (#4538)
// @card: #4716
// @owner: kade
//! #4716 — src/lib.rs `apply_plan`, the `apply --dry-run` verb in src/main.rs.
//! Retiring hand SPARQL means removing facts through the door, and a prod write
//! needs a dry run first. A dry run runs every check `apply` runs and shows the
//! one update it would send, but sends nothing.
use athena_model::{apply, apply_plan, verify_identity, ApplyOp, Store, R};
use std::cell::RefCell;

const NS: &str = "https://jeffbridwell.com/chorus#";

struct Cfg { updates: RefCell<Vec<String>> }
impl Store for Cfg {
    fn ask(&self, _sparql: &str) -> R<bool> { Ok(true) }
    fn select_v(&self, sparql: &str) -> R<Vec<String>> {
        if sparql.contains("sh:datatype") { Ok(vec!["ownerSequence|integer".into()]) }
        else if sparql.contains("definesVocabulary") { Ok(vec!["tests".into()]) }
        else { Ok(vec![]) }
    }
    fn update(&self, s: &str) -> R<()> { self.updates.borrow_mut().push(s.to_string()); Ok(()) }
}
fn cfg() -> Cfg { Cfg { updates: RefCell::new(vec![]) } }
fn set(f: &str, v: &str) -> ApplyOp { ApplyOp::Set { field: f.into(), value: v.into() } }
const G: &str = "urn:chorus:domains:tests";

#[test]
fn a_dry_run_shows_the_one_update_and_sends_nothing() {
    let s = cfg();
    let id = verify_identity(Some("kade"), &s).unwrap();
    let plan = apply_plan(&s, "test", "loom", G, None, &[set("comment", "a"), set("status", "b")], &id).unwrap();
    assert!(plan.contains(&format!("{NS}comment")) && plan.contains(&format!("{NS}status")), "{plan}");
    assert!(s.updates.borrow().is_empty(), "a dry run reaches the store: {:?}", s.updates.borrow());
}

#[test]
fn the_dry_run_shows_exactly_what_apply_then_sends() {
    let (a, b) = (cfg(), cfg());
    let id = verify_identity(Some("kade"), &a).unwrap();
    let ops = [set("comment", "a"), set("status", "b")];
    let plan = apply_plan(&a, "test", "loom", G, None, &ops, &id).unwrap();
    apply(&b, "test", "loom", G, None, &ops, &id).unwrap();
    // the modified stamp is the clock at write time; the two may straddle a second
    assert_eq!(b.updates.borrow().iter().map(|u| unstamp(u)).collect::<Vec<_>>(), vec![unstamp(&plan)]);
}

/// Every `"…Z"` timestamp literal replaced by `"T"`, so two runs compare on content.
fn unstamp(u: &str) -> String {
    let mut out = String::new();
    let mut rest = u;
    while let Some(i) = rest.find("\"20") {
        let tail = &rest[i + 1..];
        match tail.find('"') {
            Some(j) if tail[..j].ends_with('Z') => { out.push_str(&rest[..i]); out.push_str("\"T\""); rest = &tail[j + 1..]; }
            _ => { out.push_str(&rest[..i + 1]); rest = tail; }
        }
    }
    out.push_str(rest);
    out
}

// negative proof: a dry run must refuse exactly what apply refuses, or a green
// dry run would vouch for a write that then fails.
#[test]
fn negative_proof_a_dry_run_refuses_what_apply_refuses() {
    let s = cfg();
    let id = verify_identity(Some("kade"), &s).unwrap();
    let e = apply_plan(&s, "test", "loom", G, None, &[set("ownerSequence", "not-a-number")], &id).unwrap_err();
    assert!(e.contains("shape-violation"), "{e}");
}
