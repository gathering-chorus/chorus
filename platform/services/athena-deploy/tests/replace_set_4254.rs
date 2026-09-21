//! #4254 — the `replace` flag on a domain set, and the guard that keeps it from
//! becoming the 2026-08-28 ontology wipe with a config switch in front of it.
//!
//! Replace means delete-by-absence: what the source no longer says is removed
//! from the graph. That is right for a graph ONE set owns, and catastrophic for
//! a shared one — it would drop every co-tenant's rows on the next deploy.

use athena_deploy::parse_domain_sets;

/// The flag parses, and absence still means merge.
#[test]
fn replace_is_opt_in_and_absence_means_merge() {
    let m = "a|urn:chorus:domains:a|x.ttl|replace\nb|urn:chorus:domains:b|y.ttl\n";
    let sets = parse_domain_sets(m).expect("parses");
    assert!(sets[0].replace, "the flagged set replaces");
    assert!(!sets[1].replace, "an unflagged set merges");
}

/// NEGATIVE PROOF, the important one. Two sets on one graph, one flagged:
/// refused, and the refusal names the co-tenant it would have deleted.
#[test]
fn negative_proof_replacing_a_shared_graph_is_refused() {
    let m = "mine|urn:chorus:domains:shared|x.ttl|replace\n\
             theirs|urn:chorus:domains:shared|y.ttl\n";
    let err = parse_domain_sets(m).expect_err("a shared graph cannot be replaced");
    assert!(err.contains("mine"), "{err}");
    assert!(err.contains("theirs"), "names the co-tenant it would delete: {err}");
    assert!(err.contains("urn:chorus:domains:shared"), "{err}");
}

/// The order of the two lines must not decide it. A guard that only fires when
/// the flagged line comes first would pass on half the manifests that are
/// wrong, which is the class of check this card keeps finding.
#[test]
fn the_refusal_does_not_depend_on_line_order() {
    let m = "theirs|urn:chorus:domains:shared|y.ttl\n\
             mine|urn:chorus:domains:shared|x.ttl|replace\n";
    assert!(parse_domain_sets(m).is_err(), "refused whichever line is first");
}

/// A set with several files is still ONE set, so it may replace its own graph.
/// If this were refused, the vocabulary set — two files, one graph — could
/// never use the flag, and the feature would be unreachable in practice.
#[test]
fn one_set_with_several_files_may_replace_its_own_graph() {
    let m = "v|urn:chorus:domains:v|a.ttl|replace\nv|urn:chorus:domains:v|b.ttl|replace\n";
    let sets = parse_domain_sets(m).expect("parses");
    assert_eq!(sets.len(), 1);
    assert_eq!(sets[0].files.len(), 2);
    assert!(sets[0].replace);
}

/// NEGATIVE PROOF. One set whose lines disagree is two contradictory claims
/// about whether the graph may be emptied, not a preference — refused.
#[test]
fn negative_proof_a_set_disagreeing_with_itself_is_refused() {
    let m = "v|urn:chorus:domains:v|a.ttl|replace\nv|urn:chorus:domains:v|b.ttl\n";
    let err = parse_domain_sets(m).expect_err("must refuse");
    assert!(err.contains("replace"), "{err}");
}

/// NEGATIVE PROOF. A typo in the flag is a refusal, never a silent merge —
/// otherwise a set that asked for delete-by-absence would quietly not get it
/// and nothing anywhere would say so.
#[test]
fn negative_proof_an_unknown_fourth_column_is_refused() {
    let err = parse_domain_sets("v|urn:chorus:domains:v|a.ttl|replce\n").expect_err("must refuse");
    assert!(err.contains("replce"), "names what it saw: {err}");
}

/// The shipped manifest: the vocabulary set replaces, and it is the only one.
/// Named rather than counted — a bare count would go red for the wrong reason
/// the day a second owned graph legitimately takes the flag.
#[test]
fn the_shipped_manifest_replaces_only_the_vocabulary() {
    let d = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let path = format!("{d}/../../config/domain-set-manifest.txt");
    let sets = parse_domain_sets(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let replacing: Vec<&str> =
        sets.iter().filter(|s| s.replace).map(|s| s.name.as_str()).collect();
    assert_eq!(replacing, vec!["vocabulary"], "unexpected set replaces its graph");
}
