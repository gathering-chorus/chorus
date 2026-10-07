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
    query: r##"PREFIX c: <https://jeffbridwell.com/chorus#>
PREFIX sh: <http://www.w3.org/ns/shacl#>
SELECT ?s ?cls ?field WHERE {
  GRAPH <urn:chorus:ontology> {
    ?shape sh:targetClass ?cls ; sh:property ?p .
    ?p sh:path ?path ; sh:minCount ?mc .
    FILTER(?mc > 0)
    OPTIONAL { ?path sh:inversePath ?inv }
  }
  GRAPH ?g { ?s a ?cls }
  FILTER(STRSTARTS(STR(?g), "urn:chorus:"))   # #4239 — honour the scope
  FILTER(?g != <urn:chorus:ontology>)
  # #4331 — an inverse path (Domain needs some ^hasDomain) is a blank node, and
  # `?s _:b ?v` never matches, so every Domain, Service and ValueStream read as
  # missing it: 95 of the 835 on 2026-09-26. Ask the inverse the inverse way.
  FILTER(IF(BOUND(?inv),
            NOT EXISTS { GRAPH ?g3 { ?w ?inv ?s } },
            NOT EXISTS { GRAPH ?g2 { ?s ?path ?v } }))
  BIND(IF(BOUND(?inv), CONCAT("^", STRAFTER(STR(?inv), "#")), STR(?path)) AS ?field)
}"##,
};

/// #4358 — a row value outside its property's sh:in list. The DAL refuses such
/// a value at the door; this finds the ones that got in before the door did, or
/// around it. Comparison is by lexical form, the same rule the DAL applies
/// (`allowed.contains(value)` over strings), so the sweep and the door cannot
/// disagree about what "in the list" means.
///
/// The value is looked for across graphs, like COMPLETENESS: a row typed in one
/// graph whose field lives in another is still that row's field.
pub const ALLOWED_VALUES: Check = Check {
    id: "row-value-not-in-allowed-values",
    question: "is every row value one its shape allows",
    query: r##"PREFIX sh: <http://www.w3.org/ns/shacl#>
PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
SELECT DISTINCT ?s ?field ?value WHERE {
  GRAPH <urn:chorus:ontology> {
    ?shape sh:targetClass ?cls ; sh:property ?p .
    ?p sh:path ?path ; sh:in ?list .
    FILTER(isIRI(?path))
  }
  GRAPH ?g { ?s a ?cls }
  FILTER(STRSTARTS(STR(?g), "urn:chorus:"))
  FILTER(?g != <urn:chorus:ontology>)
  GRAPH ?g2 { ?s ?path ?v }
  FILTER(?g2 != <urn:chorus:ontology>)
  FILTER NOT EXISTS {
    GRAPH <urn:chorus:ontology> { ?list rdf:rest*/rdf:first ?allowed }
    FILTER(STR(?allowed) = STR(?v))
  }
  BIND(REPLACE(STR(?path), "^.*[#/]", "") AS ?field)
  BIND(STR(?v) AS ?value)
}"##,
};

