//! #4167 — the checks athena-validate runs, as data.
//!
//! Jeff, 2026-09-19: "a job we keep chipping away at until it runs clean."
//! That is the design constraint, not a mood. A job like that needs three
//! properties the bash version did not have:
//!
//!   1. every finding is a ROW, not a line of prose, so the number can go down
//!   2. a query that cannot run is UNMEASURED, never zero — the bash treated a
//!      failed query as "no violations" and printed PROVEN CLEAN against a dead
//!      store (#4166 fixed one instance; a typed Verdict makes the class
//!      unwriteable)
//!   3. every check must reach BOTH states — a check never seen red is not a
//!      check (#3734)
//!
//! The six questions are Jeff's, 2026-09-19 17:31. Four were already swept by
//! the bash. The two here were not, and one of them is how a product with five
//! domains in the store served none of them through the door for months.

/// What a check can conclude. There is no third state meaning "probably fine":
/// either it ran and found rows, it ran and found none, or it could not run —
/// and the last is never silently the second.
#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The query ran; this many violating rows came back.
    Found(usize),
    /// The query ran and the graph satisfies the check.
    Clean,
    /// The query could not run. NOT zero. Carries why.
    Unmeasured(String),
}

impl Verdict {
    /// A run is dirty only on findings. Unmeasured is its own exit, because
    /// "we don't know" must never reach Jeff as "it's fine".
    pub fn is_dirty(&self) -> bool {
        matches!(self, Verdict::Found(n) if *n > 0)
    }

    pub fn is_unmeasured(&self) -> bool {
        matches!(self, Verdict::Unmeasured(_))
    }

    /// The machine-readable summary field, keeping the bash format so
    /// /borg/graph-validate.html needs no change (#4167 AC3).
    pub fn summary_word(&self) -> &'static str {
        match self {
            Verdict::Found(n) if *n > 0 => "dirty",
            Verdict::Found(_) | Verdict::Clean => "clean",
            Verdict::Unmeasured(_) => "unreachable",
        }
    }
}

/// One question the sweep asks of the graph.
pub struct Check {
    /// Stable id, used in the report line and in the nightly diff.
    pub id: &'static str,
    /// The question in Jeff's words, not ours.
    pub question: &'static str,
    /// SPARQL returning ONE ROW PER VIOLATION. Never a COUNT: a count is
    /// derivable from rows, rows are not derivable from a count, and the person
    /// fixing this needs to know WHICH row is wrong.
    pub query: &'static str,
}

/// The required `?path` is looked for ACROSS GRAPHS, not only in the graph the
/// row was found in. Scoping it to one graph is the bug that made the first
/// version of this check return zero against a store already proven dirty — a
/// row judged complete by a graph that could not have held its fields.
pub const COMPLETENESS: Check = Check {
    id: "row-missing-required-field",
    question: "does every row satisfy its shape's required fields",
    query: r#"PREFIX c: <https://jeffbridwell.com/chorus#>
PREFIX sh: <http://www.w3.org/ns/shacl#>
SELECT ?s ?cls ?path WHERE {
  GRAPH <urn:chorus:ontology> {
    ?shape sh:targetClass ?cls ; sh:property ?p .
    ?p sh:path ?path ; sh:minCount ?mc .
    FILTER(?mc > 0)
  }
  GRAPH ?g { ?s a ?cls }
  FILTER(?g != <urn:chorus:ontology>)
  FILTER NOT EXISTS { GRAPH ?g2 { ?s ?path ?v } }
}"#,
};

/// The check nothing has ever run. A row can be in the right graph, carry every
/// required field, and still be invisible: the generated door projects only what
/// the route table says it projects. `werk` held five hasDomain edges in the
/// store on 2026-09-19 and GET /v1/products/products/werk returned no hasDomain
/// key at all — not truncated to one, absent.
///
/// It cannot be answered by SPARQL alone, so the query is empty by design and
/// the comparison runs against the live routes. It is listed beside the others
/// because it asks the same question: is what we hold what we can read.
pub const SURVIVES_THE_DOOR: Check = Check {
    id: "field-dropped-by-the-door",
    question: "is what the store holds what the API returns",
    query: "",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unmeasured_is_never_clean() {
        let u = Verdict::Unmeasured("store unreachable".into());
        assert!(!u.is_dirty());
        assert!(u.is_unmeasured());
        assert_eq!(u.summary_word(), "unreachable");
    }

    /// NEGATIVE PROOF (#3734): the state this type exists to prevent is a failed
    /// query reading as a clean graph. If Unmeasured ever collapses into Clean,
    /// this fails.
    #[test]
    fn negative_proof_unmeasured_does_not_report_clean() {
        let u = Verdict::Unmeasured("timeout".into());
        assert_ne!(u.summary_word(), Verdict::Clean.summary_word());
    }

    #[test]
    fn findings_are_dirty_and_empty_findings_are_not() {
        assert!(Verdict::Found(3).is_dirty());
        assert!(!Verdict::Found(0).is_dirty());
        assert!(!Verdict::Clean.is_dirty());
    }

    /// NEGATIVE PROOF: the completeness query must look for the required field
    /// in ANY graph. Scoping the NOT EXISTS to `?g` is the hollow version that
    /// returned zero while gathering was provably missing docState.
    #[test]
    fn completeness_looks_across_graphs_not_just_the_row_s_own() {
        assert!(COMPLETENESS.query.contains("GRAPH ?g2 { ?s ?path ?v }"));
        assert!(!COMPLETENESS.query.contains("GRAPH ?g { ?s ?path ?v }"));
    }

    /// Findings are rows, not counts.
    #[test]
    fn checks_select_rows_not_counts() {
        assert!(COMPLETENESS.query.contains("SELECT ?s"));
        assert!(!COMPLETENESS.query.contains("COUNT("));
    }
}
