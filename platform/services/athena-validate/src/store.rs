//! Running a check against the live store.
//!
//! #4167. curl is a subprocess, never a code dependency (ADR-032 §1), same as
//! the three sibling verbs.
//!
//! The whole point of this module is the distinction the bash could not make:
//! a query that did not run is NOT a query that found nothing. Every early
//! return here is an `Unmeasured` carrying why, and there is no path from a
//! failure to `Clean`.

use crate::checks::{Check, Verdict};
use std::process::Command;

/// Where the store is. Overridable for fixtures; the default is the live store
/// the other verbs use.
pub fn query_endpoint() -> String {
    std::env::var("FUSEKI_QUERY").unwrap_or_else(|_| "http://localhost:3030/pods/query".into())
}

/// One violation, as a row. The report prints these; the count is derived.
#[derive(Debug, PartialEq, Eq)]
pub struct Finding {
    pub check: String,
    pub subject: String,
    pub detail: String,
}

/// Parse a SPARQL CSV response into findings.
///
/// The header line is dropped, blank lines are skipped, and a response with
/// ONLY a header is Clean — that is the one case where "no rows" is a real
/// answer, because the header proves the query ran.
pub fn parse_csv(check_id: &str, body: &str) -> Vec<Finding> {
    let mut rows = body.lines().filter(|l| !l.trim().is_empty());
    // Drop the header; its presence is what tells us the query executed.
    rows.next();
    rows.map(|line| {
        let mut cols = line.split(',');
        let subject = cols.next().unwrap_or("").trim().to_string();
        let detail = cols.collect::<Vec<_>>().join(",").trim().to_string();
        Finding {
            check: check_id.to_string(),
            subject: short(&subject),
            detail: short(&detail),
        }
    })
    .collect()
}

/// IRIs are printed by their local name. A report Jeff has to read is not
/// improved by a namespace repeated on every line.
fn short(iri: &str) -> String {
    iri.rsplit(['#', '/']).next().unwrap_or(iri).to_string()
}

/// Run one check. Any failure to execute is Unmeasured, with the reason.
/// #4239 — the graph scope every store check reads.
///
/// Unset means what it has always meant: every `urn:chorus:` graph. Set, it
/// narrows the sweep to graphs under that prefix, which is what lets a fixture
/// check two rows without reading the whole store (531s of one card's 873s) and
/// what lets a role ask "is THIS domain clean" while fixing it.
///
/// It is applied here, in the one place a check's query is executed, rather than
/// in each query. Four queries carry the prefix today; a fifth added later would
/// otherwise be scoped by whoever remembered, which is how a check ends up
/// honouring a flag in some paths and ignoring it in others.
pub fn graph_scope() -> String {
    std::env::var("ATHENA_VALIDATE_GRAPH").unwrap_or_else(|_| "urn:chorus:".into())
}

/// Substitute the scope into a check's query. The prefix appears only as a
/// quoted literal inside STRSTARTS; the angle-bracket IRIs (`<urn:chorus:instances>`)
/// name specific graphs a check is ABOUT and are deliberately left alone.
fn scoped(query: &str) -> String {
    let scope = graph_scope();
    if scope == "urn:chorus:" {
        return query.to_string();
    }
    // #4239, measured not assumed. Two attempts, both run against a fixture graph
    // holding one untyped row:
    //
    //   prefix filter      13 lines, 287s — finds the violation, saves no time.
    //                      Fuseki walks every graph and filters afterwards.
    //   bind GRAPH <exact>  3 lines,   0s — fast, and it MISSED the violation it
    //                      was pointed at. Clean in zero seconds is the answer a
    //                      broken check gives.
    //
    // So the prefix form ships and the fast form does not. A scope that reports
    // clean because it stopped looking is the defect this crate exists to prevent.
    query.replace("\"urn:chorus:\"", &format!("\"{scope}\""))
}

