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
    pub component: String,
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
    // Abby 10-24 (demo review): a value that contains the text `GRAPH <x>` is a
    // literal, not a graph; only the text outside string literals is rewritten
    let mut out = String::with_capacity(update.len());
    let mut code = String::new();
    let mut chars = update.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '"' {
            out.push_str(&rewrite_code(&std::mem::take(&mut code), scratch));
            out.push(c);
            while let Some(d) = chars.next() {
                out.push(d);
                if d == '\\' {
                    if let Some(e) = chars.next() { out.push(e); }
                } else if d == '"' {
                    break;
                }
            }
        } else {
            code.push(c);
        }
    }
    out.push_str(&rewrite_code(&code, scratch));
    out
}

fn rewrite_code(update: &str, scratch: &str) -> String {
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
            Violation { focus: get("focusNode"), path: get("resultPath"), component: get("sourceConstraintComponent"), message: get("resultMessage") }
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

fn describe(vs: &[Violation]) -> String {
    vs.iter()
        .map(|v| format!("{}: {}", v.path.rsplit(['#', '/']).next().unwrap_or(&v.path), v.message))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Run `update` on a scratch copy of `subjects` and refuse it, with every
/// violation the write ADDS, if Jena's SHACL check of the result names any of
/// them; violations the rows already had are returned as a warning. The real
/// store is untouched either way. A store without SHACL passes everything.
pub fn gate(store: &dyn Store, update: &str, subjects: &[String]) -> R<()> {
    if !store.shacl_enabled() || subjects.is_empty() {
        return Ok(());
    }
    let now = epoch();
    let _ = sweep(store, now);
    let scratch = scratch_name(now);
    let values = subjects.iter().map(|s| format!("<{s}>")).collect::<Vec<_>>().join(" ");
    let wanted: BTreeSet<&str> = subjects.iter().map(String::as_str).collect();
    // the rows they link to, and the class chain of every type, as the scratch holds them now
    let targets = format!(
        "INSERT {{ GRAPH <{scratch}> {{ ?t ?q ?w }} }} WHERE {{ VALUES ?x {{ {values} }} \
         GRAPH <{scratch}> {{ ?x ?p ?t }} FILTER(isIRI(?t)) GRAPH ?g {{ ?t ?q ?w }} \
         FILTER(?g != <{ONTOLOGY_GRAPH}> && !STRSTARTS(STR(?g), \"{SCRATCH_PREFIX}\")) }} ;\n\
         INSERT {{ GRAPH <{scratch}> {{ ?a <{RDFS_SUBCLASS}> ?b }} }} WHERE {{ \
         GRAPH <{scratch}> {{ ?n a ?c }} GRAPH <{ONTOLOGY_GRAPH}> {{ ?c <{RDFS_SUBCLASS}>* ?a . ?a <{RDFS_SUBCLASS}> ?b }} }}"
    );
    // the marker keeps the graph in being when the rows do not exist yet (a new
    // row's before-check): Jena answers 404 for a graph with no triples
    let copy = format!(
        "INSERT DATA {{ GRAPH <{scratch}> {{ <{scratch}> a <urn:chorus:ShaclScratch> }} }} ;\n\
         INSERT {{ GRAPH <{scratch}> {{ ?s ?p ?o }} }} WHERE {{ VALUES ?x {{ {values} }} \
         {{ GRAPH ?g {{ ?x ?p ?o }} BIND(?x AS ?s) }} UNION \
         {{ GRAPH <{ONTOLOGY_GRAPH}> {{ ?ib <{SH}inversePath> ?p }} GRAPH ?g {{ ?s ?p ?x }} BIND(?x AS ?o) }} \
         FILTER(?g != <{ONTOLOGY_GRAPH}> && !STRSTARTS(STR(?g), \"{SCRATCH_PREFIX}\")) }}"
    );
    let checked = (|| -> R<(Vec<Violation>, Vec<Violation>)> {
        // the rows as they are, and what points at them through a predicate a
        // shape reads backwards (sh:inversePath) — only those: Kade 10-24, a
        // principal has 109,833 edges pointing at it on prod; the
        // write applied to the copy; then whatever the rows link to
        store.update(&copy)?;
        store.update(&rewrite_to_scratch(update, &scratch))?;
        store.update(&targets)?;
        // the shapes for the rows' classes after the write, read at this write
        // (keyed on the shapes graph's hash, so a shape edit applies to the next
        // write with no restart: AC3)
        let classes = store.select_v(&format!(
            "SELECT DISTINCT ?v WHERE {{ VALUES ?x {{ {values} }} GRAPH <{scratch}> {{ ?x a ?v }} FILTER(isIRI(?v)) }}"
        ))?;
        let shapes = store.shapes(&classes)?;
        let report = || -> R<Vec<Violation>> {
            Ok(parse_report(&store.shacl(&scratch, &shapes)?)?.into_iter().filter(|v| wanted.contains(v.focus.as_str())).collect())
        };
        let after = report()?;
        if after.is_empty() {
            return Ok((vec![], after));
        }
        // only a write that leaves a violation pays for the second check: the
        // rows as they were (Wren 10-24: a dirty row's edit is refused only for
        // what the edit adds; the rest goes back as a warning)
        store.update(&format!("DROP SILENT GRAPH <{scratch}>"))?;
        store.update(&copy)?;
        store.update(&targets)?;
        Ok((report()?, after))
    })();
    let dropped = store.update(&format!("DROP SILENT GRAPH <{scratch}>"));
    let (before, after) = checked?;
    dropped?;
    // one constraint broken on one path of one row; the message is left out, as
    // Jena's names the value and an edit that keeps a row broken the same way
    // (an sh:in outcome still not in the list) adds nothing
    let key = |v: &Violation| (v.focus.clone(), v.path.clone(), v.component.clone());
    let had: BTreeSet<_> = before.iter().map(key).collect();
    let (kept, violations): (Vec<Violation>, Vec<Violation>) = after.into_iter().partition(|v| had.contains(&key(v)));
    if !kept.is_empty() {
        let w = describe(&kept);
        eprintln!("shape-warning: {} violation(s) the row already had: {}", kept.len(), w);
        witness("model.warned", &[("reason", "shape-violation-kept"), ("count", kept.len().to_string().as_str()), ("violations", w.as_str())]);
    }
    if violations.is_empty() {
        return Ok(());
    }
    let list = describe(&violations);
    witness("model.refused", &[("reason", "shape-violation"), ("count", violations.len().to_string().as_str()), ("violations", list.as_str())]);
    Err(format!("shape-violation: {} violation(s): {}", violations.len(), list))
}

#[cfg(test)]
#[path = "shacl_tests.rs"]
mod tests;
