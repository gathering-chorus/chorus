//! #4717 AC1 — the store checks shapes. Before a governed write is sent, the
//! DAL runs it against a scratch copy and asks Jena's SHACL validator (the
//! dataset's `fuseki:shacl` service) about the result, so a write that breaks
//! the model is refused with EVERY violation in one answer and nothing is
//! written. The hand-rolled per-field checks stay; this is the store's own
//! judgement over the same shapes (urn:chorus:ontology).
//!
//! One scratch graph per write (`urn:chorus:shacl-scratch:<epoch>-<nonce>`,
//! outside every gathering and chorus graph): the row as it is, the write
//! applied to it, then the full rows of everything it links to (so `sh:node`
//! and `sh:class` see their targets) and the subClassOf chain of their types.
//! The scratch is dropped on every exit; a sweeper drops any older than
//! SCRATCH_TTL_SECS left by a killed process.
use super::{witness, Store, ONTOLOGY_GRAPH, R};
use std::collections::{BTreeMap, BTreeSet};

pub const SCRATCH_PREFIX: &str = "urn:chorus:shacl-scratch:";
pub const SCRATCH_TTL_SECS: u64 = 600;
const SH: &str = "http://www.w3.org/ns/shacl#";
const RDFS_SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";

/// One sh:ValidationResult, as the report names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub focus: String,
    pub path: String,
    pub message: String,
}

fn epoch() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// A scratch graph name that carries its birth time, for the sweeper.
pub fn scratch_name(now: u64) -> String {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    format!("{SCRATCH_PREFIX}{now}-{}-{nanos}", std::process::id())
}