pub fn run(check: &Check) -> (Verdict, Vec<Finding>) {
    if check.query.is_empty() {
        return (
            Verdict::Unmeasured(format!("{} is not a store query", check.id)),
            vec![],
        );
    }
    let out = Command::new("curl")
        .arg("-sS")
        .arg("--max-time")
        .arg("180")
        .arg("-H")
        .arg("Accept: text/csv")
        .arg("--data-urlencode")
        .arg(format!("query={}", scoped(check.query)))
        .arg(query_endpoint())
        .output();

    let out = match out {
        Ok(o) => o,
        Err(e) => return (Verdict::Unmeasured(format!("curl failed to start: {e}")), vec![]),
    };
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return (
            Verdict::Unmeasured(if why.is_empty() { "store unreachable".into() } else { why }),
            vec![],
        );
    }
    let body = String::from_utf8_lossy(&out.stdout);
    // A body with no header never ran as a query — an empty string here would
    // otherwise parse as zero findings and read as Clean. This is the exact
    // shape that printed PROVEN CLEAN against a dead store.
    if body.trim().is_empty() {
        return (Verdict::Unmeasured("empty response body".into()), vec![]);
    }
    let findings = parse_csv(check.id, &body);
    let verdict = if findings.is_empty() { Verdict::Clean } else { Verdict::Found(findings.len()) };
    (verdict, findings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::COMPLETENESS;

    #[test]
    fn a_header_only_response_is_clean() {
        assert!(parse_csv("x", "s,cls,path\n").is_empty());
    }

    #[test]
    fn rows_become_findings_with_short_names() {
        let body = "s,cls,path\nhttps://jeffbridwell.com/chorus#gathering,Product,docState\n";
        let f = parse_csv("row-missing-required-field", body);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].subject, "gathering");
        assert_eq!(f[0].detail, "Product,docState");
    }

    /// NEGATIVE PROOF (#3734): the state this module exists to prevent is an
    /// empty body reading as a clean graph. A check with no query, and a check
    /// whose response is empty, must both come back Unmeasured — never Clean.
    #[test]
    fn negative_proof_an_empty_body_is_unmeasured_not_clean() {
        let (v, f) = run(&Check { id: "x", question: "q", query: "" });
        assert!(v.is_unmeasured(), "no-query check reported {v:?}");
        assert!(f.is_empty());
        assert_ne!(v.summary_word(), Verdict::Clean.summary_word());
    }

    #[test]
    fn the_endpoint_is_overridable_for_fixtures() {
        std::env::set_var("FUSEKI_QUERY", "http://127.0.0.1:9/query");
        assert_eq!(query_endpoint(), "http://127.0.0.1:9/query");
        std::env::remove_var("FUSEKI_QUERY");
    }

    /// An unreachable store must be Unmeasured, proven by pointing at a port
    /// nothing listens on rather than by reading the code and agreeing with it.
    #[test]
    fn negative_proof_an_unreachable_store_is_unmeasured() {
        std::env::set_var("FUSEKI_QUERY", "http://127.0.0.1:9/query");
        let (v, f) = run(&COMPLETENESS);
        std::env::remove_var("FUSEKI_QUERY");
        assert!(v.is_unmeasured(), "dead store reported {v:?}");
        assert!(!v.is_dirty());
        assert!(f.is_empty());
    }
}

/// One row of a class, with the local names of its stored predicates.
///
/// #4167 — the door check needs a real subject to compare. Sampling is
/// deliberate: which fields a route projects is a property of the ROUTE, so one
/// row finds a dropped field as surely as ten thousand, in a sweep that has to
/// finish. `None` means the class has no rows to sample, which is not a
/// violation — it is nothing to say.
pub fn sample_subject(kind: &str) -> Option<(String, Vec<String>)> {
    let q = format!(
        "PREFIX c: <https://jeffbridwell.com/chorus#> SELECT ?s ?p WHERE {{ GRAPH ?g {{ ?s a c:{kind} ; ?p ?o }} }} LIMIT 60"
    );
    let out = Command::new("curl")
        .arg("-sS").arg("--max-time").arg("60")
        .arg("-H").arg("Accept: text/csv")
        .arg("--data-urlencode").arg(format!("query={q}"))
        .arg(query_endpoint())
        .output().ok()?;
    if !out.status.success() { return None; }
    let body = String::from_utf8_lossy(&out.stdout);
    let mut lines = body.lines().filter(|l| !l.trim().is_empty());
    lines.next()?; // header
    let mut subject: Option<String> = None;
    let mut preds: Vec<String> = Vec::new();
    for line in lines {
        let mut cols = line.split(',');
        let s = cols.next().unwrap_or("").trim().to_string();
        let p = cols.next().unwrap_or("").trim().to_string();
        let s_short = short(&s);
        match &subject {
            None => { subject = Some(s_short.clone()); }
            Some(first) if *first != s_short => continue, // one subject only
            _ => {}
        }
        let p_short = short(&p);
        if !p_short.is_empty() && !preds.contains(&p_short) { preds.push(p_short); }
    }
    subject.map(|s| (s, preds))
}
