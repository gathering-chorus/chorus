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
        // #4358 — shorten each column that IS an IRI, and only those. Shortening
        // the joined detail cut everything before its last '#' or '/', which ate
        // the regex and the value of a pattern finding ("#card-[0-9]+$" → "card-…").
        let detail = cols
            .map(|c| {
                let c = c.trim();
                if is_iri(c) { short(c) } else { c.to_string() }
            })
            .collect::<Vec<_>>()
            .join(",");
        Finding {
            check: check_id.to_string(),
            subject: short(&subject),
            detail,
        }
    })
    .collect()
}

fn is_iri(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://") || s.starts_with("urn:")
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

/// #4467 — exact graphs, named one by one: `ATHENA_VALIDATE_GRAPHS="urn:a urn:b"`.
///
/// The prefix scope above saves no time, because Fuseki walks every graph and
/// filters afterwards. On 10.7M triples (10.0M of them test results) three whole-store
/// checks hit the 180s limit, so every land's prove step said UNMEASURED. A land
/// only needs to prove the graphs its model feeds. Binding `?g` with VALUES before
/// the query runs means Fuseki reads only those graphs: the cards graph (46k
/// triples) answers dangling-edge in 0.4s.
///
/// Unset is None (the old behaviour). Set but empty is Some(empty): the card's
/// model feeds no instance graph, and the store checks say so instead of running.
/// A name that is not a plain `urn:chorus:` IRI is an error, never a guess,
/// because it is pasted into the query.
pub fn exact_graphs() -> Result<Option<Vec<String>>, String> {
    let Ok(raw) = std::env::var("ATHENA_VALIDATE_GRAPHS") else { return Ok(None) };
    let graphs: Vec<String> = raw.split([' ', ',', '\n']).filter(|g| !g.is_empty()).map(String::from).collect();
    for g in &graphs {
        let plain = g.chars().all(|c| c.is_ascii_alphanumeric() || ":-_.".contains(c));
        if !g.starts_with("urn:chorus:") || !plain {
            return Err(format!("ATHENA_VALIDATE_GRAPHS names '{g}', which is not a urn:chorus: graph"));
        }
    }
    Ok(Some(graphs))
}

/// The scope filter every store check carries. In exact mode it is taken out and
/// `?g` is bound up front instead.
const PREFIX_FILTER: &str = r#"FILTER(STRSTARTS(STR(?g), "urn:chorus:"))"#;

/// Bind `?g` to the exact graphs at the top of the query.
///
/// Only `?g`, the graph a check is ABOUT, is bound. The graphs a check looks
/// things up in (`?g2`, `?og`, `?pg`, `?h`) stay open, so a row in a scoped graph
/// whose field or target lives elsewhere is still judged against the whole store.
/// A check that names its own graphs (v1-row) joins with the binding and has
/// nothing to say when those graphs are out of scope.
pub fn bind_graphs(query: &str, graphs: &[String]) -> String {
    let values = format!(
        "WHERE {{\n  VALUES ?g {{ {} }}",
        graphs.iter().map(|g| format!("<{g}>")).collect::<Vec<_>>().join(" ")
    );
    query.replace(PREFIX_FILTER, "").replacen("WHERE {", &values, 1)
}

/// Substitute the scope into a check's query. The prefix appears only as a
/// quoted literal inside STRSTARTS; the angle-bracket IRIs (`<urn:chorus:instances>`)
/// name specific graphs a check is ABOUT and are deliberately left alone.
fn scoped(query: &str) -> String {
    if let Ok(Some(graphs)) = exact_graphs() {
        return bind_graphs(query, &graphs);
    }
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

/// Seconds one store query may take. 180 unless ATHENA_VALIDATE_TIMEOUT says
/// otherwise, for the daily whole-store sweep, which is not racing a land.
fn timeout_secs() -> String {
    std::env::var("ATHENA_VALIDATE_TIMEOUT")
        .ok()
        .filter(|t| t.parse::<u32>().is_ok_and(|n| n > 0))
        .unwrap_or_else(|| "180".into())
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
        .arg(timeout_secs())
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

    /// NEGATIVE PROOF (#4358): a non-IRI detail column (a regex, a literal)
    /// containing '#' or '/' must survive whole. The old whole-detail shortening
    /// turned "@id,https://…#oops,#card-[0-9]+$,miss" into "card-[0-9]+$,miss".
    #[test]
    fn negative_proof_non_iri_detail_columns_are_not_shortened() {
        let body = "s,field,value,re,outcome\nhttps://jeffbridwell.com/chorus#oops,@id,https://jeffbridwell.com/chorus#oops,#card-[0-9]+$,miss\n";
        let f = parse_csv("row-value-off-pattern", body);
        assert_eq!(f[0].subject, "oops");
        assert_eq!(f[0].detail, "@id,oops,#card-[0-9]+$,miss");
    }

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

    fn arq(tag: &str, trig: &str, query: &str) -> Vec<Finding> {
        let dir = std::env::temp_dir().join(format!("av-{}-{}", tag, std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("fx.trig"), trig).unwrap();
        std::fs::write(dir.join("q.rq"), query).unwrap();
        let out = std::process::Command::new("arq")
            .arg("--data").arg(dir.join("fx.trig")).arg("--query").arg(dir.join("q.rq")).arg("--results").arg("csv")
            .output()
            .expect("arq (Apache Jena) is required for this proof: brew install jena");
        assert!(out.status.success(), "arq failed: {}", String::from_utf8_lossy(&out.stderr));
        parse_csv(tag, &String::from_utf8_lossy(&out.stdout))
    }

    /// #4467: one dangling edge in the card's graph, one in a graph the card does
    /// not touch, and a row whose second home is outside the scope.
    const SCOPE_4467: &str = r##"@prefix c: <https://jeffbridwell.com/chorus#> .
<urn:chorus:domains:skills> { c:skill-a a c:Skill ; c:hasDomain c:gone . c:skill-b a c:Skill . }
<urn:chorus:domains:other> { c:o1 a c:Thing ; c:points c:also-gone . c:skill-b a c:Skill . }
"##;

    fn scoped_to_skills(q: &str) -> String {
        bind_graphs(q, &["urn:chorus:domains:skills".to_string()])
    }

    #[test]
    fn negative_proof_a_scoped_run_finds_the_dangling_edge_in_its_graph_only() {
        let got = arq("4467-dangle", SCOPE_4467, &scoped_to_skills(crate::ported::DANGLING_EDGE.query));
        let subjects: Vec<&str> = got.iter().map(|f| f.subject.as_str()).collect();
        assert_eq!(subjects, vec!["skill-a"], "{got:?}");
    }

    #[test]
    fn the_whole_store_run_still_sees_both_dangling_edges() {
        let got = arq("4467-dangle-all", SCOPE_4467, crate::ported::DANGLING_EDGE.query);
        assert_eq!(got.len(), 2, "{got:?}");
    }

    /// The case the old exact-graph attempt (#4239) got wrong: binding the graph
    /// hid the second home. Homes are counted across the store, so it is found.
    #[test]
    fn negative_proof_a_scoped_run_still_sees_a_second_home_outside_the_scope() {
        let got = arq("4467-home", SCOPE_4467, &scoped_to_skills(crate::ported::ONE_HOME.query));
        assert_eq!(got.len(), 1, "{got:?}");
        assert_eq!(got[0].subject, "skill-b");
        assert!(got[0].detail.starts_with('2'), "{got:?}");
    }

    #[test]
    fn every_store_check_is_bound_when_scoped() {
        let mut checks = crate::ported::all();
        checks.extend([&crate::checks::COMPLETENESS, &crate::checks::ALLOWED_VALUES, &crate::checks::PATTERN]);
        for c in checks {
            let q = scoped_to_skills(c.query);
            assert!(q.contains("VALUES ?g { <urn:chorus:domains:skills> }"), "{} is not bound", c.id);
            assert!(!q.contains(PREFIX_FILTER), "{} still walks every graph", c.id);
        }
    }

    #[test]
    fn negative_proof_a_graph_name_that_is_not_a_chorus_urn_is_refused() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        for bad in ["urn:other:x", "urn:chorus:x> } DROP ALL #"] {
            std::env::set_var("ATHENA_VALIDATE_GRAPHS", bad);
            assert!(exact_graphs().is_err(), "accepted {bad}");
        }
        std::env::set_var("ATHENA_VALIDATE_GRAPHS", "urn:chorus:domains:skills,urn:chorus:domains:cards");
        assert_eq!(exact_graphs().unwrap().unwrap().len(), 2);
        std::env::set_var("ATHENA_VALIDATE_GRAPHS", "");
        assert_eq!(exact_graphs().unwrap(), Some(vec![]));
        std::env::remove_var("ATHENA_VALIDATE_GRAPHS");
        assert_eq!(exact_graphs().unwrap(), None);
    }

    /// Both tests below set and clear FUSEKI_QUERY. Run in parallel, one clears
    /// it mid-run and the other sweeps the LIVE store (seen 2026-09-26: "dead
    /// store reported Found(835)"). Serialise them.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn the_endpoint_is_overridable_for_fixtures() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("FUSEKI_QUERY", "http://127.0.0.1:9/query");
        assert_eq!(query_endpoint(), "http://127.0.0.1:9/query");
        std::env::remove_var("FUSEKI_QUERY");
    }

    /// An unreachable store must be Unmeasured, proven by pointing at a port
    /// nothing listens on rather than by reading the code and agreeing with it.
    #[test]
    fn negative_proof_an_unreachable_store_is_unmeasured() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("FUSEKI_QUERY", "http://127.0.0.1:9/query");
        let (v, f) = run(&COMPLETENESS);
        std::env::remove_var("FUSEKI_QUERY");
        assert!(v.is_unmeasured(), "dead store reported {v:?}");
        assert!(!v.is_dirty());
        assert!(f.is_empty());
    }
}

/// The stored predicates of one subject, by the full IRI the door named for
/// it. None when the store cannot be read — never an empty list, which would
/// make every served row look clean.
pub fn predicates_of(iri: &str) -> Option<Vec<String>> {
    let q = format!("SELECT DISTINCT ?p WHERE {{ GRAPH ?g {{ <{iri}> ?p ?o }} }}");
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
    let mut preds = Vec::new();
    for l in lines {
        let p = short(l.trim());
        if !p.is_empty() && !preds.contains(&p) { preds.push(p); }
    }
    Some(preds)
}
