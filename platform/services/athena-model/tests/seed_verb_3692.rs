//! #3692 — the `seed` verb: bulk TTL ingest, the 5th DAL verb. Red-first
//! (DEC-1674) against Silas's design shape (2026-07-25):
//!   - seed takes PRE-MINTED IRIs from the TTL and PRESERVES them (add mints;
//!     seed must not — that inversion is the migration case #3392 needs)
//!   - every subject SHACL-validated via read_shape(class); ONE violating
//!     subject rejects the WHOLE batch (fail-closed, all-or-nothing INSERT)
//!   - existing subject: dcterms:created preserved (write() pattern),
//!     modified bumped, chorus:provenance "migrated" stamped
//!   - idempotent: re-running the same input is a semantic no-op (identical
//!     DELETE-WHERE + INSERT DATA)
//!   - same identity gate + assert_dal_writable as write() (identity-less
//!     refused; ontology/security graphs refused)
//! Hermetic: stub store answers shape SELECTs / existence ASKs; no live
//! Fuseki, no $HOME, no role state (test-isolation contract).

use athena_model::{delete_iri, parse_ntriples, seed, verify_identity, Identity, Store, R};
use std::cell::RefCell;

const NS: &str = "https://jeffbridwell.com/chorus#";
const G: &str = "urn:chorus:domains:icd";

struct Cfg {
    required: Vec<String>,        // sh:minCount>=1 property local-names
    existing: Vec<String>,        // IRIs that exist (ASK true)
    created_of_existing: Option<String>, // stored dcterms:created for existing subject
    asks: RefCell<Vec<String>>,
    updates: RefCell<Vec<String>>,
}

impl Store for Cfg {
    fn ask(&self, sparql: &str) -> R<bool> {
        if sparql.contains("urn:chorus:domains:security") {
            return Ok(true); // identity registry — identity_gate.rs owns that variable
        }
        self.asks.borrow_mut().push(sparql.to_string());
        Ok(self.existing.iter().any(|e| sparql.contains(e.as_str())))
    }
    fn select_v(&self, sparql: &str) -> R<Vec<String>> {
        // #3839 — the shape pins where its instances live. These fixtures model a
        // correctly-pinned shape; the refusal for an UNPINNED one is proved
        // separately in lib.rs (seed_multi_3839::a_shape_with_no_instances_graph_pin_refuses).
        if sparql.contains("chorus:instancesGraph ?v") {
            Ok(vec!["urn:chorus:instances".to_string()])
        } else if sparql.contains("sh:minCount") {
            Ok(self.required.clone())
        } else if sparql.contains("created") {
            Ok(self.created_of_existing.iter().cloned().collect())
        } else {
            Ok(vec![])
        }
    }
    fn update(&self, s: &str) -> R<()> {
        self.updates.borrow_mut().push(s.to_string());
        Ok(())
    }
}

fn cfg() -> Cfg {
    Cfg {
        required: vec![],
        existing: vec![],
        created_of_existing: None,
        asks: RefCell::new(vec![]),
        updates: RefCell::new(vec![]),
    }
}

fn vid(s: &Cfg) -> Identity {
    verify_identity(Some("kade"), s).unwrap()
}

/// Two well-formed domain subjects with pre-minted bare-grain IRIs, label set.
fn valid_triples() -> Vec<(String, String, String)> {
    vec![
        (
            format!("<{}icd>", NS),
            format!("<{}label>", NS),
            "\"ICD\"".to_string(),
        ),
        (
            format!("<{}icd>", NS),
            "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>".to_string(),
            format!("<{}Domain>", NS),
        ),
        (
            format!("<{}convergence>", NS),
            format!("<{}label>", NS),
            "\"Convergence\"".to_string(),
        ),
        (
            format!("<{}convergence>", NS),
            "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>".to_string(),
            format!("<{}Domain>", NS),
        ),
    ]
}

// ── AC1: valid TTL loads, IRIs preserved verbatim ───────────────────────────

