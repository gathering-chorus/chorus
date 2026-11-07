// @domain: domains — placed by chorus-crawl's domain rules (#4538)
//! The six checks carried over from the bash sweep.
//!
//! #4167 AC2. Ran both against the same live graph on 2026-09-19 17:49 and
//! diffed: the bash found 65 lines across six kinds, the Rust found 12,891
//! across the two NEW kinds and not one of the six. The rewrite had replaced
//! nothing. So these are ports, query for query, and the bash cannot be deleted
//! until they produce the same kinds on the same graph.
//!
//! Each query is a straight carry-over, with one change made on purpose: the
//! bash asked for a sample (`LIMIT 20`) and a separate COUNT, then printed the
//! count. That is two queries that can disagree, and it is why a finding could
//! be reported with a number nobody could trace to rows. Here one query returns
//! the rows and the count is their length.

use crate::checks::Check;

const NS: &str = "https://jeffbridwell.com/chorus#";

/// An edge pointing at a chorus IRI that is never itself a subject — a
/// reference to something that does not exist.
pub const DANGLING_EDGE: Check = Check {
    id: "dangling-edge",
    question: "does every edge point at something that exists",
    query: r#"PREFIX c: <https://jeffbridwell.com/chorus#>
SELECT ?s ?p WHERE {
  GRAPH ?g {
    ?s ?p ?o .
    FILTER(?p != <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>)
    FILTER(isIRI(?o) && STRSTARTS(STR(?o), "https://jeffbridwell.com/chorus#"))
    FILTER NOT EXISTS { GRAPH ?og { ?o ?anyp ?anyo } }
  }
  FILTER(STRSTARTS(STR(?g), "urn:chorus:"))
}"#,
};

/// A chorus subject carrying data but no rdf:type — a row nothing can serve,
/// because every generated route selects by class.
pub const UNTYPED_INSTANCE: Check = Check {
    id: "untyped-instance",
    question: "does every row say what it is",
    query: r#"PREFIX c: <https://jeffbridwell.com/chorus#>
SELECT DISTINCT ?s WHERE {
  GRAPH ?g {
    ?s ?p ?o .
    FILTER(STRSTARTS(STR(?s), "https://jeffbridwell.com/chorus#"))
    FILTER NOT EXISTS { ?s a ?t }
  }
  FILTER(STRSTARTS(STR(?g), "urn:chorus:"))
}"#,
};

