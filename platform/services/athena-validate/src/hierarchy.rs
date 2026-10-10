//! #4481 — the model rule, swept over what is already in the store.
//!
//! Jeff, 2026-10-10, reviewed with Wren: each row stores ONE parent link
//! (value stream → step → domain → service → running unit); step and stream are
//! read up that chain, never stored twice; a link joins two rows of the same
//! hierarchy unless its property says why it may cross. Design:
//! https://claude.ai/artifact/L8xNss5fqT4QjVSBDgxJ2p
//!
//! The rule lives in the model as annotations (chorus:inHierarchy,
//! chorus:parentVia, chorus:parentOptional, chorus:crossesHierarchy,
//! chorus:chainPosition in chorus.ttl). These queries read those annotations
//! instead of naming classes, so changing the rule is a model edit and the
//! check follows it. The one exception is PARENT_LOOP: a SPARQL property path
//! cannot be a variable, so it names the chain's four parent links itself.
//!
//! Every query returns one row per violation, ?s first, like the rest of the
//! crate, and carries the scope filter on ?g, the graph the row lives in.

use crate::checks::Check;

/// A row of a chain class with no parent, or with more than one.
pub const ONE_PARENT: Check = Check {
    id: "row-parent-count",
    question: "does every row in the chain store exactly one parent link",
    query: r##"PREFIX c: <https://jeffbridwell.com/chorus#>
PREFIX sh: <http://www.w3.org/ns/shacl#>
SELECT DISTINCT ?s ?cls ?parents WHERE { {
  {
    SELECT ?s ?cls ?opt (COUNT(DISTINCT ?par) AS ?n) WHERE {
      GRAPH <urn:chorus:ontology> {
        ?cls c:parentVia ?via .
        OPTIONAL { ?cls c:parentOptional ?opt }
      }
      GRAPH ?g { ?s a ?cls }
      FILTER(STRSTARTS(STR(?g), "urn:chorus:"))
      FILTER(?g != <urn:chorus:ontology>)
      # A FILTER on ?inv inside an OPTIONAL sees the OPTIONAL's own ?inv, not the
      # outer one, so the first version counted every inbound link as a parent
      # (chorus-domain: 105). Each branch now joins on ?via itself.
      OPTIONAL { GRAPH ?g2 { ?s ?via ?par } FILTER(?g2 != <urn:chorus:ontology>) }
      OPTIONAL { GRAPH <urn:chorus:ontology> { ?via sh:inversePath ?inv } GRAPH ?g3 { ?par ?inv ?s } FILTER(?g3 != <urn:chorus:ontology>) }
    } GROUP BY ?s ?cls ?opt
  }
  FILTER(?n > 1 || (?n = 0 && !BOUND(?opt)))
  BIND(STR(?n) AS ?parents)
} UNION {
  BIND(<urn:chorus:ontology> AS ?g)
  FILTER(STRSTARTS(STR(?g), "urn:chorus:"))
  FILTER NOT EXISTS { GRAPH <urn:chorus:ontology> { ?any c:parentVia ?v } }
  BIND("model-has-no-parentVia" AS ?s)
} }"##,
};

/// A step or stream stored on a row whose class does not hold it as its parent
/// link: a Service's atStep, a Domain's atStream or primaryStep. Those are read
/// up the chain; a stored copy is how a service came to say "building" while
/// its domain said "Proving" (all 10 hosted services disagreed, 2026-10-10).
pub const STORED_DERIVED: Check = Check {
    id: "row-stores-derived-position",
    question: "is step and stream only ever read up the chain, never stored a second time",
    query: r##"PREFIX c: <https://jeffbridwell.com/chorus#>
SELECT DISTINCT ?s ?cls ?field WHERE { {
  GRAPH <urn:chorus:ontology> { ?p c:chainPosition true }
  GRAPH ?g { ?s a ?cls }
  FILTER(STRSTARTS(STR(?g), "urn:chorus:"))
  FILTER(?g != <urn:chorus:ontology>)
  GRAPH ?g2 { ?s ?p ?o }
  FILTER(?g2 != <urn:chorus:ontology>)
  FILTER NOT EXISTS { GRAPH <urn:chorus:ontology> { ?cls c:parentVia ?p } }
  BIND(REPLACE(STR(?p), "^.*[#/]", "") AS ?field)
} UNION {
  BIND(<urn:chorus:ontology> AS ?g)
  FILTER(STRSTARTS(STR(?g), "urn:chorus:"))
  FILTER NOT EXISTS { GRAPH <urn:chorus:ontology> { ?any c:chainPosition true } }
  BIND("model-has-no-chainPosition" AS ?s)
} }"##,
};

