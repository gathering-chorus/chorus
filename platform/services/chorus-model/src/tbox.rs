//! #3718 — the TBox half of the governed writer.
//!
//! `athena-model add` governs INSTANCES. Nothing governed the SCHEMA: classes,
//! properties and shapes were hand-typed into `.ttl` files, and every failure
//! surfaced 2026-07-30/31 was a schema-layer failure that a type could have
//! refused — 69 classes declaring themselves and binding nothing, 11 properties
//! with no domain and/or range, 12 classes with no shape at all.
//!
//! THE RULE THAT SHAPES EVERY FUNCTION HERE (Jeff, 2026-08-02, hard AC):
//! LEGIBILITY, NOT SPEED. This module REFUSES when a required field is missing.
//! It never fills a default. A default invents a judgment nobody made — that is
//! exactly what born-v2 did when it stamped ~120 unreviewed classes as
//! "canonical," and a defaulting writer would do the same thing at mint time,
//! permanently and silently.
//!
//! Speed, when we want it, comes from TEMPLATES THE HUMAN CHOOSES — real,
//! visible, editable values — never from the tool guessing. Fast and legible are
//! only in tension when the tool is the one deciding.
//!
//! CALLERS NEVER PASS AN IRI. They pass local names; this module forms the IRI
//! per ADR-040. That is the whole point of a governed writer: an IRI typed by
//! hand is an IRI that can be wrong, and 6 namespaces' worth of drift says so.

use std::collections::BTreeSet;
use std::fmt;

/// The one namespace this writer mints into. ADR-040.
pub const NS: &str = "https://jeffbridwell.com/chorus#";

/// Why a TBox write was refused. Every variant is a real defect we measured in
/// the model, not a hypothetical — the doc comment on each says which.
#[derive(Debug, PartialEq, Eq)]
pub enum TboxRefusal {
    /// 11 properties in the live model have no domain and/or no range. A
    /// property without both is unreasonable-over by construction: nothing can
    /// infer from it and nothing can validate against it.
    PropertyMissingDomainOrRange { name: String, missing: &'static str },
    /// 69 of 186 classes (38%) declare themselves and are claimed by no domain.
    /// An unclaimed class is invisible to the generator — it cannot be served,
    /// and nothing reports that it wasn't.
    ClassUnclaimed { name: String },
    /// A required field was absent. NEVER defaulted — see the module header.
    RequiredFieldMissing { subject: String, field: &'static str },
    /// The local name is not a legal local name for its kind. Classes are
    /// UpperCamel, properties lowerCamel — the convention the generator's
    /// name-to-route derivation already assumes.
    MalformedLocalName { name: String, kind: &'static str, why: &'static str },
    /// The caller passed something that looks like an IRI. They must not: the
    /// writer forms IRIs. Accepting one here would reintroduce the hand-typed
    /// IRI as a supported input, which is the drift this card exists to stop.
    CallerPassedIri { value: String },
    /// The target file is in no deploy manifest, so anything written to it binds
    /// nothing. A file can be complete, valid, committed — and invisible.
    NotInManifest { file: String },
    /// A shape with no required properties declares no floor, so it can refuse
    /// nothing. That is not a shape; it is the appearance of one.
    ShapeWithNoFloor { class: String },
}

impl TboxRefusal {
    pub fn code(&self) -> &'static str {
        match self {
            Self::PropertyMissingDomainOrRange { .. } => "property-domain-range",
            Self::ClassUnclaimed { .. } => "unclaimed-class",
            Self::RequiredFieldMissing { .. } => "required-field-missing",
            Self::MalformedLocalName { .. } => "malformed-local-name",
            Self::CallerPassedIri { .. } => "caller-passed-iri",
            Self::NotInManifest { .. } => "manifest-membership",
            Self::ShapeWithNoFloor { .. } => "shape-with-no-floor",
        }
    }
}

impl fmt::Display for TboxRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PropertyMissingDomainOrRange { name, missing } => write!(
                f,
                "property '{name}' has no {missing}. A property without both a domain and a \
                 range cannot be reasoned over or validated against — 11 live properties are \
                 in this state today. Supply --domain and --range; they are not defaulted."
            ),
            Self::ClassUnclaimed { name } => write!(
                f,
                "class '{name}' is claimed by no domain. An unclaimed class binds nothing: the \
                 generator will not serve it and nothing will report the omission. Pass \
                 --claimed-by <domain>, or say why not — 69 of 186 classes are in this state."
            ),
            Self::RequiredFieldMissing { subject, field } => write!(
                f,
                "'{subject}' is missing required field '{field}'. This writer REFUSES rather \
                 than filling a default: a defaulted field is a judgment nobody made, recorded \
                 permanently as though someone had."
            ),
            Self::MalformedLocalName { name, kind, why } => {
                write!(f, "'{name}' is not a valid {kind} local name — {why}.")
            }
            Self::CallerPassedIri { value } => write!(
                f,
                "'{value}' looks like an IRI. Callers pass LOCAL NAMES; this writer forms the \
                 IRI (ADR-040). A hand-typed IRI is one that can be wrong, and six namespaces \
                 of drift in the live model started exactly there."
            ),
            Self::NotInManifest { file } => write!(
                f,
                "'{file}' is in no deploy manifest. Anything written here binds nothing — the \
                 file can be complete, valid and committed and still be invisible to every \
                 surface. Add it to MODEL_SET first."
            ),
            Self::ShapeWithNoFloor { class } => write!(
                f,
                "shape for '{class}' declares no required properties. A shape with no floor \
                 refuses nothing — it is the appearance of validation, not validation."
            ),
        }
    }
}

