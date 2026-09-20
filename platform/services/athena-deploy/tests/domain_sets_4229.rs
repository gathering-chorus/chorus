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
    let path = format!("{}/../../config/domain-set-manifest.txt", crate_dir());
    let text = std::fs::read_to_string(&path).expect("the manifest ships with the verb");
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
    let root = format!("{}/../../..", crate_dir());
    let path = format!("{}/../../config/domain-set-manifest.txt", crate_dir());
    let sets = parse_domain_sets(&std::fs::read_to_string(&path).unwrap()).unwrap();
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

// ---- the verify the Rust verb did not have -------------------------------
// The old check was ASK { graph is non-empty }, which passes even when the
// merge dropped every staged subject. These cover the replacement: how many
// staged subjects are ABSENT afterwards, and what happens when the store does
// not answer at all.
use athena_deploy::verify_missing;

#[test]
fn zero_absent_subjects_is_a_clean_merge() {
    assert_eq!(verify_missing("n\n0\n"), Some(0));
}

#[test]
fn absent_subjects_are_counted_not_rounded_away() {
    assert_eq!(verify_missing("n\n7\n"), Some(7));
    assert_eq!(verify_missing("n\n\"12\"\n"), Some(12));
}

#[test]
fn negative_proof_an_unanswered_verify_is_not_a_pass() {
    // #3726 single-request-truth: a blind verify that passes is worse than no
    // verify. Anything that is not the store answering the question must come
    // back None, which the caller turns into a refusal.
    for not_an_answer in ["", "\n", "<html>502 Bad Gateway</html>", "error\n1\n", "0\n"] {
        assert_eq!(verify_missing(not_an_answer), None, "{not_an_answer:?} must not read as an answer");
    }
}

#[test]
fn negative_proof_the_verify_can_tell_the_two_states_apart() {
    // The old ASK-non-empty could not: both of these were "true". If this ever
    // stops holding, the verify has gone hollow again.
    assert_ne!(verify_missing("n\n0\n"), verify_missing("n\n41\n"));
}

// ---- the TTL= partial gate ----------------------------------------------
use athena_deploy::sets_run;

#[test]
fn a_full_run_deploys_the_domain_sets() {
    assert!(sets_run(None));
    assert!(sets_run(Some("")));
    assert!(sets_run(Some("   ")));
}

#[test]
fn negative_proof_a_single_file_run_does_not_restage_the_other_thirteen() {
    // The bash gates all eight sets behind [ -z "${TTL:-}" ]. Without this a
    // one-file partial — the recovery path, and the way I deployed a single
    // shape by hand this morning — would also re-stage every other set.
    assert!(!sets_run(Some("/x/security-model-3618.ttl")));
}

// ---- #4125: a source file may not author a Role as an owner --------------
use athena_deploy::role_owner_offences;

// Read at run time, never at compile time: the nightly shares one target dir
// across werks, so a binary built in one tree gets re-run in another. A baked-in
// path would then read somebody else's files and report green about the wrong repo.
fn crate_dir() -> String {
    std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR when it runs a test")
}


#[test]
fn an_ownedby_pointing_at_a_role_is_named_with_its_line() {
    let ttl = "chorus:thing a chorus:Card ;\n    chorus:ownedBy chorus:role-wren .\n";
    let hits = role_owner_offences("f.ttl", ttl);
    assert_eq!(hits.len(), 1);
    assert!(hits[0].starts_with("f.ttl:2:"), "{hits:?}");
}

#[test]
fn negative_proof_the_guard_leaves_the_other_role_uses_alone() {
    // 24 legitimate chorus:role-* uses exist on holdsRole and appointedHat. A
    // guard that cannot tell those from an ownedBy violation is the #3734
    // shape — it would refuse every deploy forever and get switched off.
    let ttl = "chorus:p chorus:holdsRole chorus:role-wren ;\n    chorus:appointedHat chorus:role-kade ;\n    chorus:ownedBy chorus:principal-wren .\n";
    assert_eq!(role_owner_offences("f.ttl", ttl), Vec::<String>::new());
}

#[test]
fn a_commented_out_violation_is_not_a_violation() {
    let ttl = "# chorus:ownedBy chorus:role-wren was here\nchorus:x a chorus:Card .\n";
    assert_eq!(role_owner_offences("f.ttl", ttl), Vec::<String>::new());
}

#[test]
fn the_shipped_model_files_are_clean_of_this_today() {
    // If this ever fails, a source file has re-authored the violation the
    // 2026-09-18 store fix could not hold against.
    let root = format!("{}/../../..", crate_dir());
    for f in ["roles/silas/ontology/chorus.ttl", "roles/kade/ontology/werk-domains.ttl"] {
        let text = std::fs::read_to_string(format!("{root}/{f}")).expect(f);
        assert_eq!(role_owner_offences(f, &text), Vec::<String>::new());
    }
}
