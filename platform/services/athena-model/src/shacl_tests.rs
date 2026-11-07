// @test-type: unit
// @domain: persistence
// @card: #4717
//! #4717 AC1 — src/shacl.rs `gate`, `rewrite_to_scratch`, `parse_report`, `sweep`
//! against a store that records every update and answers SHACL with canned
//! reports: three violations refuse the write with all three and nothing reaches
//! the real graph; a dirty row is refused only for what a write adds; a failed
//! check refuses and still drops the scratch; a non-report is an error.
use super::*;
use std::cell::RefCell;

const ROW: &str = "https://jeffbridwell.com/chorus#alert-a";

fn report(results: &[(&str, &str, &str)]) -> String {
    let mut s = format!("_:r <{SH}conforms> \"{}\"^^<http://www.w3.org/2001/XMLSchema#boolean> .\n", results.is_empty());
    for (i, (focus, path, msg)) in results.iter().enumerate() {
        s.push_str(&format!("_:r <{SH}result> _:v{i} .\n_:v{i} <{SH}focusNode> <{focus}> .\n_:v{i} <{SH}resultPath> <{path}> .\n_:v{i} <{SH}resultMessage> \"{msg}\" .\n"));
    }
    s
}

/// A store that records every update and answers SHACL with a canned report.
/// `reports` answer the before and after checks in turn; the last repeats.
struct Shacl { reports: RefCell<Vec<R<String>>>, graphs: Vec<String>, updates: RefCell<Vec<String>> }
impl Store for Shacl {
    fn ask(&self, _: &str) -> R<bool> { Ok(false) }
    fn select_v(&self, _: &str) -> R<Vec<String>> { Ok(self.graphs.clone()) }
    fn update(&self, s: &str) -> R<()> { self.updates.borrow_mut().push(s.to_string()); Ok(()) }
    fn shacl_enabled(&self) -> bool { true }
    fn shapes(&self, _: &[String]) -> R<String> { Ok(String::new()) }
    fn shacl(&self, _: &str, _: &str) -> R<String> {
        let mut r = self.reports.borrow_mut();
        if r.len() > 1 { r.remove(0) } else { r[0].clone() }
    }
}
fn store(report: R<String>) -> Shacl { Shacl { reports: RefCell::new(vec![report]), graphs: vec![], updates: RefCell::new(vec![]) } }
/// the gate checks after the write first, and before it only when needed
fn store2(before: R<String>, after: R<String>) -> Shacl { Shacl { reports: RefCell::new(vec![after, before]), ..store(Ok(String::new())) } }
const WRITE: &str = "INSERT DATA { GRAPH <urn:chorus:instances:alerts> { <https://jeffbridwell.com/chorus#alert-a> <https://jeffbridwell.com/chorus#platform> \"x\" } }";

#[test]
fn a_write_is_pointed_at_the_scratch_and_the_ontology_is_left_alone() {
    let w = "DELETE { GRAPH <urn:g:a> { ?s ?p ?o } } INSERT { GRAPH  <urn:g:b> { ?s ?p 1 } } WHERE { GRAPH <urn:chorus:ontology> { ?c ?d ?e } GRAPH ?g { ?s ?p ?o } } ; # NOGRAPH <urn:x>";
    let r = rewrite_to_scratch(w, "urn:s");
    assert_eq!(r, "DELETE { GRAPH <urn:s> { ?s ?p ?o } } INSERT { GRAPH  <urn:s> { ?s ?p 1 } } WHERE { GRAPH <urn:chorus:ontology> { ?c ?d ?e } GRAPH ?g { ?s ?p ?o } } ; # NOGRAPH <urn:x>");
}

/// NEGATIVE (Abby, demo review): `GRAPH <iri>` inside a string value is data
/// and stays exactly as written.
#[test]
fn graph_text_inside_a_literal_is_not_rewritten() {
    let w = "INSERT DATA { GRAPH <urn:g:a> { <urn:s> <urn:p> \"see GRAPH <urn:g:a> and \\\" GRAPH <urn:x>\" } }";
    assert_eq!(rewrite_to_scratch(w, "urn:s"), "INSERT DATA { GRAPH <urn:s> { <urn:s> <urn:p> \"see GRAPH <urn:g:a> and \\\" GRAPH <urn:x>\" } }");
}

