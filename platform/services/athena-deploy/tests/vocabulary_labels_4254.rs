//! #4254 — the within-scheme label report, and the negative proof that it can
//! still go red now that the real collision is gone.
//!
//! Two traps this file is written against, both ours from this week:
//!
//!  1. #3725 — a fixture whose marker comment contained the very string being
//!     matched, so the check "passed" for the wrong reason. Every planted
//!     collision here is planted in a `skos:prefLabel` / `skos:altLabel`
//!     statement, and `a_comment_is_not_a_claim` proves a comment carrying the
//!     same word does NOT register.
//!  2. Scope — a collision planted ACROSS two schemes cannot be seen by a
//!     within-scheme report, so a proof built on one passes vacuously.
//!     `a_cross_scheme_homonym_is_not_a_collision` pins that the split is
//!     deliberate, and every red fixture plants INSIDE one scheme.

use athena_deploy::duplicate_concept_labels;

fn f(text: &str) -> Vec<(String, String)> {
    vec![("vocab.ttl".to_string(), text.to_string())]
}

/// The live file, as authored. Wren caught the one real collision before this
/// report existed, so the honest expectation here is zero.
#[test]
fn the_authored_vocabulary_is_clean() {
    // Resolved from the crate, never from CHORUS_ROOT: that env var points at
    // canonical, and a test that reads canonical while running in a werk
    // measures the wrong tree (#3701).
    let d = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let root = format!("{d}/../../..");
    let path = format!("{root}/roles/silas/ontology/vocabulary-identity-4254.ttl");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let out = duplicate_concept_labels(&f(&text));
    assert!(out.is_empty(), "authored vocabulary has collisions: {out:?}");
}

/// NEGATIVE PROOF. A second concept claims, as an altLabel, a word an existing
/// concept already prefers — inside ONE scheme. The report must name both.
#[test]
fn an_altlabel_colliding_with_a_preflabel_in_one_scheme_is_reported() {
    let ttl = r#"
vocab:service a skos:Concept ;
    skos:inScheme vocab:identity ;
    skos:prefLabel "service" .

vocab:bot a skos:Concept ;
    skos:inScheme vocab:identity ;
    skos:prefLabel "bot" ;
    skos:altLabel "service" .
"#;
    let out = duplicate_concept_labels(&f(ttl));
    assert_eq!(out.len(), 1, "expected exactly one collision, got {out:?}");
    let line = &out[0];
    assert!(line.contains("vocab:service (prefLabel)"), "{line}");
    assert!(line.contains("vocab:bot (altLabel)"), "{line}");
    assert!(line.contains("vocab:identity"), "{line}");
}

/// NEGATIVE PROOF. Two concepts both PREFERRING one word in one scheme — the
/// exact state Wren caught in the authored file.
#[test]
fn two_preflabels_on_one_word_in_one_scheme_are_reported() {
    let ttl = r#"
vocab:agent a skos:Concept ;
    skos:inScheme vocab:identity ;
    skos:prefLabel "agent" .

vocab:launch-agent a skos:Concept ;
    skos:inScheme vocab:identity ;
    skos:prefLabel "agent" .
"#;
    let out = duplicate_concept_labels(&f(ttl));
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(out[0].contains("vocab:agent (prefLabel)"), "{}", out[0]);
    assert!(out[0].contains("vocab:launch-agent (prefLabel)"), "{}", out[0]);
}

/// The scope, pinned. The SAME two rows in two schemes are legal SKOS and must
/// NOT be reported — and this is why a proof planted across schemes would be
/// vacuous.
#[test]
fn a_cross_scheme_homonym_is_not_a_collision() {
    let ttl = r#"
vocab:agent a skos:Concept ;
    skos:inScheme vocab:identity ;
    skos:prefLabel "agent" .

vocab:launch-agent a skos:Concept ;
    skos:inScheme vocab:runtime ;
    skos:prefLabel "agent" .
"#;
    assert!(duplicate_concept_labels(&f(ttl)).is_empty());
}

