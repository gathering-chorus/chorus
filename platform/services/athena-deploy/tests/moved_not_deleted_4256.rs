//! #4256 — a subject that MOVED between files in the model set is not deleted.
//!
//! #4250 moved nine property declarations out of chorus.ttl and into the files
//! that own their classes. The source-delete guard compared each file against
//! its own previous version, read all nine as deletions, and refused the land's
//! model deploy. The store then sat on the pre-#4250 model for a day while the
//! source on main was already correct — the four missing API routes and the
//! rows=0 floors in test-instances-graph-3768 are that, not a deploy defect.
//!
//! The guard could not tell "deleted from the set" from "declared somewhere
//! else in the set" — the two states it exists to separate (#3734).

use athena_deploy::{declared_subjects, vanished_subjects};

/// The set-wide question the deploy now asks, as a function of the same two
/// inputs the deploy has: what each file said before, and what the whole set
/// says now.
fn gone_from_set(before: &[(&str, &str)], now: &[(&str, &str)]) -> Vec<String> {
    let mut declared_now: Vec<String> = Vec::new();
    for (_, text) in now {
        declared_now.extend(declared_subjects(text));
    }
    let mut gone = Vec::new();
    for (name, old_text) in before {
        let current = now.iter().find(|(n, _)| n == name).map(|(_, t)| *t).unwrap_or("");
        for s in vanished_subjects(old_text, current) {
            if !declared_now.contains(&s) {
                gone.push(s);
            }
        }
    }
    gone
}

const CHORUS_BEFORE: &str = "\
chorus:filePath a owl:DatatypeProperty ;
    rdfs:domain chorus:CodeFile .

chorus:keeper a owl:DatatypeProperty ;
    rdfs:range xsd:string .
";

#[test]
fn a_subject_moved_to_a_sibling_file_is_not_a_deletion() {
    // Exactly #4250: filePath leaves chorus.ttl and lands in werk-domains.ttl.
    let before = [("chorus.ttl", CHORUS_BEFORE), ("werk-domains.ttl", "")];
    let now = [
        ("chorus.ttl", "chorus:keeper a owl:DatatypeProperty ;\n    rdfs:range xsd:string .\n"),
        ("werk-domains.ttl", "chorus:filePath a owl:DatatypeProperty ;\n    rdfs:range xsd:string .\n"),
    ];
    assert_eq!(gone_from_set(&before, &now), Vec::<String>::new());
}

#[test]
fn negative_proof_a_subject_deleted_from_every_file_is_still_caught() {
    // The state the guard exists for: gone from its file AND from the set.
    let before = [("chorus.ttl", CHORUS_BEFORE), ("werk-domains.ttl", "")];
    let now = [
        ("chorus.ttl", "chorus:keeper a owl:DatatypeProperty ;\n    rdfs:range xsd:string .\n"),
        ("werk-domains.ttl", "chorus:somethingElse a owl:DatatypeProperty .\n"),
    ];
    let gone = gone_from_set(&before, &now);
    assert_eq!(gone, vec!["filePath".to_string()], "a real deletion must still refuse");
}

#[test]
fn negative_proof_the_old_per_file_rule_would_have_refused_the_move() {
    // Watch the shape that broke: per-file, the move reads as a deletion.
    // This is the behaviour #4256 removes; if this assertion ever fails the
    // per-file comparison is gone from vanished_subjects too and this test
    // has stopped measuring anything.
    let per_file_only = vanished_subjects(
        CHORUS_BEFORE,
        "chorus:keeper a owl:DatatypeProperty ;\n    rdfs:range xsd:string .\n",
    );
    assert_eq!(per_file_only, vec!["filePath".to_string()]);
}

#[test]
fn the_nine_properties_4250_moved_are_all_declared_somewhere_today() {
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let root = format!("{crate_dir}/../../..");
    let mut declared: Vec<String> = Vec::new();
    for role in ["silas", "kade", "wren"] {
        let dir = format!("{root}/roles/{role}/ontology");
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("ttl") {
                declared.extend(declared_subjects(&std::fs::read_to_string(&p).unwrap_or_default()));
            }
        }
    }
    for moved in ["filePath", "httpMethod", "implementedBy", "cdhash", "cardId",
                  "expiresAt", "result", "hasDomain"] {
        assert!(declared.contains(&moved.to_string()),
            "{moved} moved on #4250 and must still be declared somewhere in the set");
    }
}
