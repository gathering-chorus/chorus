//! #4229 — the eight set legs as data. The bash wrote this leg eight times;
//! four copies were one verify and two refusals weaker than the others because
//! later fixes only reached some of them. One implementation plus rows cannot
//! drift from itself that way — but the parser has to refuse rather than skip,
//! or a dropped row becomes a silently unloaded file, which is exactly how the
//! Rust verb came to deploy 2 files while the bash deployed 41 and said success.
use athena_deploy::{parse_domain_sets, DomainSet};

const GOOD: &str = "\
# a comment
security|urn:chorus:domains:security|roles/silas/ontology/a.ttl
security|urn:chorus:domains:security|roles/silas/ontology/b.ttl

values|urn:chorus:domains:values|roles/wren/ontology/values.ttl
";

#[test]
fn rows_group_into_sets_and_keep_their_order() {
    let sets = parse_domain_sets(GOOD).expect("parses");
    assert_eq!(sets.len(), 2);
    assert_eq!(sets[0].name, "security");
    assert_eq!(sets[0].graph, "urn:chorus:domains:security");
    assert_eq!(sets[0].files, vec!["roles/silas/ontology/a.ttl", "roles/silas/ontology/b.ttl"]);
    assert_eq!(sets[1].name, "values");
}

#[test]
fn the_real_manifest_holds_the_eight_sets_the_bash_deploys() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/domain-set-manifest.txt");
    let text = std::fs::read_to_string(path).expect("the manifest ships with the verb");
    let sets = parse_domain_sets(&text).expect("the shipped manifest must parse");
    let names: Vec<&str> = sets.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names.len(), 8, "eight sets, got {names:?}");
    for want in ["security", "code-vocab", "roles", "infrastructure",
                 "principles", "values", "services", "practices"] {
        assert!(names.contains(&want), "{want} missing from {names:?}");
    }
    let files: usize = sets.iter().map(|s| s.files.len()).sum();
    assert_eq!(files, 13, "the bash names 13 files across these eight sets");
}

#[test]
fn every_file_in_the_shipped_manifest_exists() {
    // A manifest naming a file that is not there would deploy nothing for that
    // set and, before this card, say success.
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../..");
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/domain-set-manifest.txt");
    let sets = parse_domain_sets(&std::fs::read_to_string(path).unwrap()).unwrap();
    for s in &sets {
        for f in &s.files {
            let p = format!("{root}/{f}");
            assert!(std::path::Path::new(&p).is_file(), "{f} named by set {} is missing", s.name);
        }
    }
}

#[test]
fn negative_proof_a_malformed_row_is_refused_not_skipped() {
    // The whole point. If this returned Ok with one set, the row would vanish
    // and the deploy would report success having loaded less than it was told.
    let err = parse_domain_sets("security|urn:chorus:domains:security\nvalues|urn:x|f.ttl")
        .expect_err("a two-field row must refuse");
    assert!(err.contains("line 1"), "the refusal must name the line: {err}");
}

#[test]
fn negative_proof_a_graph_that_is_not_an_iri_is_refused() {
    let err = parse_domain_sets("values|values|f.ttl").expect_err("must refuse");
    assert!(err.contains("not a graph IRI"), "{err}");
}

#[test]
fn negative_proof_one_set_cannot_target_two_graphs() {
    // Two rows disagreeing about a set's home is the drift this file replaces,
    // stated in data. It must fail loudly rather than pick one.
    let err = parse_domain_sets(
        "values|urn:chorus:domains:values|a.ttl\nvalues|urn:chorus:instances|b.ttl",
    )
    .expect_err("must refuse");
    assert!(err.contains("cannot also target"), "{err}");
}

#[test]
fn an_empty_manifest_is_no_sets_not_an_error() {
    assert_eq!(parse_domain_sets("# only a comment\n\n").unwrap(), Vec::<DomainSet>::new());
}