/// The #3725 trap, proved shut. A comment carrying the colliding word is not a
/// claim on it, so a fixture cannot pass or fail on its own marker text.
#[test]
fn a_comment_is_not_a_claim() {
    let ttl = r#"
vocab:service a skos:Concept ;
    skos:inScheme vocab:identity ;
    skos:prefLabel "service" .

# This comment says skos:prefLabel "service" and must count for nothing.
vocab:bot a skos:Concept ;
    skos:inScheme vocab:identity ;
    skos:prefLabel "bot" .   # also skos:altLabel "service" here, in a comment
"#;
    let out = duplicate_concept_labels(&f(ttl));
    assert!(out.is_empty(), "a comment registered as a claim: {out:?}");
}

/// The words are one word whatever the casing.
#[test]
fn matching_is_case_insensitive() {
    let ttl = r#"
vocab:launch-agent a skos:Concept ;
    skos:inScheme vocab:runtime ;
    skos:prefLabel "LaunchAgent" .

vocab:unit a skos:Concept ;
    skos:inScheme vocab:runtime ;
    skos:prefLabel "unit" ;
    skos:altLabel "launchagent" .
"#;
    assert_eq!(duplicate_concept_labels(&f(ttl)).len(), 1);
}

/// A concept with no scheme cannot be scoped, so it is not silently pooled
/// with everything else — that would invent collisions.
#[test]
fn a_concept_with_no_scheme_is_not_pooled() {
    let ttl = r#"
vocab:a a skos:Concept ;
    skos:prefLabel "orphan" .

vocab:b a skos:Concept ;
    skos:prefLabel "orphan" .
"#;
    assert!(duplicate_concept_labels(&f(ttl)).is_empty());
}

/// The collision usually spans two roles' files, like #4250's did.
#[test]
fn a_collision_across_two_files_is_reported() {
    let files = vec![
        (
            "a.ttl".to_string(),
            "vocab:service a skos:Concept ;\n    skos:inScheme vocab:identity ;\n    skos:prefLabel \"service\" .\n".to_string(),
        ),
        (
            "b.ttl".to_string(),
            "vocab:worker a skos:Concept ;\n    skos:inScheme vocab:identity ;\n    skos:prefLabel \"service\" .\n".to_string(),
        ),
    ];
    let out = duplicate_concept_labels(&files);
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(out[0].contains("a.ttl:"), "{}", out[0]);
    assert!(out[0].contains("b.ttl:"), "{}", out[0]);
}

// --- the cross-scheme count: the blind spot, made countable ----------------

use athena_deploy::cross_scheme_repeats;

/// The authored file today. Asserted as a LIST, not a number, so adding a
/// fourth homonym goes red for a reason a person can read rather than because
/// 2 became 3.
#[test]
fn the_authored_vocabulary_repeats_exactly_these_words_across_schemes() {
    let d = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let path = format!("{d}/../../../roles/silas/ontology/vocabulary-identity-4254.ttl");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let out = cross_scheme_repeats(&f(&text));
    assert_eq!(
        out,
        vec![
            "\"agent\" in vocab:identity and vocab:runtime".to_string(),
            "\"session\" in vocab:identity and vocab:runtime".to_string(),
        ],
        "cross-scheme repeats changed"
    );
}

/// The two numbers separate the two states. This is the case the within-scheme
/// report is BLIND to — and the one the cross-scheme count exists to surface.
#[test]
fn a_word_in_two_schemes_is_a_repeat_and_not_a_collision() {
    let ttl = r#"
vocab:agent a skos:Concept ;
    skos:inScheme vocab:identity ;
    skos:prefLabel "agent" .

vocab:launch-agent a skos:Concept ;
    skos:inScheme vocab:runtime ;
    skos:prefLabel "agent" .
"#;
    assert!(duplicate_concept_labels(&f(ttl)).is_empty(), "not a collision");
    assert_eq!(cross_scheme_repeats(&f(ttl)).len(), 1, "but it IS a repeat");
}

/// NEGATIVE PROOF for the count: two concepts in ONE scheme are a collision
/// and NOT a cross-scheme repeat. If this returned 1, the two numbers would be
/// the same number twice and neither could be trusted.
#[test]
fn one_scheme_is_a_collision_and_not_a_repeat() {
    let ttl = r#"
vocab:a a skos:Concept ;
    skos:inScheme vocab:identity ;
    skos:prefLabel "agent" .

vocab:b a skos:Concept ;
    skos:inScheme vocab:identity ;
    skos:prefLabel "agent" .
"#;
    assert_eq!(duplicate_concept_labels(&f(ttl)).len(), 1);
    assert!(cross_scheme_repeats(&f(ttl)).is_empty(), "same scheme is not a repeat");
}