/// Rows still living in the two v1 graphs. Jeff's standing ask: every row in its
/// own domain graph, never the catch-all and never the schema graph.
pub const V1_ROW: Check = Check {
    id: "v1-row",
    question: "is any row still in the catch-all or the schema graph",
    query: r#"SELECT ?s ?c WHERE {
  VALUES ?g { <urn:chorus:instances> <urn:chorus:ontology> }
  # #4239 — this check names its two graphs, so it would sweep them whatever the
  # scope said. A check that ignores the scope silently is worse than no scope:
  # it makes a scoped run look like a whole-store run. Scoped elsewhere, this
  # check correctly has nothing to say.
  FILTER(STRSTARTS(STR(?g), "urn:chorus:"))
  GRAPH ?g { ?s a ?c }
  # #4091 — a ToolInput is a NodeShape (a tool's argument schema), so it is
  # schema, not a row: it lives in the schema graph by design. Each new one
  # read as a v1 row and turned a card's model land red.
  FILTER NOT EXISTS { GRAPH ?g { ?s a <http://www.w3.org/ns/shacl#NodeShape> } }
  # #4744 — an Operation (a product's live computation: route, home domain, handler) is
  # a generator input like a ToolInput: athena-gen reads it from the model files, so it
  # lives in the schema graph for now. DEBT (Kade's review): unlike a ToolInput it is a
  # row, not schema; the fix is athena-gen reading Operation rows from their domain
  # graph, then this exemption goes. Anywhere else it is still a row.
  FILTER NOT EXISTS { FILTER(?g = <urn:chorus:ontology>) GRAPH ?g { ?s a <https://jeffbridwell.com/chorus#Operation> } }
  FILTER(!STRSTARTS(STR(?c), "http://www.w3.org/2002/07/owl#")
      && !STRSTARTS(STR(?c), "http://www.w3.org/ns/shacl#")
      && !STRSTARTS(STR(?c), "http://www.w3.org/2000/01/rdf-schema#"))
}"#,
};

/// An ownedBy whose object is not an existing Principal — a row owned by
/// nobody, or by a name rather than a person.
pub const OWNER_NOT_PRINCIPAL: Check = Check {
    id: "owner-not-principal",
    question: "is every row's owner a principal that exists",
    query: r#"PREFIX c: <https://jeffbridwell.com/chorus#>
SELECT ?s ?o WHERE {
  GRAPH ?g { ?s c:ownedBy ?o }
  FILTER(STRSTARTS(STR(?g), "urn:chorus:"))
  FILTER NOT EXISTS { GRAPH ?pg { ?o a c:Principal } }
}"#,
};

/// A class whose rows carry ownedBy but whose shape never says what an owner is.
/// Section OWNER_NOT_PRINCIPAL counts the bad rows; this counts the silence that
/// lets them in, because the generated door enforces exactly what the shape
/// states and no more.
pub const OWNER_RULE_MISSING: Check = Check {
    id: "owner-rule-missing",
    question: "does every class that has owners say what an owner is",
    query: r#"PREFIX c: <https://jeffbridwell.com/chorus#>
PREFIX sh: <http://www.w3.org/ns/shacl#>
SELECT DISTINCT ?cls WHERE {
  GRAPH ?g { ?s c:ownedBy ?o ; a ?cls }
  FILTER NOT EXISTS {
    GRAPH ?sg { ?sh sh:targetClass ?cls ; sh:property ?p . ?p sh:path c:ownedBy ; sh:class c:Principal }
  }
}"#,
};

/// A subject living in more than one graph. Two homes means two answers to the
/// same question, and which one a reader gets depends on which route they came
/// through.
///
/// #4336 — the line names the graphs, not only their count: "pulse is in 2
/// graphs" is not actionable, "pulse is in A and B" is. The retired bash printed
/// them; the #4167 port dropped them.
pub const ONE_HOME: Check = Check {
    id: "one-home",
    question: "does every row live in exactly one graph",
    query: r#"PREFIX c: <https://jeffbridwell.com/chorus#>
SELECT ?s (COUNT(DISTINCT ?h) AS ?homes) (GROUP_CONCAT(DISTINCT STR(?h); separator=" ") AS ?graphs) WHERE {
  GRAPH ?g { ?s a ?t }
  FILTER(STRSTARTS(STR(?g), "urn:chorus:"))
  FILTER(STRSTARTS(STR(?s), "https://jeffbridwell.com/chorus#"))
  # #4467 — the homes are counted in ?h, not ?g. A scoped run binds ?g to the
  # card's graphs; counting ?g would then see one home for a row that has two.
  GRAPH ?h { ?s a ?t2 }
  FILTER(STRSTARTS(STR(?h), "urn:chorus:"))
}
GROUP BY ?s
HAVING(COUNT(DISTINCT ?h) > 1)"#,
};

/// Every check the sweep runs against the store, in report order.
pub fn all() -> Vec<&'static Check> {
    vec![
        &V1_ROW,
        &DANGLING_EDGE,
        &UNTYPED_INSTANCE,
        &ONE_HOME,
        &OWNER_NOT_PRINCIPAL,
        &OWNER_RULE_MISSING,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The six kinds the bash reported on 2026-09-19 17:49. If a rename or a
    /// deletion changes one, this fails — the report format is a contract with
    /// /borg/graph-validate.html, which is not modified by this card.
    #[test]
    fn the_six_bash_kinds_are_all_present() {
        let ids: Vec<&str> = all().iter().map(|c| c.id).collect();
        for expected in [
            "v1-row",
            "dangling-edge",
            "untyped-instance",
            "one-home",
            "owner-not-principal",
            "owner-rule-missing",
        ] {
            assert!(ids.contains(&expected), "{expected} is missing from the registry");
        }
    }

    /// NEGATIVE PROOF (#3734): the state this file exists to catch is the one
    /// found by running both sweeps — a Rust verb that reports NONE of the
    /// bash's kinds while claiming to replace it. Drop any kind from the
    /// registry and the test above goes red; this asserts the registry is not
    /// silently empty, which is the same failure one size larger.
    #[test]
    fn negative_proof_an_empty_registry_is_not_a_passing_sweep() {
        assert!(!all().is_empty());
        assert_eq!(all().len(), 6, "a check was added or lost without updating the port list");
    }

    /// The bash ran a sample query and a separate COUNT, so the number it
    /// printed could not be traced to the rows it showed. One query, rows only.
    #[test]
    fn every_ported_check_returns_rows_not_counts() {
        for c in all() {
            assert!(!c.query.contains("COUNT(*)"), "{} counts instead of listing rows", c.id);
            assert!(c.query.contains("SELECT"), "{} has no SELECT", c.id);
            assert!(!c.query.contains("LIMIT 20"), "{} still carries the bash sample limit", c.id);
        }
    }

    /// #4091 — run V1_ROW on a TriG fixture with Jena's arq (required, never skipped).
    fn v1_rows(trig: &str) -> String {
        let dir = std::env::temp_dir().join(format!("av-v1row-{}-{}", std::process::id(), trig.len()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("fx.trig"), trig).unwrap();
        std::fs::write(dir.join("q.rq"), V1_ROW.query).unwrap();
        let out = std::process::Command::new("arq")
            .arg("--data").arg(dir.join("fx.trig")).arg("--query").arg(dir.join("q.rq")).arg("--results").arg("csv")
            .output().expect("arq (Apache Jena) is required: brew install jena");
        assert!(out.status.success(), "arq failed: {}", String::from_utf8_lossy(&out.stderr));
        let body = String::from_utf8_lossy(&out.stdout).to_string();
        assert!(body.starts_with("s,"), "the query did not run: {body:?}");
        body
    }

    #[test]
    fn a_tool_input_shape_in_the_schema_graph_is_not_a_v1_row() {
        let body = v1_rows(r#"@prefix c: <https://jeffbridwell.com/chorus#> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
<urn:chorus:ontology> { c:ConvergenceLandAlertInput a sh:NodeShape , c:ToolInput . }"#);
        assert!(!body.contains("ConvergenceLandAlertInput"), "{body}");
    }

    /// NEGATIVE PROOF: a real row in the schema graph is still caught.
    #[test]
    fn a_row_in_the_schema_graph_is_still_a_v1_row() {
        let body = v1_rows(r#"@prefix c: <https://jeffbridwell.com/chorus#> .
<urn:chorus:ontology> { c:tool-x a c:Tool . }
<urn:chorus:instances> { c:card-1 a c:Card . }"#);
        assert!(body.contains("tool-x") && body.contains("card-1"), "{body}");
    }

    #[test]
    fn an_operation_in_the_schema_graph_is_not_a_v1_row_4744() {
        let body = v1_rows(r#"@prefix c: <https://jeffbridwell.com/chorus#> .
<urn:chorus:ontology> { c:tree a c:Operation . }"#);
        assert!(!body.contains("tree"), "{body}");
    }

    /// NEGATIVE PROOF: an Operation in the catch-all graph is still a v1 row.
    #[test]
    fn an_operation_in_the_catch_all_graph_is_still_a_v1_row_4744() {
        let body = v1_rows(r#"@prefix c: <https://jeffbridwell.com/chorus#> .
<urn:chorus:instances> { c:stray a c:Operation . }"#);
        assert!(body.contains("stray"), "{body}");
    }

    #[test]
    fn the_namespace_is_the_chorus_one() {
        assert!(NS.ends_with('#'));
        for c in all() {
            assert!(c.query.contains("chorus"), "{} does not mention the chorus namespace", c.id);
        }
    }
}