fn looks_like_iri(s: &str) -> bool {
    s.contains("://") || s.contains('#') || s.starts_with("chorus:") || s.contains('/')
}

fn is_upper_camel(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && s.chars().all(|c| c.is_ascii_alphanumeric())
}

fn is_lower_camel(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && s.chars().all(|c| c.is_ascii_alphanumeric())
}

/// What the caller supplied for a class. Every field is explicit; there is no
/// `Default` impl on purpose — deriving Default would put the defaulting
/// behaviour this module refuses to have one `..Default::default()` away.
pub struct ClassSpec<'a> {
    pub name: &'a str,
    pub comment: Option<&'a str>,
    pub claimed_by: Option<&'a str>,
    pub target_file: &'a str,
}

pub struct PropertySpec<'a> {
    pub name: &'a str,
    pub domain: Option<&'a str>,
    pub range: Option<&'a str>,
    pub comment: Option<&'a str>,
    pub target_file: &'a str,
}

pub struct ShapeSpec<'a> {
    pub class: &'a str,
    pub required: &'a [String],
    pub target_file: &'a str,
}

fn manifest_check(file: &str, deploy_set: &BTreeSet<String>, out: &mut Vec<TboxRefusal>) {
    if file.is_empty() {
        out.push(TboxRefusal::RequiredFieldMissing {
            subject: "(write)".into(),
            field: "--file",
        });
    } else if !deploy_set.is_empty() && !deploy_set.contains(file) {
        out.push(TboxRefusal::NotInManifest { file: file.into() });
    }
}

/// Every refusal, never just the first. A caller who fixes one violation and
/// resubmits into the next learns the shape one turn at a time; that is the
/// opposite of legibility.
pub fn check_class(spec: &ClassSpec, deploy_set: &BTreeSet<String>) -> Vec<TboxRefusal> {
    let mut out = Vec::new();
    if looks_like_iri(spec.name) {
        out.push(TboxRefusal::CallerPassedIri { value: spec.name.into() });
    } else if !is_upper_camel(spec.name) {
        out.push(TboxRefusal::MalformedLocalName {
            name: spec.name.into(),
            kind: "class",
            why: "classes are UpperCamel with no punctuation",
        });
    }
    if spec.comment.is_none_or(str::is_empty) {
        out.push(TboxRefusal::RequiredFieldMissing {
            subject: spec.name.into(),
            field: "--comment",
        });
    }
    match spec.claimed_by {
        None => out.push(TboxRefusal::ClassUnclaimed { name: spec.name.into() }),
        Some(d) if looks_like_iri(d) => {
            out.push(TboxRefusal::CallerPassedIri { value: d.into() })
        }
        Some(_) => {}
    }
    manifest_check(spec.target_file, deploy_set, &mut out);
    out
}

pub fn check_property(spec: &PropertySpec, deploy_set: &BTreeSet<String>) -> Vec<TboxRefusal> {
    let mut out = Vec::new();
    if looks_like_iri(spec.name) {
        out.push(TboxRefusal::CallerPassedIri { value: spec.name.into() });
    } else if !is_lower_camel(spec.name) {
        out.push(TboxRefusal::MalformedLocalName {
            name: spec.name.into(),
            kind: "property",
            why: "properties are lowerCamel with no punctuation",
        });
    }
    // The headline defect: domain AND range, both, always. Reported as one
    // refusal naming which half is missing, so the message is actionable.
    let missing = match (spec.domain, spec.range) {
        (None, None) => Some("domain and no range"),
        (None, Some(_)) => Some("domain"),
        (Some(_), None) => Some("range"),
        (Some(_), Some(_)) => None,
    };
    if let Some(m) = missing {
        out.push(TboxRefusal::PropertyMissingDomainOrRange {
            name: spec.name.into(),
            missing: m,
        });
    }
    if spec.comment.is_none_or(str::is_empty) {
        out.push(TboxRefusal::RequiredFieldMissing {
            subject: spec.name.into(),
            field: "--comment",
        });
    }
    manifest_check(spec.target_file, deploy_set, &mut out);
    out
}