#[test]
fn seed_loads_valid_subjects_preserving_ttl_iris() {
    let store = cfg();
    let id = vid(&store);
    let report = seed(&store, "domain", &valid_triples(), "migrated", Some(G), &id).unwrap();
    assert_eq!(report.subjects, 2, "both subjects seeded");

    let ups = store.updates.borrow();
    assert_eq!(ups.len(), 1, "ONE transaction — all-or-nothing");
    let s = &ups[0];
    // The TTL's own IRIs — never re-minted, never re-slugged.
    assert!(s.contains(&format!("<{}icd>", NS)), "pre-minted IRI preserved: {}", s);
    assert!(s.contains(&format!("<{}convergence>", NS)), "pre-minted IRI preserved");
    assert!(s.contains(&format!("GRAPH <{}>", G)), "graph-scoped");
    assert!(s.contains("provenance"), "provenance stamped");
    assert!(s.contains("migrated"), "provenance value = migrated");
    assert!(s.contains("created") && s.contains("modified"), "audit envelope stamped");
}

// ── AC3: existing subject — created preserved, modified bumped ──────────────

#[test]
fn seed_preserves_existing_created_on_reload() {
    let mut store = cfg();
    store.existing = vec![format!("{}icd", NS)];
    store.created_of_existing = Some("2025-01-01T00:00:00".to_string());
    let id = vid(&store);
    seed(&store, "domain", &valid_triples(), "migrated", Some(G), &id).unwrap();

    let ups = store.updates.borrow();
    assert!(
        ups[0].contains("2025-01-01T00:00:00"),
        "existing dcterms:created must survive the reload (write() pattern): {}",
        ups[0]
    );
}

// ── AC2: one bad subject rejects the WHOLE batch ────────────────────────────

#[test]
fn seed_rejects_whole_batch_on_one_shape_violation() {
    let mut store = cfg();
    // the shape requires 'owner' — neither subject carries it, but ONE failing
    // subject is already enough; assert nothing at all is written.
    store.required = vec!["owner".to_string()];
    let id = vid(&store);
    let err = seed(&store, "domain", &valid_triples(), "migrated", Some(G), &id).unwrap_err();
    assert!(err.starts_with("shape-violation"), "typed refusal: {}", err);
    assert!(
        store.updates.borrow().is_empty(),
        "fail-closed: a violating subject must leave the store untouched"
    );
}

// ── AC4: idempotent — same input, identical transaction ─────────────────────