/// A link between rows of two different hierarchies whose property is not on
/// the named list of exceptions.
pub const CROSS_HIERARCHY: Check = Check {
    id: "link-crosses-hierarchy",
    question: "does every link stay inside one hierarchy, unless its type is a named exception",
    query: r##"PREFIX c: <https://jeffbridwell.com/chorus#>
SELECT DISTINCT ?s ?field ?o WHERE { {
  GRAPH <urn:chorus:ontology> {
    ?c1 c:inHierarchy ?h1 .
    ?c2 c:inHierarchy ?h2 .
    FILTER(?h1 != ?h2)
  }
  GRAPH ?g { ?s a ?c1 }
  FILTER(STRSTARTS(STR(?g), "urn:chorus:"))
  FILTER(?g != <urn:chorus:ontology>)
  GRAPH ?g2 { ?s ?p ?o }
  FILTER(?g2 != <urn:chorus:ontology>)
  FILTER(isIRI(?o))
  GRAPH ?g3 { ?o a ?c2 }
  FILTER(?g3 != <urn:chorus:ontology>)
  FILTER NOT EXISTS { GRAPH <urn:chorus:ontology> { ?p c:crossesHierarchy ?why } }
  BIND(REPLACE(STR(?p), "^.*[#/]", "") AS ?field)
} UNION {
  BIND(<urn:chorus:ontology> AS ?g)
  FILTER(STRSTARTS(STR(?g), "urn:chorus:"))
  FILTER NOT EXISTS { GRAPH <urn:chorus:ontology> { ?any c:inHierarchy ?h } }
  BIND("model-has-no-inHierarchy" AS ?s)
} }"##,
};

/// A link type that does not say what it links from or to. Without both, the
/// store accepts a link between any two kinds of thing. The subject is the
/// property itself; ?g binds the ontology graph so the scope filter holds.
pub const UNTYPED_LINK: Check = Check {
    id: "link-type-untyped",
    question: "does every link type say what kind of thing it links from and to",
    query: r##"PREFIX owl: <http://www.w3.org/2002/07/owl#>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
SELECT DISTINCT ?s ?missing WHERE {
  BIND(<urn:chorus:ontology> AS ?g)
  FILTER(STRSTARTS(STR(?g), "urn:chorus:"))
  GRAPH ?g { ?s a owl:ObjectProperty }
  FILTER(STRSTARTS(STR(?s), "https://jeffbridwell.com/chorus#"))
  OPTIONAL { GRAPH ?g { ?s rdfs:domain ?d } }
  OPTIONAL { GRAPH ?g { ?s rdfs:range ?r } }
  FILTER(!BOUND(?d) || !BOUND(?r))
  BIND(IF(!BOUND(?d) && !BOUND(?r), "from,to", IF(!BOUND(?d), "from", "to")) AS ?missing)
}"##,
};

/// A link stored as text: a Domain whose step is the word "Building" instead of
/// the step row, a Product whose consumes is a string (Wren, pulse, 08:41).
/// Text cannot be followed up the chain, so it reads as no link at all.
pub const LINK_AS_LITERAL: Check = Check {
    id: "link-stored-as-text",
    question: "is every link a link to a row, never a piece of text",
    query: r##"PREFIX owl: <http://www.w3.org/2002/07/owl#>
SELECT DISTINCT ?s ?field ?value WHERE {
  GRAPH <urn:chorus:ontology> { ?p a owl:ObjectProperty }
  GRAPH ?g { ?s ?p ?o }
  FILTER(STRSTARTS(STR(?g), "urn:chorus:"))
  FILTER(?g != <urn:chorus:ontology>)
  FILTER(isLiteral(?o))
  BIND(REPLACE(STR(?p), "^.*[#/]", "") AS ?field)
  BIND(STR(?o) AS ?value)
}"##,
};