/// An altLabel is not a claim on the preferred word across schemes — only a
/// prefLabel is. Otherwise "machine" as a variant of host would read as a
/// repeat against any scheme that prefers it.
#[test]
fn an_altlabel_does_not_count_as_a_cross_scheme_repeat() {
    let ttl = r#"
vocab:host a skos:Concept ;
    skos:inScheme vocab:runtime ;
    skos:prefLabel "host" ;
    skos:altLabel "machine" .

vocab:machine a skos:Concept ;
    skos:inScheme vocab:identity ;
    skos:prefLabel "machine" .
"#;
    assert!(cross_scheme_repeats(&f(ttl)).is_empty());
}

// --- ungrounded terms: the marker nobody read, turned into a count ---------

use athena_deploy::ungrounded_concepts;

/// The authored file. Named, not counted — "stage" is the one term that names
/// nothing existing, and it says so in its own note.
#[test]
fn only_the_proposed_term_names_nothing_in_the_model() {
    let d = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let path = format!("{d}/../../../roles/silas/ontology/vocabulary-identity-4254.ttl");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let mut out = ungrounded_concepts(&f(&text));
    out.sort();
    // NAMED, not counted. One term names nothing that exists: the class ruling
    // 1 proposes. #4302 grounded the other — the everyday word for what a nudge
    // is delivered to is now chorus:SessionRun — so it must NOT come back.
    assert_eq!(out.len(), 1, "ungrounded set changed: {out:?}");
    assert!(out.iter().any(|o| o.contains("vocab:stage")), "{out:?}");
    assert!(!out.iter().any(|o| o.contains("vocab:terminal-session")), "#4302 grounded terminal-session: {out:?}");
}

/// NEGATIVE PROOF. A concept WITH an exactMatch must not be counted — if it
/// were, the number would be "how many concepts are there" wearing another
/// name, and could never go down.
#[test]
fn a_concept_with_an_exactmatch_is_not_counted() {
    let ttl = r#"
vocab:service a skos:Concept ;
    skos:inScheme vocab:identity ;
    skos:prefLabel "service" ;
    skos:exactMatch chorus:principalKind .
"#;
    assert!(ungrounded_concepts(&f(ttl)).is_empty());
}

/// NEGATIVE PROOF. A concept WITHOUT one must be counted, and named by its
/// preferred word rather than its IRI, so the report is readable by a person.
#[test]
fn a_concept_with_no_exactmatch_is_counted_and_named() {
    let ttl = r#"
vocab:stage a skos:Concept ;
    skos:inScheme vocab:governance ;
    skos:prefLabel "stage" .
"#;
    let out = ungrounded_concepts(&f(ttl));
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(out[0].contains("\"stage\""), "{}", out[0]);
    assert!(out[0].contains("vocab:stage"), "{}", out[0]);
}

/// A PROPOSED note is not what makes a term proposed — the missing edge is.
/// Counting the note instead would let a comment and the data disagree, which
/// is the state this replaced.
#[test]
fn the_note_is_not_what_decides_it() {
    let ttl = r#"
vocab:a a skos:Concept ;
    skos:inScheme vocab:identity ;
    skos:prefLabel "a" ;
    skos:note "PROPOSED, not agreed." ;
    skos:exactMatch chorus:Something .
"#;
    assert!(
        ungrounded_concepts(&f(ttl)).is_empty(),
        "a PROPOSED note on a grounded term must not count"
    );
}

/// A ConceptScheme is not a term and must never be counted as an ungrounded
/// one — three scheme titles would otherwise read as three proposed words.
#[test]
fn a_scheme_is_not_a_term() {
    let ttl = r#"
vocab:identity a skos:ConceptScheme ;
    skos:prefLabel "Chorus identity vocabulary" .
"#;
    assert!(ungrounded_concepts(&f(ttl)).is_empty());
}