#[test]
fn seed_is_idempotent_same_input_same_update() {
    let store = cfg();
    let id = vid(&store);
    seed(&store, "domain", &valid_triples(), "migrated", Some(G), &id).unwrap();
    let first = store.updates.borrow()[0].clone();
    // second run: the store now "has" the subjects and answers their created
    let mut store2 = cfg();
    store2.existing = vec![format!("{}icd", NS), format!("{}convergence", NS)];
    let id2 = vid(&store2);
    seed(&store2, "domain", &valid_triples(), "migrated", Some(G), &id2).unwrap();
    let second = store2.updates.borrow()[0].clone();
    // identical up to the audit timestamps — compare with stamp lines stripped
    let strip = |s: &str| -> String {
        s.lines()
            .filter(|l| !l.contains("created") && !l.contains("modified"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(strip(&first), strip(&second), "re-run must be a semantic no-op");
}

// ── AC5: identity gate + graph door ─────────────────────────────────────────

#[test]
fn seed_refuses_identity_less_run() {
    let store = cfg();
    // the ONLY door to an Identity is verify_identity; a None claim refuses.
    assert!(
        verify_identity(None, &store).is_err(),
        "identity-less callers cannot obtain the Identity seed requires"
    );
}

#[test]
fn seed_refuses_ontology_and_security_graphs() {
    let store = cfg();
    let id = vid(&store);
    for g in ["urn:chorus:ontology", "urn:chorus:domains:security"] {
        let e = seed(&store, "domain", &valid_triples(), "migrated", Some(g), &id).unwrap_err();
        assert!(e.contains("dba-only"), "typed dba-only refusal for {}: {}", g, e);
    }
    assert!(store.updates.borrow().is_empty(), "protected graphs never written");
}

// ── IRI guard: off-convention subject refused ───────────────────────────────

#[test]
fn seed_refuses_subject_iri_off_kind_convention() {
    let store = cfg();
    let id = vid(&store);
    let rogue = vec![(
        "<urn:somewhere:else/icd>".to_string(),
        format!("<{}label>", NS),
        "\"ICD\"".to_string(),
    )];
    let e = seed(&store, "domain", &rogue, "migrated", Some(G), &id).unwrap_err();
    assert!(e.contains("iri"), "typed IRI-guard refusal: {}", e);
    assert!(store.updates.borrow().is_empty());
}

// ── Review findings (Silas, 2026-07-25): the two design calls, pinned ───────

#[test]
fn seed_refuses_mixed_kind_rdf_type() {
    // A subject declaring rdf:type != the seed kind's class is contamination —
    // a mixed-kind TTL is two seed runs, never one blind one.
    let store = cfg();
    let id = vid(&store);
    let mixed = vec![
        (
            format!("<{}icd>", NS),
            "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>".to_string(),
            format!("<{}Service>", NS), // wrong class for kind "domain"
        ),
        (
            format!("<{}icd>", NS),
            format!("<{}label>", NS),
            "\"ICD\"".to_string(),
        ),
    ];
    let e = seed(&store, "domain", &mixed, "migrated", Some(G), &id).unwrap_err();
    assert!(e.starts_with("shape-violation"), "typed refusal: {}", e);
    assert!(e.contains("Service"), "refusal names the offending type: {}", e);
    assert!(store.updates.borrow().is_empty(), "nothing written on a mixed-kind batch");
}

#[test]
fn seed_batch_internal_edge_satisfies_but_dangling_refuses() {
    // The migration case: TTLs are internally referential. An edge whose target
    // exists ONLY in the batch must load; an edge whose target exists in
    // neither the store nor the batch is a dangling ref — refused fail-closed.
    // (Silas review 2026-07-25: this is the most regression-prone code in the
    // verb — a future store-only "simplification" must go red here.)
    let internal = vec![
        (
            format!("<{}icd>", NS),
            format!("<{}label>", NS),
            "\"ICD\"".to_string(),
        ),
        (
            // edge to a subject that is IN THIS BATCH (below), not in the store
            format!("<{}icd>", NS),
            format!("<{}dependsOn>", NS),
            format!("<{}convergence>", NS),
        ),
        (
            format!("<{}convergence>", NS),
            format!("<{}label>", NS),
            "\"Convergence\"".to_string(),
        ),
    ];
    let store = cfg(); // store knows NOTHING — batch-internal must still satisfy
    let id = vid(&store);
    let report = seed(&store, "domain", &internal, "migrated", Some(G), &id).unwrap();
    assert_eq!(report.subjects, 2, "batch-internal edge target satisfies referential integrity");
    assert_eq!(store.updates.borrow().len(), 1, "loads in one transaction");

    // Same shape, but the edge now points OUTSIDE both store and batch.
    let dangling = vec![
        (
            format!("<{}icd>", NS),
            format!("<{}label>", NS),
            "\"ICD\"".to_string(),
        ),
        (
            format!("<{}icd>", NS),
            format!("<{}dependsOn>", NS),
            format!("<{}nowhere-to-be-found>", NS),
        ),
    ];
    let store2 = cfg();
    let id2 = vid(&store2);
    let e = seed(&store2, "domain", &dangling, "migrated", Some(G), &id2).unwrap_err();
    assert!(e.starts_with("unknown-target"), "typed refusal: {}", e);
    assert!(e.contains("nowhere-to-be-found"), "refusal names the dangling target: {}", e);
    assert!(store2.updates.borrow().is_empty(), "dangling ref writes nothing");
}

// ── #3392 — realm-policy table (Silas ruling 2026-07-25) ────────────────────
// The seed door is a KNOWN-REALMS ALLOWLIST, not chorus-only and not open:
//   chorus realm    → strict KINDS-convention IRI check + rdf:type-match (unchanged)
//   gathering realm → NS-membership + well-formed + no-injection ONLY (ICD
//                     self-types via icd:Domain — not our KINDS vocab, not our
//                     type vocab; stricter would wrongly reject valid ICD)
//   anything else   → off-realm refusal (unchanged)

const ICD_G: &str = "urn:gathering:icd/current";

fn icd_triples() -> Vec<(String, String, String)> {
    vec![
        // live-store reality (verified 2026-07-25): instance subjects resolved
        // against base https://jeffbridwell.com/, class defs subject in urn:gathering:icd#
        (
            "<https://jeffbridwell.com/icd/domain/photos>".to_string(),
            "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>".to_string(),
            "<urn:gathering:icd#Domain>".to_string(),
        ),
        (
            "<https://jeffbridwell.com/icd/domain/photos>".to_string(),
            "<urn:gathering:icd#domainName>".to_string(),
            "\"photos\"".to_string(),
        ),
        (
            "<urn:gathering:icd#Domain>".to_string(),
            "<http://www.w3.org/2000/01/rdf-schema#label>".to_string(),
            "\"ICD Domain\"".to_string(),
        ),
    ]
}

#[test]
fn seed_gathering_realm_loads_icd_with_native_iris_and_types() {
    // The whole point of #3392: ICD loads into ITS graph with ITS IRIs and ITS
    // type vocab — no chorus kind convention, no rdf:type-match, no re-homing.
    let store = cfg();
    let id = vid(&store);
    let report = seed(&store, "domain", &icd_triples(), "migrated", Some(ICD_G), &id).unwrap();
    assert_eq!(report.subjects, 2, "both ICD subjects load");
    let ups = store.updates.borrow();
    assert_eq!(ups.len(), 1, "one transaction");
    let s = &ups[0];
    assert!(s.contains("<https://jeffbridwell.com/icd/domain/photos>"), "native instance IRI preserved");
    assert!(s.contains("<urn:gathering:icd#Domain>"), "native class IRI preserved");
    assert!(s.contains(&format!("GRAPH <{}>", ICD_G)), "scoped to the gathering graph");
    assert!(s.contains("migrated"), "provenance still stamped");
}

#[test]
fn seed_gathering_realm_skips_chorus_type_guard() {
    // icd:Domain != chorus#Domain — under the chorus policy this is the
    // mixed-kind refusal; under the gathering policy it MUST pass (their
    // type vocab, not ours). Pinned so a future tightening goes red here.
    let store = cfg();
    let id = vid(&store);
    let typed_foreign = vec![(
        "<https://jeffbridwell.com/icd/domain/music>".to_string(),
        "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>".to_string(),
        "<urn:gathering:icd#Domain>".to_string(),
    )];
    assert!(
        seed(&store, "domain", &typed_foreign, "migrated", Some(ICD_G), &id).is_ok(),
        "gathering realm must not apply the chorus rdf:type-match guard"
    );
}

#[test]
fn seed_gathering_realm_still_refuses_off_ns_subjects() {
    // Bounded, not open: a subject outside the gathering ns-set refuses even
    // when the graph is a gathering graph.
    let store = cfg();
    let id = vid(&store);
    let rogue = vec![(
        "<https://evil.example.com/icd/domain/photos>".to_string(),
        "<urn:gathering:icd#domainName>".to_string(),
        "\"photos\"".to_string(),
    )];
    let e = seed(&store, "domain", &rogue, "migrated", Some(ICD_G), &id).unwrap_err();
    assert!(e.contains("iri"), "typed IRI-guard refusal: {}", e);
    assert!(store.updates.borrow().is_empty());
}

#[test]
fn seed_unknown_realm_graph_still_refused() {
    // The allowlist is chorus + gathering — nothing else. The off-realm door
    // #3573 closed stays closed.
    let store = cfg();
    let id = vid(&store);
    for g in ["urn:other:stuff", "https://example.com/graphs/x"] {
        let e = seed(&store, "domain", &valid_triples(), "migrated", Some(g), &id).unwrap_err();
        assert!(e.contains("realm") || e.contains("refused"), "unknown realm refused ({}): {}", g, e);
    }
    assert!(store.updates.borrow().is_empty());
}

#[test]
fn seed_chorus_realm_policy_unchanged_by_the_widening() {
    // Regression pin: the gathering allowance must not loosen chorus — an
    // off-convention chorus subject still refuses in a chorus graph.
    let store = cfg();
    let id = vid(&store);
    let rogue = vec![(
        "<urn:somewhere:else/icd>".to_string(),
        format!("<{}label>", NS),
        "\"ICD\"".to_string(),
    )];
    assert!(seed(&store, "domain", &rogue, "migrated", Some(G), &id).is_err());
}

// ── Real-content literals (#3392 live-load finding #2) ──────────────────────
// ICD carries mermaid diagrams and slug templates: literals containing
// { } ; and ESCAPED quotes. riot-canonicalized N-Triples guarantees inner
// quotes arrive backslash-escaped, so those chars are inert in the
// door-assembled INSERT — refusing them refuses real data (the
// "photograph" ruling extended to the chars mermaid actually uses).

#[test]
fn seed_accepts_escaped_real_content_literals() {
    let store = cfg();
    let id = vid(&store);
    let real = vec![
        (
            "<https://jeffbridwell.com/icd/section/documents/diagrams>".to_string(),
            "<urn:gathering:icd#mermaidSource>".to_string(),
            r#""graph LR\n    H -->|\"streaming\"| RP{PhotoRouter}\n    RP -->|yes| PH[Photos] ;""#.to_string(),
        ),
        (
            "<https://jeffbridwell.com/icd/field/documents/docSlug-1>".to_string(),
            "<urn:gathering:icd#fieldTypeDescription>".to_string(),
            r#""{name-slug}-{fileId:8}""#.to_string(),
        ),
    ];
    let report = seed(&store, "domain", &real, "migrated", Some(ICD_G), &id).unwrap();
    assert_eq!(report.subjects, 2, "mermaid + slug-template literals must load");
    assert!(store.updates.borrow()[0].contains("PhotoRouter"), "content preserved");
}

#[test]
fn seed_still_refuses_unescaped_quote_breakout() {
    // The actual injection vector: an UNESCAPED quote closing the literal
    // early. NT never produces this; a hand-crafted input trying it refuses.
    let store = cfg();
    let id = vid(&store);
    let evil = vec![(
        "<https://jeffbridwell.com/icd/field/x>".to_string(),
        "<urn:gathering:icd#fieldTypeDescription>".to_string(),
        "\"broken\" } ; INSERT DATA { GRAPH <urn:x> { <a> <b> \"c\" \"".to_string(),
    )];
    let e = seed(&store, "domain", &evil, "migrated", Some(ICD_G), &id).unwrap_err();
    assert!(e.contains("slot"), "breakout-shaped literal refused: {}", e);
    assert!(store.updates.borrow().is_empty());
}

#[test]
fn seed_refuses_backslash_parity_breakout() {
    // Silas's hole (ruling 2026-07-25): `foo\\"` — the \\ is an ESCAPED
    // BACKSLASH, so the following quote is UNESCAPED and closes the literal
    // early. A quote is escaped only when preceded by an ODD run of
    // backslashes; an even run before a quote = real delimiter = refuse.
    let store = cfg();
    let id = vid(&store);
    let evil = vec![(
        "<https://jeffbridwell.com/icd/field/z>".to_string(),
        "<urn:gathering:icd#fieldTypeDescription>".to_string(),
        // literal body: foo\\" } ; INSERT ... — even backslash run then quote
        "\"foo\\\\\" } ; INSERT DATA { GRAPH <urn:x> { <a> <b> \"c\" } } \"".to_string(),
    )];
    let e = seed(&store, "domain", &evil, "migrated", Some(ICD_G), &id).unwrap_err();
    assert!(e.contains("slot"), "even-backslash-run quote breakout refused: {}", e);
    assert!(store.updates.borrow().is_empty());

    // And the legit sibling: an odd run (escaped backslash + escaped quote)
    // is real content and must load.
    let legit = vec![(
        "<https://jeffbridwell.com/icd/field/z2>".to_string(),
        "<urn:gathering:icd#fieldTypeDescription>".to_string(),
        r#""path \\ then \" quote""#.to_string(),
    )];
    let store2 = cfg();
    let id2 = vid(&store2);
    assert!(seed(&store2, "domain", &legit, "migrated", Some(ICD_G), &id2).is_ok());
}

#[test]
fn seed_still_refuses_raw_control_chars_in_literals() {
    let store = cfg();
    let id = vid(&store);
    let raw_newline = vec![(
        "<https://jeffbridwell.com/icd/field/y>".to_string(),
        "<urn:gathering:icd#fieldTypeDescription>".to_string(),
        "\"line one\nline two\"".to_string(), // RAW newline — NT would emit \n escaped
    )];
    let e = seed(&store, "domain", &raw_newline, "migrated", Some(ICD_G), &id).unwrap_err();
    assert!(e.contains("slot"), "raw control char refused: {}", e);
}

// ── delete_iri (#3392 — governed foreign-realm delete, Silas ruling) ────────
// The migration-cleanup path: a live-only junk subject in a gathering graph
// has no chorus (kind,name) — delete_entity can't address it. delete_iri is
// the DAL door for by-IRI deletion: realm-policy gated, dba-graph refused,
// subject + inbound references in ONE transaction, model.deleted witnessed.

#[test]
fn delete_iri_removes_subject_and_inbound_refs_one_tx() {
    let store = cfg();
    let id = vid(&store);
    let iri = "https://jeffbridwell.com/icd/field/photos/test-automation-field";
    delete_iri(&store, iri, ICD_G, &id).unwrap();
    let ups = store.updates.borrow();
    assert_eq!(ups.len(), 1, "one transaction");
    let s = &ups[0];
    assert!(s.contains(&format!("<{}> ?p ?o", iri)), "subject triples deleted: {}", s);
    assert!(s.contains(&format!("?s ?p2 <{}>", iri)), "inbound references deleted: {}", s);
    assert!(s.contains(&format!("GRAPH <{}>", ICD_G)), "graph-scoped");
}

#[test]
fn delete_iri_refuses_off_realm_and_dba_graphs() {
    let store = cfg();
    let id = vid(&store);
    let iri = "https://jeffbridwell.com/icd/field/photos/test-automation-field";
    assert!(delete_iri(&store, iri, "urn:other:stuff", &id).is_err(), "unknown realm refused");
    assert!(delete_iri(&store, iri, "urn:chorus:ontology", &id).is_err(), "dba graph refused");
    assert!(store.updates.borrow().is_empty());
}

#[test]
fn delete_iri_refuses_subject_outside_realm_ns() {
    let store = cfg();
    let id = vid(&store);
    let e = delete_iri(&store, "https://evil.example.com/x", ICD_G, &id).unwrap_err();
    assert!(e.contains("iri"), "off-ns subject refused: {}", e);
    assert!(store.updates.borrow().is_empty());
}

// ── N-Triples parsing (the CLI feeds seed via riot --output=ntriples) ───────

#[test]
fn parse_ntriples_reads_simple_lines() {
    let nt = format!(
        "<{ns}icd> <{ns}label> \"ICD\" .\n<{ns}icd> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <{ns}Domain> .\n",
        ns = NS
    );
    let triples = parse_ntriples(&nt).unwrap();
    assert_eq!(triples.len(), 2);
    assert_eq!(triples[0].0, format!("<{}icd>", NS));
    assert_eq!(triples[0].2, "\"ICD\"");
}

#[test]
fn parse_ntriples_refuses_blank_nodes() {
    let nt = "_:b0 <urn:p> \"x\" .\n";
    assert!(
        parse_ntriples(nt).is_err(),
        "blank nodes have no preservable IRI — refuse, don't guess"
    );
}