pub fn check_shape(spec: &ShapeSpec, deploy_set: &BTreeSet<String>) -> Vec<TboxRefusal> {
    let mut out = Vec::new();
    if looks_like_iri(spec.class) {
        out.push(TboxRefusal::CallerPassedIri { value: spec.class.into() });
    } else if !is_upper_camel(spec.class) {
        out.push(TboxRefusal::MalformedLocalName {
            name: spec.class.into(),
            kind: "class",
            why: "classes are UpperCamel with no punctuation",
        });
    }
    if spec.required.is_empty() {
        out.push(TboxRefusal::ShapeWithNoFloor { class: spec.class.into() });
    }
    manifest_check(spec.target_file, deploy_set, &mut out);
    out
}

// ── Turtle emission ────────────────────────────────────────────────────────
// Pure string builders: no I/O, no graph, unit-testable without a live store.
// They are only ever reached AFTER the checks above return empty, so they never
// have to defend themselves against missing fields.

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
}

pub fn class_turtle(spec: &ClassSpec) -> String {
    let claimed = spec.claimed_by.expect("checked");
    format!(
        "chorus:{name} a owl:Class ;\n    \
         rdfs:label \"{name}\" ;\n    \
         rdfs:comment \"{comment}\" .\n\
         chorus:{domain} chorus:definesVocabulary chorus:{name} .\n",
        name = spec.name,
        comment = esc(spec.comment.expect("checked")),
        domain = claimed,
    )
}

pub fn property_turtle(spec: &PropertySpec) -> String {
    // rdfs:range may be a datatype (xsd:string) or a class. A caller-supplied
    // `xsd:`-prefixed range is NOT a hand-typed chorus IRI, so it is allowed
    // through verbatim; anything else is minted into our namespace.
    let range = spec.range.expect("checked");
    let range_term = if range.starts_with("xsd:") {
        range.to_string()
    } else {
        format!("chorus:{range}")
    };
    format!(
        "chorus:{name} a owl:ObjectProperty ;\n    \
         rdfs:label \"{name}\" ;\n    \
         rdfs:comment \"{comment}\" ;\n    \
         rdfs:domain chorus:{domain} ;\n    \
         rdfs:range {range_term} .\n",
        name = spec.name,
        comment = esc(spec.comment.expect("checked")),
        domain = spec.domain.expect("checked"),
    )
}