#[test]
fn a_report_yields_every_violation_and_a_conforming_one_none() {
    let v = parse_report(&report(&[(ROW, "urn:p#a", "need 1"), (ROW, "urn:p#b", "not \\\"bool\\\""), (ROW, "urn:p#c", "bad class")])).unwrap();
    assert_eq!(v.len(), 3);
    assert_eq!(v[1].message, "not \"bool\"");
    assert!(parse_report(&report(&[])).unwrap().is_empty());
}

/// NEGATIVE: an answer that is not a report is an error, never "conforms".
#[test]
fn negative_proof_a_non_report_is_an_error_not_a_pass() {
    assert!(parse_report("").is_err());
    assert!(parse_report("<a> <b> <c> .").is_err());
    assert!(parse_report("<html>oops</html>").is_err());
}

#[test]
fn three_violations_refuse_the_write_with_all_three_and_nothing_is_written() {
    let s = store2(Ok(report(&[])), Ok(report(&[(ROW, "urn:p#a", "need 1"), (ROW, "urn:p#b", "need bool"), (ROW, "urn:p#c", "need Platform")])));
    let e = gate(&s, WRITE, &[ROW.to_string()]).unwrap_err();
    assert!(e.starts_with("shape-violation: 3 violation(s): a: need 1; b: need bool; c: need Platform"), "{e}");
    let u = s.updates.borrow();
    assert!(u.iter().all(|x| !x.contains("GRAPH <urn:chorus:instances:alerts>")), "the real graph was written: {u:?}");
    assert!(u.last().unwrap().starts_with(&format!("DROP SILENT GRAPH <{SCRATCH_PREFIX}")), "scratch not dropped: {u:?}");
}

/// Wren 10-24: an edit to a row that already breaks its shape, fixing
/// nothing and breaking nothing new, passes; what it adds is refused alone.
#[test]
fn a_dirty_row_is_refused_only_for_what_the_write_adds() {
    let old = (ROW, "urn:p#rank", "rank is an integer");
    assert!(gate(&store2(Ok(report(&[old])), Ok(report(&[old]))), WRITE, &[ROW.to_string()]).is_ok());
    let e = gate(&store2(Ok(report(&[old])), Ok(report(&[old, (ROW, "urn:p#owner", "needs an owner")]))), WRITE, &[ROW.to_string()]).unwrap_err();
    assert_eq!(e, "shape-violation: 1 violation(s): owner: needs an owner");
}

#[test]
fn violations_on_rows_the_write_did_not_touch_do_not_refuse_it() {
    let s = store(Ok(report(&[("https://jeffbridwell.com/chorus#platform-old", "urn:p#a", "old row")])));
    assert!(gate(&s, WRITE, &[ROW.to_string()]).is_ok());
}

/// NEGATIVE: a store whose SHACL fails refuses the write (never a silent pass)
/// and still drops its scratch.
#[test]
fn negative_proof_a_failed_check_refuses_and_still_drops_the_scratch() {
    let s = store(Err("fuseki/shacl: HTTP failure".into()));
    assert!(gate(&s, WRITE, &[ROW.to_string()]).unwrap_err().contains("HTTP failure"));
    assert!(s.updates.borrow().last().unwrap().starts_with("DROP SILENT GRAPH"));
}

#[test]
fn the_sweeper_drops_only_scratch_graphs_past_their_time() {
    let now = 1_800_000_000;
    let s = Shacl { graphs: vec![format!("{SCRATCH_PREFIX}{}-1-2", now - 700), format!("{SCRATCH_PREFIX}{}-1-2", now - 10)], ..store(Ok(String::new())) };
    assert_eq!(sweep(&s, now).unwrap(), 1);
    let u = s.updates.borrow();
    assert!(u[0].contains(&format!("{}-1-2", now - 700)) && !u[0].contains(&format!("{}-1-2>", now - 10)), "{u:?}");
}