/// A link whose target is not the kind of thing its type says: a Domain whose
/// step points at a v1 Vertebra row instead of a ValueStreamStep (40 rows on
/// 2026-10-10), or a tag that names a row no graph holds. Typing the links is
/// what makes this checkable; a range of owl:Thing says "anything" and is skipped.
pub const LINK_WRONG_TARGET: Check = Check {
    id: "link-target-wrong-kind",
    question: "does every link point at a row of the kind its type says",
    query: r##"PREFIX owl: <http://www.w3.org/2002/07/owl#>
PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
SELECT DISTINCT ?s ?field ?o WHERE {
  GRAPH <urn:chorus:ontology> { ?p a owl:ObjectProperty ; rdfs:range ?r . FILTER(?r != owl:Thing) }
  GRAPH ?g { ?s ?p ?o }
  FILTER(STRSTARTS(STR(?g), "urn:chorus:"))
  FILTER(?g != <urn:chorus:ontology>)
  FILTER(isIRI(?o))
  FILTER NOT EXISTS {
    # any graph: a class target (definesVocabulary, eventAbout) is typed owl:Class
    # in the ontology graph, and that is where it belongs
    GRAPH ?g2 { ?o a ?t }
    GRAPH <urn:chorus:ontology> {
      ?t rdfs:subClassOf* ?allowed .
      { ?r owl:unionOf/rdf:rest*/rdf:first ?allowed } UNION { FILTER(isIRI(?r)) BIND(?r AS ?allowed) }
    }
  }
  BIND(REPLACE(STR(?p), "^.*[#/]", "") AS ?field)
}"##,
};

/// A parent chain that comes back to where it started. The chain has five
/// levels, so a loop that stays inside it is at most five links long; the query
/// follows one to five parent links. Longer loops would need rows outside the
/// chain's classes, which ONE_PARENT and CROSS_HIERARCHY already report.
pub const PARENT_LOOP: Check = Check {
    id: "parent-chain-loops",
    question: "does every parent chain end at a value stream instead of looping back",
    query: r##"PREFIX c: <https://jeffbridwell.com/chorus#>
SELECT DISTINCT ?s ?hops WHERE {
  GRAPH ?g { ?s ?first ?x1 }
  FILTER(STRSTARTS(STR(?g), "urn:chorus:"))
  FILTER(?g != <urn:chorus:ontology>)
  VALUES ?first { c:runsService c:atStep c:inStream }
  {
    FILTER(?x1 = ?s) BIND("1" AS ?hops)
  } UNION {
    GRAPH ?g2 { ?x1 (c:runsService|^c:hosts|c:atStep|c:inStream) ?s } BIND("2" AS ?hops)
  } UNION {
    GRAPH ?g2 { ?x1 (c:runsService|^c:hosts|c:atStep|c:inStream) ?x2 }
    GRAPH ?g3 { ?x2 (c:runsService|^c:hosts|c:atStep|c:inStream) ?s } BIND("3" AS ?hops)
  } UNION {
    GRAPH ?g2 { ?x1 (c:runsService|^c:hosts|c:atStep|c:inStream) ?x2 }
    GRAPH ?g3 { ?x2 (c:runsService|^c:hosts|c:atStep|c:inStream) ?x3 }
    GRAPH ?g4 { ?x3 (c:runsService|^c:hosts|c:atStep|c:inStream) ?s } BIND("4" AS ?hops)
  }
}"##,
};