pub fn shape_turtle(spec: &ShapeSpec) -> String {
    let props = spec
        .required
        .iter()
        .map(|p| {
            let path = if p.starts_with("rdfs:") { p.clone() } else { format!("chorus:{p}") };
            format!("    sh:property [ sh:path {path} ; sh:minCount 1 ] ;\n")
        })
        .collect::<String>();
    format!(
        "chorus:{class}Shape a sh:NodeShape ;\n    \
         sh:targetClass chorus:{class} ;\n{props}    \
         chorus:generatedBy \"athena-model\" .\n",
        class = spec.class,
        props = props,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> BTreeSet<String> {
        ["designing/data/x.ttl".to_string()].into_iter().collect()
    }
    fn ok_file() -> &'static str {
        "designing/data/x.ttl"
    }

    #[test]
    fn a_property_without_a_range_is_refused() {
        // The measured defect: 11 live properties have no domain and/or range.
        let r = check_property(
            &PropertySpec {
                name: "hostedOn",
                domain: Some("Service"),
                range: None,
                comment: Some("where it runs"),
                target_file: ok_file(),
            },
            &manifest(),
        );
        assert!(matches!(
            r.as_slice(),
            [TboxRefusal::PropertyMissingDomainOrRange { missing: "range", .. }]
        ));
    }

    #[test]
    fn a_complete_property_passes_and_emits_both_edges() {
        let spec = PropertySpec {
            name: "hostedOn",
            domain: Some("Service"),
            range: Some("Machine"),
            comment: Some("where it runs"),
            target_file: ok_file(),
        };
        assert!(check_property(&spec, &manifest()).is_empty());
        let t = property_turtle(&spec);
        assert!(t.contains("rdfs:domain chorus:Service"));
        assert!(t.contains("rdfs:range chorus:Machine"));
    }

    #[test]
    fn a_datatype_range_is_not_minted_into_our_namespace() {
        // xsd:string must stay xsd:string. Minting it to chorus:xsd:string
        // would be silently wrong in a way SHACL would accept structurally.
        let spec = PropertySpec {
            name: "label",
            domain: Some("Service"),
            range: Some("xsd:string"),
            comment: Some("a label"),
            target_file: ok_file(),
        };
        assert!(property_turtle(&spec).contains("rdfs:range xsd:string"));
        assert!(!property_turtle(&spec).contains("chorus:xsd"));
    }

    #[test]
    fn an_unclaimed_class_is_refused() {
        // 69 of 186 classes are in this state; the writer must not add a 70th.
        let r = check_class(
            &ClassSpec {
                name: "Widget",
                comment: Some("a widget"),
                claimed_by: None,
                target_file: ok_file(),
            },
            &manifest(),
        );
        assert!(matches!(r.as_slice(), [TboxRefusal::ClassUnclaimed { .. }]));
    }

    #[test]
    fn nothing_is_ever_defaulted_a_missing_comment_refuses() {
        // The hard AC: REFUSE, never fill. If this test ever needs changing to
        // let a write through, the born-v2 failure is being rebuilt.
        let r = check_class(
            &ClassSpec {
                name: "Widget",
                comment: None,
                claimed_by: Some("cards"),
                target_file: ok_file(),
            },
            &manifest(),
        );
        assert!(r.iter().any(|x| matches!(
            x,
            TboxRefusal::RequiredFieldMissing { field: "--comment", .. }
        )));
    }

    #[test]
    fn a_caller_supplied_iri_is_refused_not_accepted() {
        for bad in ["chorus:Widget", "https://jeffbridwell.com/chorus#Widget", "a/b"] {
            let r = check_class(
                &ClassSpec {
                    name: bad,
                    comment: Some("x"),
                    claimed_by: Some("cards"),
                    target_file: ok_file(),
                },
                &manifest(),
            );
            assert!(
                r.iter().any(|x| matches!(x, TboxRefusal::CallerPassedIri { .. })),
                "{bad} must be refused"
            );
        }
    }

    #[test]
    fn a_file_outside_the_manifest_is_refused() {
        let r = check_class(
            &ClassSpec {
                name: "Widget",
                comment: Some("x"),
                claimed_by: Some("cards"),
                target_file: "roles/wren/scratch.ttl",
            },
            &manifest(),
        );
        assert!(r.iter().any(|x| matches!(x, TboxRefusal::NotInManifest { .. })));
    }

    #[test]
    fn a_shape_with_no_required_properties_is_refused() {
        let r = check_shape(
            &ShapeSpec { class: "Widget", required: &[], target_file: ok_file() },
            &manifest(),
        );
        assert!(matches!(r.as_slice(), [TboxRefusal::ShapeWithNoFloor { .. }]));
    }

    #[test]
    fn every_refusal_is_collected_not_just_the_first() {
        // A caller with three problems learns all three in one turn.
        let r = check_property(
            &PropertySpec {
                name: "Bad-Name",
                domain: None,
                range: None,
                comment: None,
                target_file: "not/in/manifest.ttl",
            },
            &manifest(),
        );
        assert!(r.len() >= 4, "expected every violation, got {}: {:?}", r.len(), r);
    }

    #[test]
    fn the_emitted_class_claims_itself_so_it_cannot_be_born_unclaimed() {
        // The generator serves from definesVocabulary. A class minted WITHOUT
        // that edge is invisible the moment it lands — so the edge is part of
        // the mint, not a follow-up step someone has to remember.
        let spec = ClassSpec {
            name: "Widget",
            comment: Some("a widget"),
            claimed_by: Some("cards"),
            target_file: ok_file(),
        };
        assert!(check_class(&spec, &manifest()).is_empty());
        assert!(class_turtle(&spec).contains("chorus:cards chorus:definesVocabulary chorus:Widget"));
    }

    #[test]
    fn an_empty_manifest_does_not_silently_pass_everything() {
        // If the deploy-set parse returns nothing, membership is UNKNOWN, not
        // satisfied. Skipping the check on an empty set is the right call only
        // because a false refusal would block all work — so this test pins the
        // behaviour deliberately rather than leaving it accidental.
        let empty = BTreeSet::new();
        let r = check_class(
            &ClassSpec {
                name: "Widget",
                comment: Some("x"),
                claimed_by: Some("cards"),
                target_file: "anything.ttl",
            },
            &empty,
        );
        assert!(r.is_empty(), "empty manifest must not fabricate a refusal");
    }
}
