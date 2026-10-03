//! #4338 — a file merged into the domains graph must not name a Domain as a
//! subject. That merge replaces every subject its files name, so on 2026-10-02
//! 15:51 cmdb-layers-4293.ttl (two edges per domain) wiped 12 Domain rows down to
//! one inLayer triple each, and 12 classes lost their routes.
//!
//! The guard reads triples through riot, never Turtle lines (Wren's review: a
//! full IRI subject or a second-home declaration slipped past a `chorus:` match).
use athena_deploy::{declared_domains, domain_set_clobbers, model_set, parse_domain_sets, to_ntriples, DOMAINS_GRAPH};

const PREFIXES: &str = "@prefix chorus: <https://jeffbridwell.com/chorus#> .\n@prefix owl: <http://www.w3.org/2002/07/owl#> .\n@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n";
const HOME: &str = "\
chorus:logs a chorus:Domain ;
    rdfs:label \"logs\" .
chorus:time
    a owl:Class, chorus:Domain .
chorus:Machine a owl:Class .
";

/// Turtle → (label, N-Triples) through the same riot call the deploy makes.
fn nt(label: &str, body: &str) -> (String, String) {
    // one file per call: tests run in parallel and share labels
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("clobber-4338-{}-{n}-{label}.ttl", std::process::id()));
    std::fs::write(&dir, format!("{PREFIXES}{body}")).unwrap();
    let out = to_ntriples(dir.to_str().unwrap()).expect("riot reads the fixture");
    let _ = std::fs::remove_file(&dir);
    (label.to_string(), out)
}

const LOGS: &str = "<https://jeffbridwell.com/chorus#logs>";
const TIME: &str = "<https://jeffbridwell.com/chorus#time>";

#[test]
fn domains_are_read_from_triples_not_lines() {
    // time's type sits on the line after its subject; a line match missed it
    assert_eq!(declared_domains(&[nt("home", HOME)]), vec![LOGS, TIME]);
}

#[test]
fn negative_proof_the_1551_shape_is_named() {
    let set = "chorus:layer-foundation a chorus:Layer .\n\
               chorus:logs chorus:inLayer chorus:layer-foundation .\n\
               chorus:time chorus:dependsOn chorus:logs .\n";
    let hits = domain_set_clobbers(&[nt("home", HOME)], &[nt("cmdb", set)]);
    assert_eq!(hits, vec![format!("cmdb: {LOGS}"), format!("cmdb: {TIME}")]);
}

#[test]
fn negative_proof_a_full_iri_subject_is_named() {
    let set = "<https://jeffbridwell.com/chorus#logs> chorus:inLayer chorus:layer-foundation .\n";
    assert_eq!(domain_set_clobbers(&[nt("home", HOME)], &[nt("cmdb", set)]), vec![format!("cmdb: {LOGS}")]);
}

#[test]
fn negative_proof_a_set_file_declaring_a_domain_is_a_second_home() {
    // not declared in any home file: the set file is the only place it is a Domain
    let set = "chorus:metrics     a chorus:Domain .\n";
    assert_eq!(
        domain_set_clobbers(&[nt("home", HOME)], &[nt("cmdb", set)]),
        vec!["cmdb: <https://jeffbridwell.com/chorus#metrics>".to_string()]
    );
}

#[test]
fn a_domain_only_as_an_object_is_not_a_clobber() {
    let set = "chorus:layer-foundation a chorus:Layer ;\n    chorus:groups chorus:logs .\n";
    assert!(domain_set_clobbers(&[nt("home", HOME)], &[nt("cmdb", set)]).is_empty());
}

#[test]
fn an_unreadable_file_is_an_error_not_an_empty_pass() {
    assert!(to_ntriples("/nonexistent/clobber-4338.ttl").is_err());
}

#[test]
fn the_shipped_model_names_no_domain_in_the_domains_graph_sets() {
    let root = format!("{}/../../..", std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let read = |p: &str| (p.to_string(), to_ntriples(p).unwrap_or_else(|e| panic!("{e}")));
    let homes: Vec<_> = model_set(&root, None).iter().map(|p| read(p)).collect();
    // a guard whose input vanished must fail, never pass on nothing
    let n = declared_domains(&homes).len();
    assert!(n >= 40, "found {n} Domains in the model set");
    let manifest = std::fs::read_to_string(format!("{root}/platform/config/domain-set-manifest.txt")).unwrap();
    let set_files: Vec<_> = parse_domain_sets(&manifest).unwrap().iter()
        .filter(|ds| ds.graph == DOMAINS_GRAPH)
        .flat_map(|ds| ds.files.clone()).map(|p| read(&format!("{root}/{p}"))).collect();
    assert!(!set_files.is_empty(), "no set targets the domains graph");
    assert_eq!(domain_set_clobbers(&homes, &set_files), Vec::<String>::new());
}