/// Every `GRAPH <g>` in `update` pointed at `scratch`, except the ontology's
/// (shapes and class rows are read there, never written by a row write).
pub fn rewrite_to_scratch(update: &str, scratch: &str) -> String {
    let mut out = String::with_capacity(update.len());
    let mut rest = update;
    while let Some(i) = rest.find("GRAPH") {
        let (head, tail) = rest.split_at(i);
        out.push_str(head);
        let after = &tail[5..];
        let word = head.chars().last().map_or(true, |c| !c.is_alphanumeric() && c != '_');
        let trimmed = after.trim_start();
        let gap = &after[..after.len() - trimmed.len()];
        match trimmed.strip_prefix('<').and_then(|t| t.split_once('>')) {
            Some((iri, after_iri)) if word => {
                let target = if iri == ONTOLOGY_GRAPH { iri } else { scratch };
                out.push_str(&format!("GRAPH{gap}<{target}>"));
                rest = after_iri;
            }
            _ => {
                out.push_str("GRAPH");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// One term of an N-Triples line, and the rest of the line.
fn nt_term(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    if let Some(t) = s.strip_prefix('<') {
        let (iri, rest) = t.split_once('>')?;
        return Some((iri.to_string(), rest));
    }
    if s.starts_with("_:") {
        let end = s.find(char::is_whitespace).unwrap_or(s.len());
        return Some((s[..end].to_string(), &s[end..]));
    }
    if let Some(t) = s.strip_prefix('"') {
        let (mut lit, mut chars) = (String::new(), t.char_indices());
        while let Some((i, c)) = chars.next() {
            match c {
                '\\' => match chars.next().map(|(_, e)| e) {
                    Some('n') => lit.push('\n'),
                    Some('t') => lit.push('\t'),
                    Some(e) => lit.push(e),
                    None => return None,
                },
                '"' => {
                    // drop a datatype or language tag after the closing quote
                    let rest = &t[i + 1..];
                    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                    return Some((lit, &rest[end..]));
                }
                _ => lit.push(c),
            }
        }
        return None;
    }
    None
}

/// The violations in a SHACL validation report (N-Triples), every one of them.
/// A body that is no report at all is an error, never "conforms".
pub fn parse_report(nt: &str) -> R<Vec<Violation>> {
    let mut by: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut results: BTreeSet<String> = BTreeSet::new();
    let mut is_report = false;
    for line in nt.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let (s, rest) = nt_term(line).ok_or_else(|| format!("shacl-report-error: not N-Triples: {line}"))?;
        let (p, rest) = nt_term(rest).ok_or_else(|| format!("shacl-report-error: not N-Triples: {line}"))?;
        let (o, _) = nt_term(rest).ok_or_else(|| format!("shacl-report-error: not N-Triples: {line}"))?;
        if p == format!("{SH}conforms") {
            is_report = true;
        }
        if p == format!("{SH}result") {
            results.insert(o.clone());
        }
        if let Some(local) = p.strip_prefix(SH) {
            by.entry(s).or_default().insert(local.to_string(), o);
        }
    }
    if !is_report {
        return Err("shacl-report-error: the store's answer is not a validation report".into());
    }
    Ok(results
        .iter()
        .map(|r| {
            let f = by.get(r).cloned().unwrap_or_default();
            let get = |k: &str| f.get(k).cloned().unwrap_or_default();
            Violation { focus: get("focusNode"), path: get("resultPath"), message: get("resultMessage") }
        })
        .collect())
}

/// Drop scratch graphs older than SCRATCH_TTL_SECS (a killed write leaves one).
pub fn sweep(store: &dyn Store, now: u64) -> R<usize> {
    let graphs = store.select_v(&format!(
        "SELECT DISTINCT ?v WHERE {{ GRAPH ?v {{ }} FILTER(STRSTARTS(STR(?v), \"{SCRATCH_PREFIX}\")) }}"
    ))?;
    let stale: Vec<&String> = graphs
        .iter()
        .filter(|g| {
            g.strip_prefix(SCRATCH_PREFIX)
                .and_then(|t| t.split('-').next())
                .and_then(|n| n.parse::<u64>().ok())
                .map_or(true, |born| now.saturating_sub(born) > SCRATCH_TTL_SECS)
        })
        .collect();
    if !stale.is_empty() {
        store.update(&stale.iter().map(|g| format!("DROP SILENT GRAPH <{g}>")).collect::<Vec<_>>().join(" ;\n"))?;
    }
    Ok(stale.len())
}

/// Run `update` on a scratch copy of `subjects` and refuse it, with every
/// violation, if Jena's SHACL check of the result names any of them. The real
/// store is untouched either way. A store without SHACL passes everything.
pub fn gate(store: &dyn Store, update: &str, subjects: &[String]) -> R<()> {
    if !store.shacl_enabled() || subjects.is_empty() {
        return Ok(());
    }
    let now = epoch();
    let _ = sweep(store, now);
    let scratch = scratch_name(now);
    let values = subjects.iter().map(|s| format!("<{s}>")).collect::<Vec<_>>().join(" ");
    let checked = (|| -> R<Vec<Violation>> {
        // the rows as they are, and what points at them (inverse paths)
        store.update(&format!(
            "INSERT {{ GRAPH <{scratch}> {{ ?s ?p ?o }} }} WHERE {{ VALUES ?x {{ {values} }} \
             {{ GRAPH ?g {{ ?x ?p ?o }} BIND(?x AS ?s) }} UNION {{ GRAPH ?g {{ ?s ?p ?x }} BIND(?x AS ?o) }} \
             FILTER(?g != <{ONTOLOGY_GRAPH}> && !STRSTARTS(STR(?g), \"{SCRATCH_PREFIX}\")) }}"
        ))?;
        // the write itself, on the copy
        store.update(&rewrite_to_scratch(update, &scratch))?;
        // the rows they link to after the write, and the class chain of every type
        store.update(&format!(
            "INSERT {{ GRAPH <{scratch}> {{ ?t ?q ?w }} }} WHERE {{ VALUES ?x {{ {values} }} \
             GRAPH <{scratch}> {{ ?x ?p ?t }} FILTER(isIRI(?t)) GRAPH ?g {{ ?t ?q ?w }} \
             FILTER(?g != <{ONTOLOGY_GRAPH}> && !STRSTARTS(STR(?g), \"{SCRATCH_PREFIX}\")) }} ;\n\
             INSERT {{ GRAPH <{scratch}> {{ ?a <{RDFS_SUBCLASS}> ?b }} }} WHERE {{ \
             GRAPH <{scratch}> {{ ?n a ?c }} GRAPH <{ONTOLOGY_GRAPH}> {{ ?c <{RDFS_SUBCLASS}>* ?a . ?a <{RDFS_SUBCLASS}> ?b }} }}"
        ))?;
        let wanted: BTreeSet<&str> = subjects.iter().map(String::as_str).collect();
        Ok(parse_report(&store.shacl(&scratch)?)?.into_iter().filter(|v| wanted.contains(v.focus.as_str())).collect())
    })();
    let dropped = store.update(&format!("DROP SILENT GRAPH <{scratch}>"));
    let violations = checked?;
    dropped?;
    if violations.is_empty() {
        return Ok(());
    }
    let list = violations
        .iter()
        .map(|v| {
            let path = v.path.rsplit(['#', '/']).next().unwrap_or(&v.path);
            format!("{path}: {}", v.message)
        })
        .collect::<Vec<_>>()
        .join("; ");
    witness("model.refused", &[("reason", "shape-violation"), ("count", violations.len().to_string().as_str()), ("violations", list.as_str())]);
    Err(format!("shape-violation: {} violation(s): {}", violations.len(), list))
}

#[cfg(test)]
mod tests {
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
    struct Shacl { report: R<String>, graphs: Vec<String>, updates: RefCell<Vec<String>> }
    impl Store for Shacl {
        fn ask(&self, _: &str) -> R<bool> { Ok(false) }
        fn select_v(&self, _: &str) -> R<Vec<String>> { Ok(self.graphs.clone()) }
        fn update(&self, s: &str) -> R<()> { self.updates.borrow_mut().push(s.to_string()); Ok(()) }
        fn shacl_enabled(&self) -> bool { true }
        fn shacl(&self, _: &str) -> R<String> { self.report.clone() }
    }
    fn store(report: R<String>) -> Shacl { Shacl { report, graphs: vec![], updates: RefCell::new(vec![]) } }
    const WRITE: &str = "INSERT DATA { GRAPH <urn:chorus:instances:alerts> { <https://jeffbridwell.com/chorus#alert-a> <https://jeffbridwell.com/chorus#platform> \"x\" } }";

    #[test]
    fn a_write_is_pointed_at_the_scratch_and_the_ontology_is_left_alone() {
        let w = "DELETE { GRAPH <urn:g:a> { ?s ?p ?o } } INSERT { GRAPH  <urn:g:b> { ?s ?p 1 } } WHERE { GRAPH <urn:chorus:ontology> { ?c ?d ?e } GRAPH ?g { ?s ?p ?o } } ; # NOGRAPH <urn:x>";
        let r = rewrite_to_scratch(w, "urn:s");
        assert_eq!(r, "DELETE { GRAPH <urn:s> { ?s ?p ?o } } INSERT { GRAPH  <urn:s> { ?s ?p 1 } } WHERE { GRAPH <urn:chorus:ontology> { ?c ?d ?e } GRAPH ?g { ?s ?p ?o } } ; # NOGRAPH <urn:x>");
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
        let s = store(Ok(report(&[(ROW, "urn:p#a", "need 1"), (ROW, "urn:p#b", "need bool"), (ROW, "urn:p#c", "need Platform")])));
        let e = gate(&s, WRITE, &[ROW.to_string()]).unwrap_err();
        assert!(e.starts_with("shape-violation: 3 violation(s): a: need 1; b: need bool; c: need Platform"), "{e}");
        let u = s.updates.borrow();
        assert!(u.iter().all(|x| !x.contains("GRAPH <urn:chorus:instances:alerts>")), "the real graph was written: {u:?}");
        assert!(u.last().unwrap().starts_with(&format!("DROP SILENT GRAPH <{SCRATCH_PREFIX}")), "scratch not dropped: {u:?}");
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
}
