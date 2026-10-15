// @test-type: unit — signal:api is fixture-data
// @domain: athena
//! #4536 run 10 (10-15 14:42) — two causes of 6 "new" issues on the staging
//! validate. (1) The card deploy loaded every subject in werk-domains.ttl, so
//! untyped code / pipelines / version-control fragments re-landed in the
//! ontology graph. (2) The seed step copied prod again after the baseline was
//! measured, and prod had moved (messages +636, provenance +10).
use athena_deploy::card::changed_triples;
use athena_deploy::staging::copy_is_this_runs;

const C: &str = "<https://jeffbridwell.com/chorus#";
fn t(s: &str, p: &str, o: &str) -> String {
    format!("{C}{s}> {C}{p}> {o} .\n")
}

#[test]
fn an_unchanged_subject_in_a_touched_file_is_not_loaded() {
    let code = t("code", "status", "\"building\"");
    let shape_old = t("TestShape", "label", "\"t\"");
    let main = format!("{code}{shape_old}");
    let shape_new = format!("{shape_old}{C}TestShape> <http://www.w3.org/ns/shacl#property> _:b0 .\n_:b0 <http://www.w3.org/ns/shacl#path> {C}result> .\n");
    let werk = format!("{code}{shape_new}");
    let out = changed_triples(&werk, &main);
    assert!(!out.contains("#code>"), "unchanged subject loaded: {out}");
}

/// NEGATIVE PROOF: the changed subject IS loaded, whole, with the blank nodes it reaches.
#[test]
fn negative_proof_a_changed_subject_is_loaded_with_its_blank_nodes() {
    let code = t("code", "status", "\"building\"");
    let shape_old = t("TestShape", "label", "\"t\"");
    let main = format!("{code}{shape_old}");
    let werk = format!("{code}{shape_old}{C}TestShape> <http://www.w3.org/ns/shacl#property> _:b0 .\n_:b0 <http://www.w3.org/ns/shacl#path> {C}result> .\n");
    let out = changed_triples(&werk, &main);
    assert_eq!(out.lines().count(), 3, "{out}");
    assert!(out.contains("_:b0 <http://www.w3.org/ns/shacl#path>"), "{out}");
    // a file main never had: everything is the card's
    assert_eq!(changed_triples(&werk, "").lines().count(), 4);
}

#[test]
fn the_seed_reuses_a_copy_made_by_this_run() {
    assert!(copy_is_this_runs(Some("034gUFGI\n"), Some("034gUFGI")).unwrap().contains("reused"));
}

/// NEGATIVE PROOF: a copy from another run, an unstamped copy, or no trace is refused.
#[test]
fn negative_proof_a_stale_or_unnamed_copy_is_refused() {
    assert!(copy_is_this_runs(Some("034gSFDS"), Some("034gUFGI")).unwrap_err().contains("not this run"));
    assert!(copy_is_this_runs(None, Some("034gUFGI")).is_err());
    assert!(copy_is_this_runs(Some(""), Some("034gUFGI")).is_err());
    assert!(copy_is_this_runs(Some("034gUFGI"), None).is_err());
}