/// #4358 — a row value that does not match its property's sh:pattern, and a row
/// IRI that does not match a node-level sh:pattern on its class's shape (standard
/// SHACL: a NodeShape's pattern tests STR(focus node)). Node findings carry the
/// field `@id`.
///
/// A malformed regex in the model must not read as clean. In SPARQL a REGEX
/// with a bad pattern is an expression error, and an error inside FILTER drops
/// the row — every row, silently: the vacuous pass #3734 forbids. So the
/// outcome is BOUND first and an error becomes the outcome `regex-error`, which
/// is reported like a miss. A broken pattern turns the sweep red, loudly.
pub const PATTERN: Check = Check {
    id: "row-value-off-pattern",
    question: "does every row value, and every row IRI, match its shape's sh:pattern",
    query: r##"PREFIX sh: <http://www.w3.org/ns/shacl#>
SELECT DISTINCT ?s ?field ?value ?re ?outcome WHERE {
  {
    GRAPH <urn:chorus:ontology> {
      ?shape sh:targetClass ?cls ; sh:property ?p .
      ?p sh:path ?path ; sh:pattern ?re .
      FILTER(isIRI(?path))
      OPTIONAL { ?p sh:flags ?fl }
    }
    GRAPH ?g { ?s a ?cls }
    FILTER(STRSTARTS(STR(?g), "urn:chorus:"))
    FILTER(?g != <urn:chorus:ontology>)
    GRAPH ?g2 { ?s ?path ?v }
    FILTER(?g2 != <urn:chorus:ontology>)
    BIND(REPLACE(STR(?path), "^.*[#/]", "") AS ?field)
  } UNION {
    GRAPH <urn:chorus:ontology> {
      ?shape sh:targetClass ?cls ; sh:pattern ?re .
      OPTIONAL { ?shape sh:flags ?fl }
    }
    GRAPH ?g { ?s a ?cls }
    FILTER(STRSTARTS(STR(?g), "urn:chorus:"))
    FILTER(?g != <urn:chorus:ontology>)
    BIND(?s AS ?v)
    BIND("@id" AS ?field)
  }
  BIND(COALESCE(IF(REGEX(STR(?v), STR(?re), COALESCE(STR(?fl), "")), "match", "miss"), "regex-error") AS ?outcome)
  FILTER(?outcome != "match")
  BIND(STR(?v) AS ?value)
}"##,
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

    /// Run a check's query against a TriG fixture with Jena's `arq` and return
    /// (subject, detail) pairs. Without `arq` this fails — it does not skip.
    fn run_on_fixture(check: &Check, tag: &str, trig: &str) -> Vec<(String, String)> {
        let dir = std::env::temp_dir().join(format!("av-{}-{}", tag, std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let data = dir.join("fx.trig");
        let query = dir.join("q.rq");
        std::fs::write(&data, trig).unwrap();
        std::fs::write(&query, check.query).unwrap();
        let out = std::process::Command::new("arq")
            .arg("--data").arg(&data).arg("--query").arg(&query).arg("--results").arg("csv")
            .output()
            .expect("arq (Apache Jena) is required for this proof: brew install jena");
        assert!(out.status.success(), "arq failed: {}", String::from_utf8_lossy(&out.stderr));
        let body = String::from_utf8_lossy(&out.stdout).to_string();
        // A header proves the query ran; without one a finding-free result would
        // be indistinguishable from a query that never executed.
        assert!(body.lines().next().is_some_and(|h| h.starts_with("s,")), "no header: {body:?}");
        crate::store::parse_csv(check.id, &body)
            .iter()
            .map(|f| (f.subject.clone(), f.detail.clone()))
            .collect()
    }

    const SHAPES_4358: &str = r##"@prefix c: <https://jeffbridwell.com/chorus#> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
<urn:chorus:ontology> {
  c:CardShape sh:targetClass c:Card ;
    sh:pattern "#card-[0-9]+$" ;
    sh:property [ sh:path c:state ; sh:in ( "open" "done" ) ] ;
    sh:property [ sh:path c:version ; sh:pattern "^v[0-9]+$" ] ;
    sh:property [ sh:path c:code ; sh:pattern "^ab$" ; sh:flags "i" ] .
}
"##;

    /// The ALLOWED_VALUES positive half: rows whose values are all in the list
    /// produce no findings — so a red on the negative proof is about the value.
    #[test]
    fn allowed_values_is_clean_when_every_value_is_in_the_list() {
        let fx = format!("{SHAPES_4358}<urn:chorus:domains:cards> {{ <https://jeffbridwell.com/chorus#card-1> a <https://jeffbridwell.com/chorus#Card> ; <https://jeffbridwell.com/chorus#state> \"open\" . }}\n");
        assert_eq!(run_on_fixture(&ALLOWED_VALUES, "4358-in-ok", &fx), vec![]);
    }

    /// NEGATIVE PROOF (#3734 / #4358), as a real query: card-2's state is
    /// "ajar", not in ( "open" "done" ). The check must name card-2 and never
    /// card-1. A value held in a second graph is still checked.
    #[test]
    fn negative_proof_allowed_values_names_a_value_outside_sh_in() {
        let fx = format!(r##"{SHAPES_4358}<urn:chorus:domains:cards> {{
  <https://jeffbridwell.com/chorus#card-1> a <https://jeffbridwell.com/chorus#Card> ; <https://jeffbridwell.com/chorus#state> "done" .
  <https://jeffbridwell.com/chorus#card-2> a <https://jeffbridwell.com/chorus#Card> .
}}
<urn:chorus:domains:other> {{ <https://jeffbridwell.com/chorus#card-2> <https://jeffbridwell.com/chorus#state> "ajar" . }}
"##);
        let got = run_on_fixture(&ALLOWED_VALUES, "4358-in", &fx);
        assert_eq!(got, vec![("card-2".to_string(), "state,ajar".to_string())], "{got:?}");
    }

    #[test]
    fn pattern_is_clean_when_every_value_and_iri_matches() {
        let fx = format!(r##"{SHAPES_4358}<urn:chorus:domains:cards> {{
  <https://jeffbridwell.com/chorus#card-1> a <https://jeffbridwell.com/chorus#Card> ;
    <https://jeffbridwell.com/chorus#version> "v12" ; <https://jeffbridwell.com/chorus#code> "AB" .
}}
"##);
        assert_eq!(run_on_fixture(&PATTERN, "4358-re-ok", &fx), vec![]);
    }

    /// NEGATIVE PROOF (#3734 / #4358), as a real query: card-2's version is off
    /// the property pattern, and row `oops` is off the node pattern. Both are
    /// named; card-1 (on both patterns, code "AB" matching under flag i) never.
    #[test]
    fn negative_proof_pattern_names_values_and_iris_off_their_patterns() {
        let fx = format!(r##"{SHAPES_4358}<urn:chorus:domains:cards> {{
  <https://jeffbridwell.com/chorus#card-1> a <https://jeffbridwell.com/chorus#Card> ;
    <https://jeffbridwell.com/chorus#version> "v1" ; <https://jeffbridwell.com/chorus#code> "Ab" .
  <https://jeffbridwell.com/chorus#card-2> a <https://jeffbridwell.com/chorus#Card> ; <https://jeffbridwell.com/chorus#version> "1.0" .
  <https://jeffbridwell.com/chorus#oops> a <https://jeffbridwell.com/chorus#Card> .
}}
"##);
        let mut got = run_on_fixture(&PATTERN, "4358-re", &fx);
        got.sort();
        assert_eq!(
            got,
            vec![
                ("card-2".to_string(), "version,1.0,^v[0-9]+$,miss".to_string()),
                ("oops".to_string(), "@id,oops,#card-[0-9]+$,miss".to_string()),
            ],
            "{got:?}"
        );
    }

    /// NEGATIVE PROOF: a malformed regex in the model must turn the sweep red,
    /// never clean. In SPARQL a bad REGEX is an expression error that FILTER
    /// would silently drop; the check reports it as `regex-error` instead.
    #[test]
    fn negative_proof_a_malformed_model_regex_is_a_finding_not_clean() {
        let fx = r##"@prefix c: <https://jeffbridwell.com/chorus#> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
<urn:chorus:ontology> { c:CardShape sh:targetClass c:Card ; sh:property [ sh:path c:version ; sh:pattern "^v[0-9+$" ] . }
<urn:chorus:domains:cards> { c:card-1 a c:Card ; c:version "v1" . }
"##;
        let got = run_on_fixture(&PATTERN, "4358-bad", fx);
        assert_eq!(got.len(), 1, "{got:?}");
        assert_eq!(got[0].0, "card-1");
        assert!(got[0].1.ends_with(",regex-error"), "{got:?}");
    }

    /// Both new checks select rows, not counts, and honour the graph scope.
    #[test]
    fn new_checks_select_rows_and_carry_the_scope_literal() {
        for c in [&ALLOWED_VALUES, &PATTERN] {
            assert!(c.query.contains("SELECT DISTINCT ?s"), "{}", c.id);
            assert!(!c.query.contains("COUNT("), "{}", c.id);
            assert!(c.query.contains(r#"STRSTARTS(STR(?g), "urn:chorus:")"#), "{}", c.id);
        }
    }

    /// NEGATIVE PROOF (#4331), run as a real query, not a substring. d1 has a
    /// product pointing at it and a label; d2 has neither. The check must name
    /// d2 twice and d1 never. The old query named d1 too, because an inverse
    /// path is a blank node that `?s ?path ?v` can never match. Needs Jena's
    /// `arq` (brew install jena); without it this fails, it does not skip.
    #[test]
    fn negative_proof_inverse_required_field_is_measured_on_a_fixture() {
        let dir = std::env::temp_dir().join(format!("av-4331-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let data = dir.join("fx.trig");
        let query = dir.join("q.rq");
        std::fs::write(&data, r##"@prefix c: <https://jeffbridwell.com/chorus#> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
<urn:chorus:ontology> {
  c:DomainShape sh:targetClass c:Domain ;
    sh:property [ sh:path [ sh:inversePath c:hasDomain ] ; sh:minCount 1 ] ;
    sh:property [ sh:path rdfs:label ; sh:minCount 1 ] .
}
<urn:chorus:domains:domains> { c:d1 a c:Domain ; rdfs:label "one" . c:d2 a c:Domain . }
<urn:chorus:domains:products> { c:p1 c:hasDomain c:d1 . }
"##).unwrap();
        std::fs::write(&query, COMPLETENESS.query).unwrap();
        let out = std::process::Command::new("arq")
            .arg("--data").arg(&data).arg("--query").arg(&query).arg("--results").arg("csv")
            .output()
            .expect("arq (Apache Jena) is required for this proof: brew install jena");
        assert!(out.status.success(), "arq failed: {}", String::from_utf8_lossy(&out.stderr));
        let findings = crate::store::parse_csv(COMPLETENESS.id, &String::from_utf8_lossy(&out.stdout));
        let got: Vec<(String, String)> = findings.iter().map(|f| (f.subject.clone(), f.detail.clone())).collect();
        assert!(got.contains(&("d2".into(), "Domain,^hasDomain".into())), "{got:?}");
        // #4358 — each IRI column is shortened on its own now, so the class
        // survives beside the field ("Domain,label"); the whole-detail cut used
        // to drop it to "label".
        assert!(got.contains(&("d2".into(), "Domain,label".into())), "{got:?}");
        assert!(got.iter().all(|(s, _)| s != "d1"), "d1 has both and was reported: {got:?}");
        assert_eq!(got.len(), 2, "{got:?}");
    }
}