pub fn all() -> Vec<&'static Check> {
    vec![&ONE_PARENT, &STORED_DERIVED, &CROSS_HIERARCHY, &UNTYPED_LINK, &LINK_AS_LITERAL, &LINK_WRONG_TARGET, &PARENT_LOOP]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run a check against a TriG fixture with Jena's `arq`. Without `arq` this
    /// fails; it does not skip.
    fn run(check: &Check, tag: &str, trig: &str) -> Vec<(String, String)> {
        let dir = std::env::temp_dir().join(format!("av-4481-{}-{}-{}", check.id, tag, std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (data, query) = (dir.join("fx.trig"), dir.join("q.rq"));
        std::fs::write(&data, format!("{MODEL}{trig}")).unwrap();
        std::fs::write(&query, check.query).unwrap();
        let out = std::process::Command::new("arq")
            .arg("--data").arg(&data).arg("--query").arg(&query).arg("--results").arg("csv")
            .output()
            .expect("arq (Apache Jena) is required for this proof: brew install jena");
        assert!(out.status.success(), "arq failed: {}", String::from_utf8_lossy(&out.stderr));
        let body = String::from_utf8_lossy(&out.stdout).to_string();
        assert!(body.lines().next().is_some_and(|h| h.starts_with("s,")), "no header: {body:?}");
        crate::store::parse_csv(check.id, &body).iter().map(|f| (f.subject.clone(), f.detail.clone())).collect()
    }

    /// The rule as the model states it, cut down to what the fixtures use.
    const MODEL: &str = r##"@prefix c: <https://jeffbridwell.com/chorus#> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
<urn:chorus:ontology> {
  c:ValueStream c:inHierarchy "chain" .
  c:ValueStreamStep c:inHierarchy "chain" ; c:parentVia c:inStream .
  c:Domain c:inHierarchy "chain" ; c:parentVia c:atStep .
  c:Service c:inHierarchy "chain" ; c:parentVia [ sh:inversePath c:hosts ] .
  c:ServiceInstance c:inHierarchy "chain" ; c:parentVia c:runsService ; c:parentOptional true .
  c:Product c:inHierarchy "product" .
  c:Principal c:inHierarchy "principal" .
  c:hasDomain c:crossesHierarchy "groups" .
  c:ownedBy c:crossesHierarchy "owner" .
  c:atStep c:chainPosition true . c:atStream c:chainPosition true . c:inStream c:chainPosition true .
  c:atStep a owl:ObjectProperty ; rdfs:domain c:Domain ; rdfs:range c:ValueStreamStep .
  c:hosts a owl:ObjectProperty ; rdfs:domain c:Domain ; rdfs:range c:Service .
  c:runsService a owl:ObjectProperty ; rdfs:range c:Service .
}
<urn:chorus:domains:value-streams> {
  c:vs a c:ValueStream .
  c:building a c:ValueStreamStep ; c:inStream c:vs .
}
"##;

    #[test]
    fn a_well_formed_chain_has_no_parent_findings() {
        // p1..p3 point at d1 (inbound links of other kinds): they are not its parents
        let fx = r##"<urn:chorus:domains:domains> { c:d1 a c:Domain ; c:atStep c:building ; c:hosts c:svc1 . c:svc1 a c:Service . c:u1 a c:ServiceInstance ; c:runsService c:svc1 . c:u2 a c:ServiceInstance . c:p1 c:hasDomain c:d1 . c:p2 c:hasDomain c:d1 . c:p3 c:dependsOn c:d1 . }"##;
        assert_eq!(run(&ONE_PARENT, "ok", fx), vec![], "u2 has no service and is allowed none");
        assert_eq!(run(&STORED_DERIVED, "ok", fx), vec![]);
        assert_eq!(run(&PARENT_LOOP, "ok", fx), vec![]);
    }

    /// NEGATIVE PROOF: a domain with two steps, a domain with none, and a
    /// service no domain hosts are each named; an optional-parent unit with two
    /// services is named too.
    #[test]
    fn negative_proof_one_parent_names_none_and_two() {
        let fx = r##"<urn:chorus:domains:domains> {
  c:two a c:Domain ; c:atStep c:building, c:proving . c:proving a c:ValueStreamStep ; c:inStream c:vs .
  c:none a c:Domain .
  c:orphan a c:Service .
  c:u a c:ServiceInstance ; c:runsService c:s1, c:s2 .
}"##;
        let got = run(&ONE_PARENT, "neg", fx);
        let names: Vec<&str> = got.iter().map(|(s, _)| s.as_str()).collect();
        for n in ["two", "none", "orphan", "u"] {
            assert!(names.contains(&n), "{n} missing: {got:?}");
        }
        assert_eq!(got.len(), 4, "{got:?}");
    }

    /// NEGATIVE PROOF: a service that stores its own step, and a domain that
    /// stores its stream, are named; the domain's own atStep is not.
    #[test]
    fn negative_proof_stored_derived_names_second_copies() {
        let fx = r##"<urn:chorus:domains:domains> {
  c:d a c:Domain ; c:atStep c:building ; c:atStream c:vs ; c:hosts c:s .
  c:s a c:Service ; c:atStep c:building .
}"##;
        let mut got = run(&STORED_DERIVED, "neg", fx);
        got.sort();
        assert_eq!(got, vec![("d".into(), "Domain,atStream".into()), ("s".into(), "Service,atStep".into())]);
    }

    /// NEGATIVE PROOF: a domain whose partOf points at a product crosses from
    /// the chain into the product hierarchy with no exception, and is named; the
    /// same link through hasDomain (an exception) and ownedBy are not.
    #[test]
    fn negative_proof_cross_hierarchy_names_unlisted_links_only() {
        let fx = r##"<urn:chorus:domains:domains> {
  c:p a c:Product ; c:hasDomain c:d .
  c:d a c:Domain ; c:atStep c:building ; c:partOf c:p ; c:ownedBy c:silas .
  c:silas a c:Principal .
}"##;
        assert_eq!(run(&CROSS_HIERARCHY, "neg", fx), vec![("d".into(), "partOf,p".into())]);
    }

    /// NEGATIVE PROOF: runsService says what it links to but not from, and is
    /// named for it; atStep and hosts say both and are not.
    #[test]
    fn negative_proof_untyped_link_names_the_half_typed_property() {
        assert_eq!(run(&UNTYPED_LINK, "neg", ""), vec![("runsService".into(), "from".into())]);
    }

    /// NEGATIVE PROOF: the Domain step stored as the word "Building" is named.
    #[test]
    fn negative_proof_link_as_literal_names_text_where_a_row_belongs() {
        let fx = r##"<urn:chorus:domains:domains> { c:d a c:Domain ; c:atStep "Building" . c:d2 a c:Domain ; c:atStep c:building . }"##;
        assert_eq!(run(&LINK_AS_LITERAL, "neg", fx), vec![("d".into(), "atStep,Building".into())]);
    }

    /// NEGATIVE PROOF: a domain whose step points back at the domain loops in
    /// two links, and one whose step is itself loops in one; both are named.
    #[test]
    fn negative_proof_parent_loop_names_a_chain_that_comes_back() {
        let fx = r##"<urn:chorus:domains:domains> {
  c:self a c:Domain ; c:atStep c:self .
  c:d a c:Domain ; c:atStep c:st . c:st a c:ValueStreamStep ; c:inStream c:d .
}"##;
        let names: Vec<String> = run(&PARENT_LOOP, "neg", fx).into_iter().map(|(s, _)| s).collect();
        assert!(names.contains(&"self".to_string()), "{names:?}");
        assert!(names.contains(&"d".to_string()), "{names:?}");
    }

    /// NEGATIVE PROOF (#3734): a store whose model lacks the rule must not read
    /// clean. Each annotation-driven check names the missing annotation instead.
    #[test]
    fn negative_proof_a_model_without_the_rule_is_a_finding_not_clean() {
        let dir = std::env::temp_dir().join(format!("av-4481-norule-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let data = dir.join("fx.trig");
        std::fs::write(&data, "@prefix c: <https://jeffbridwell.com/chorus#> .\n<urn:chorus:domains:domains> { c:d a c:Domain . }\n").unwrap();
        for (check, want) in [(&ONE_PARENT, "model-has-no-parentVia"), (&STORED_DERIVED, "model-has-no-chainPosition"), (&CROSS_HIERARCHY, "model-has-no-inHierarchy")] {
            let query = dir.join(format!("{}.rq", check.id));
            std::fs::write(&query, check.query).unwrap();
            let out = std::process::Command::new("arq").arg("--data").arg(&data).arg("--query").arg(&query).arg("--results").arg("csv").output().expect("arq required");
            assert!(out.status.success(), "arq failed: {}", String::from_utf8_lossy(&out.stderr));
            let body = String::from_utf8_lossy(&out.stdout).to_string();
            assert!(body.lines().skip(1).any(|l| l.starts_with(want)), "{}: {body:?}", check.id);
        }
    }

    /// NEGATIVE PROOF: a domain whose step is a Vertebra row, and a tag naming a
    /// row no graph types, are named; a step that is a ValueStreamStep, a target
    /// in a union range, and a subclass of the range are not.
    #[test]
    fn negative_proof_wrong_target_names_the_wrong_kind_and_the_missing_row() {
        let fx = r##"<urn:chorus:ontology> {
  c:dependsOn a owl:ObjectProperty ; rdfs:range [ owl:unionOf ( c:Domain c:Service ) ] .
  c:Subdomain rdfs:subClassOf c:Domain .
}
<urn:chorus:domains:domains> {
  c:vert a c:Vertebra .
  c:bad a c:Domain ; c:atStep c:vert .
  c:good a c:Domain ; c:atStep c:building ; c:dependsOn c:svc, c:sub .
  c:svc a c:Service . c:sub a c:Subdomain .
  c:lost a c:Domain ; c:atStep c:building ; c:dependsOn c:nowhere .
}"##;
        let mut got = run(&LINK_WRONG_TARGET, "neg", fx);
        got.sort();
        assert_eq!(got, vec![("bad".into(), "atStep,vert".into()), ("lost".into(), "dependsOn,nowhere".into())]);
    }

    #[test]
    fn every_check_selects_rows_and_carries_the_scope_filter() {
        for c in all() {
            assert!(c.query.contains("SELECT DISTINCT ?s"), "{}", c.id);
            assert!(c.query.contains(r#"FILTER(STRSTARTS(STR(?g), "urn:chorus:"))"#), "{}", c.id);
        }
    }
}
