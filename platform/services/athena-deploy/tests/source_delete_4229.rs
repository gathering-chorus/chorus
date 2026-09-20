//! #4125 — a subject deleted from source is named, not silently kept. Nothing
//! has ever left the ontology graph by absence; the additive merge only ever
//! touches subjects that are IN staging. Deleting whatever is absent instead
//! is the 2026-06-26 wipe, so absence drives a refusal, never a delete.
use athena_deploy::{declared_subjects, vanished_subjects};

const BEFORE: &str = "\
chorus:Alpha a owl:Class ;
    rdfs:label \"a\" .
chorus:Beta a owl:Class .
chorus:Gamma a owl:Class .
";

const AFTER: &str = "\
chorus:Alpha a owl:Class ;
    rdfs:label \"a\" .
chorus:Gamma a owl:Class .
";

#[test]
fn a_subject_that_left_the_file_is_named() {
    assert_eq!(vanished_subjects(BEFORE, AFTER), vec!["Beta".to_string()]);
}

#[test]
fn nothing_removed_means_nothing_to_report() {
    assert_eq!(vanished_subjects(BEFORE, BEFORE), Vec::<String>::new());
}

#[test]
fn negative_proof_a_subject_used_only_as_an_object_is_not_this_files_to_retire() {
    // The guard must not fire on a reference. A check that cannot tell a
    // declaration from a mention would refuse every deploy that reorders a
    // file, and then get switched off — the #3734 shape.
    let before = "chorus:Alpha a owl:Class ;\n    chorus:related chorus:Elsewhere .\n";
    let after = "chorus:Alpha a owl:Class .\n";
    assert_eq!(vanished_subjects(before, after), Vec::<String>::new());
}

#[test]
fn negative_proof_an_indented_or_commented_line_is_not_a_declaration() {
    let before = "    chorus:Indented a owl:Class .\n# chorus:Commented a owl:Class .\n";
    assert_eq!(declared_subjects(before), Vec::<String>::new());
}

#[test]
fn a_predicate_that_is_not_rdf_type_is_not_a_declaration() {
    // `chorus:X label "y"` declares nothing; only `chorus:X a …` does.
    assert_eq!(declared_subjects("chorus:Alpha rdfs:label \"a\" .\n"), Vec::<String>::new());
    assert_eq!(declared_subjects("chorus:Alpha a owl:Class .\n"), vec!["Alpha".to_string()]);
}
