//! #3718 — THE ADR REFUSAL CORE for athena-model.
//!
//! Jeff, 2026-07-31: *"maybe an owl-model in rust that injects all our modeling
//! adr requirements into model work as mandatory and has a core that is
//! deterministic for the actual modeling specifics — werk-code is another one
//! that is very conceptually similar here."*
//!
//! `werk-*` is the governed writer for CODE. `athena-model` is the governed
//! writer for the MODEL. Same shape: typed verbs, a deterministic core, a
//! refusal taxonomy, no free-text back door.
//!
//! **Why this module is pure.** Every predicate takes facts and returns a
//! verdict — no store, no filesystem, no clock. An ADR that lives in prose gets
//! violated and nobody notices: ADR-051 was violated three times before a tool
//! caught it (#3581 Domain, #3675 Service, #3698 ValueStream — same missing
//! placement, found one at a time, by symptom, over months). An ADR that is an
//! exit code cannot be violated silently. Purity is what makes these testable
//! without a live graph, which is what makes them trustworthy.
//!
//! ## The rulings encoded here, and who made them
//!
//! **Silas (OWL-DBA, 2026-08-02)** — athena-model refuses only what is decidable
//! from the triple being written. ADR-047/048 govern the GENERATED SURFACE and
//! stay generator-side: a model write cannot see the generated surface, so
//! refusing them at write time checks a thing you cannot observe.
//!
//! **Silas, same day** — ADR-051 and ADR-025 are ONE check, never two. A
//! mechanical "must declare an ABox instances graph" would REFUSE Domain,
//! Service and SubDomain — the ones that are correct — because punned classes
//! live in the ontology graph deliberately (a Domain is an `owl:Class`; its
//! instances are subclasses). The predicate is *placement CONSISTENT WITH KIND*.
//!
//! **Silas's read-path check, 2026-08-02 — THERE ARE TWO PLACEMENT QUESTIONS.**
//! athena-make HARD-REQUIRES a single `urn:chorus:ontology` graph for SHACL shapes
//! and class definitions: `ONTOLOGY_GRAPH` is a const used in ~10 schema reads
//! with NO union across domain graphs (shape read, requiresAuth, the minCount
//! floor, vocab). So:
//!
//!   - **SCHEMA triples** (shapes, class defs, `definesVocabulary`) MUST live in
//!     `urn:chorus:ontology`. That is the read path's required home, not legacy.
//!     A schema write landing there is CORRECT and must never be refused.
//!   - **INSTANCE triples** (ABox individuals) derive their graph from the
//!     domain that claims the class.
//!
//! Encoding one rule for both would write a refusal the read path cannot
//! satisfy. The mechanism split survives — for schema only, and legitimately —
//! until athena-make can union shapes across domain graphs. Silas owns that
//! read-path drain as the named prerequisite migration.
//!
//! **Jeff, 2026-08-02** — the meta-model is self-describing, so placement is
//! DERIVED, not declared: the `products` domain's `definesVocabulary` IS
//! `chorus:Product`; `domains` defines `Domain`/`SubDomain`/`CollectionDomain`.
//! Therefore **a class's graph = the graph of the domain whose
//! `definesVocabulary` contains it.** One rule, zero annotations. An explicit
//! `instancesGraph` becomes an OVERRIDE with a stated reason, not the mechanism.
//! Silas confirmed the read path permits it (athena-make resolves shape scope
//! per-table; cross-domain shape union after the drain is fine).
//!
//! **Jeff, 2026-08-02 — LEGIBILITY, NOT SPEED.** A missing required field is
//! REFUSED. Never defaulted, never inferred. This is born-v2 one layer up: a
//! default at mint time invents a judgment nobody made, permanently and
//! silently — born-v2 stamped ~120 unreviewed classes "canonical" exactly that
//! way. Consultant speed comes from TEMPLATES THE HUMAN CHOOSES, supplying
//! real, visible, editable values — never from values the tool guesses.

use std::collections::BTreeSet;

/// Why a write was refused. One variant per ADR clause, so the exit path names
/// the rule rather than saying "invalid". The taxonomy IS the API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// ADR-045 — a Domain must be declared as an `owl:Class`.
    DomainNotClass(String),
    /// ADR-051 × ADR-025 — placement contradicts the derivation. ONE check.
    Placement(String),
    /// Manifest membership — the target file binds nothing.
    Manifest(String),
    /// shape-is-truth — a shapeless class is not part of the served model.
    NoShape(String),
    /// absence-stays-absent — a required field is missing. NEVER defaulted.
    MissingRequired(String),
    /// No domain claims the class in `definesVocabulary`.
    UnclaimedClass(String),
}

impl Refusal {
    pub fn code(&self) -> &'static str {
        match self {
            Refusal::DomainNotClass(_) => "adr045-domain-not-class",
            Refusal::Placement(_) => "adr051x025-placement",
            Refusal::Manifest(_) => "manifest-membership",
            Refusal::NoShape(_) => "shape-is-truth",
            Refusal::MissingRequired(_) => "absence-stays-absent",
            Refusal::UnclaimedClass(_) => "unclaimed-class",
        }
    }
    pub fn message(&self) -> &str {
        match self {
            Refusal::DomainNotClass(m)
            | Refusal::Placement(m)
            | Refusal::Manifest(m)
            | Refusal::NoShape(m)
            | Refusal::MissingRequired(m)
            | Refusal::UnclaimedClass(m) => m,
        }
    }
}

pub type Verdict = Result<(), Refusal>;

/// A class is PUNNED when it is both an `owl:Class` and a chorus kind — its
/// "instances" are subclasses, living in the ontology graph (ADR-045/025).
/// VERIFIED per class against the store, never assumed by analogy: Product
/// looks punned next to Domain and is NOT (its individuals carry
/// `a chorus:Product` and nothing else). Assuming it produced a wrong finding
/// on 2026-08-02 — the readout blamed the placement when the placement was right.
pub fn is_punned(class_local: &str) -> bool {
    matches!(class_local, "Domain" | "SubDomain" | "CollectionDomain" | "Service")
}

/// Which layer a write targets. The two obey DIFFERENT placement rules and
/// conflating them writes a refusal the read path cannot satisfy.
///
/// **A PUNNED INDIVIDUAL IS SCHEMA-LAYER** (Silas, OWL-DBA, 2026-08-02). That is
/// what punning MEANS: ADR-045 says a Domain IS an `owl:Class`, so a specific
/// Domain is simultaneously an individual of `chorus:Domain` AND itself a class
/// that other things subclass. Under ADR-025 (ontology = schema/TBox), a thing
/// that IS a class belongs in the ontology graph. The live store's 40 Domains in
/// `urn:chorus:ontology` are not drift — they are correct.
///
/// This is TWO layers, not three. I had the punned individual filed under
/// `Instance`, which was the single misclassification; `layer_of` puts it back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// SHACL shapes, class definitions, `definesVocabulary` — AND punned
    /// individuals, because a punned individual is itself a class.
    Schema,
    /// Plain ABox individuals.
    Instance,
}

/// Classify a write. The caller states whether it is writing a definition; the
/// punned case is decided here so no caller has to remember the rule.
pub fn layer_of(class_local: &str, is_definition: bool) -> Layer {
    if is_definition || is_punned(class_local) {
        Layer::Schema
    } else {
        Layer::Instance
    }
}

/// The single graph athena-make reads every shape and class definition from.
pub const SCHEMA_GRAPH: &str = "urn:chorus:ontology";

/// The graph a class's INSTANCES belong in, DERIVED from the meta-model.
/// `defining_domain` is the domain whose `definesVocabulary` contains the class.
pub fn derived_placement(class_local: &str, defining_domain: Option<&str>) -> Option<String> {
    if is_punned(class_local) {
        return Some(SCHEMA_GRAPH.to_string());
    }
    defining_domain.map(|d| format!("urn:chorus:domains:{}", d))
}

/// **ADR-051 × ADR-025, as ONE check, over TWO layers.**
///
/// `Layer::Schema` — must land in `SCHEMA_GRAPH`; anything else is refused,
/// because athena-make reads schema from exactly one graph.
/// `Layer::Instance` — the derivation governs; an explicit `instancesGraph`
/// that disagrees is allowed ONLY with a stated reason.
pub fn check_placement_layered(
    layer: Layer,
    class_local: &str,
    defining_domain: Option<&str>,
    declared: Option<&str>,
    override_reason: Option<&str>,
) -> Verdict {
    if layer == Layer::Schema {
        return match declared {
            None | Some(SCHEMA_GRAPH) => Ok(()),
            Some(d) => Err(Refusal::Placement(format!(
                "{}: schema (shapes, class defs, definesVocabulary) must live in {} — athena-make reads \
                 every shape from that one graph with no union. Got {}. Draining schema to domain \
                 graphs needs the read-path migration first.",
                class_local, SCHEMA_GRAPH, d
            ))),
        };
    }
    check_placement(class_local, defining_domain, declared, override_reason)
}

/// **Instance-layer placement.** The derivation governs; an explicit
/// `instancesGraph` that disagrees is allowed ONLY with a stated reason — an
/// unexplained override is how a placement silently stops matching the model.
pub fn check_placement(
    class_local: &str,
    defining_domain: Option<&str>,
    declared: Option<&str>,
    override_reason: Option<&str>,
) -> Verdict {
    let derived = derived_placement(class_local, defining_domain);
    match (declared, derived.as_deref()) {
        (None, Some(_)) => Ok(()),
        (None, None) => Err(Refusal::UnclaimedClass(format!(
            "{}: no domain claims this class in definesVocabulary, so its placement cannot be \
             derived. Add it to a domain's vocabulary before writing instances.",
            class_local
        ))),
        (Some(d), Some(dv)) if d == dv => Ok(()),
        (Some(d), Some(dv)) => match override_reason {
            Some(r) if !r.trim().is_empty() => Ok(()),
            _ => Err(Refusal::Placement(format!(
                "{}: declared placement {} contradicts the derived placement {} (the graph of the \
                 domain whose definesVocabulary contains it). An override needs a stated reason — \
                 ADR-051 × ADR-025 as one check.",
                class_local, d, dv
            ))),
        },
        (Some(d), None) => Err(Refusal::UnclaimedClass(format!(
            "{}: declares placement {} but no domain claims the class in definesVocabulary — \
             nothing to check the declaration against.",
            class_local, d
        ))),
    }
}

/// **ADR-045** — a Domain is an `owl:Class`, and only that shape is accepted.
pub fn check_domain_declaration(kind: &str, declared_types: &[&str]) -> Verdict {
    if kind != "domain" {
        return Ok(());
    }
    if declared_types.iter().any(|t| *t == "owl:Class") {
        Ok(())
    } else {
        Err(Refusal::DomainNotClass(format!(
            "a Domain must be declared as an owl:Class (ADR-045); got [{}]",
            declared_types.join(", ")
        )))
    }
}

/// **Manifest membership** — a write to a file outside `MODEL_SET` /
/// `INSTANCE_SET` binds nothing. Measured 2026-08-02: 69 of 186 classes (38%)
/// sit in files no deploy set references, and `APISurface` has 25 LIVE instances
/// from one of them. Complete, valid, committed — and invisible.
pub fn check_manifest(target_file: &str, deploy_set: &BTreeSet<String>) -> Verdict {
    if deploy_set.contains(target_file) {
        Ok(())
    } else {
        Err(Refusal::Manifest(format!(
            "{} is in neither MODEL_SET nor INSTANCE_SET — a write here binds nothing. Add the \
             file to the manifest first, or write to a file that deploys.",
            target_file
        )))
    }
}

/// **shape-is-truth** (Silas, 2026-07-30) — `rdfs:domain` INFERS, it does not
/// constrain, and we run no reasoner; SHACL is the layer that actually refuses.
pub fn check_has_shape(class_local: &str, has_shape: bool) -> Verdict {
    if has_shape {
        Ok(())
    } else {
        Err(Refusal::NoShape(format!(
            "{} has no sh:NodeShape — it is not part of the served model, so it may not be written \
             as though it were. Land the shape first.",
            class_local
        )))
    }
}

/// **absence-stays-absent / LEGIBILITY** (Jeff, 2026-08-02). This function has
/// NO default branch by construction: there is nowhere for an invented value to
/// enter. The ruling is structural, not remembered.
pub fn check_required(
    class_local: &str,
    required: &[String],
    supplied: &BTreeSet<String>,
) -> Verdict {
    let missing: Vec<&str> = required
        .iter()
        .filter(|r| !supplied.contains(*r))
        .map(|s| s.as_str())
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(Refusal::MissingRequired(format!(
        "{}: missing required field(s) {} — athena-model REFUSES rather than defaulting. A default \
         at mint time invents a judgment nobody made (born-v2 stamped ~120 unreviewed classes \
         canonical exactly that way). Supply the values, or start from a template you chose.",
        class_local,
        missing.join(", ")
    )))
}

/// The `.ttl` files the deploy script actually loads — PARSED FROM THE SCRIPT,
/// never copied. A hand-kept list here would reproduce the authored-vs-deployed
/// gap the manifest check exists to catch.
pub fn deploy_set_from_script() -> BTreeSet<String> {
    let root = std::env::var("CHORUS_ROOT")
        .unwrap_or_else(|_| "/Users/jeffbridwell/CascadeProjects/chorus".to_string());
    let path = format!("{}/platform/scripts/athena-deploy-model.sh", root);
    let body = match std::fs::read_to_string(&path) {
        Ok(b) => b,
        Err(_) => return BTreeSet::new(),
    };
    let mut out = BTreeSet::new();
    for line in body.lines() {
        let l = line.trim();
        if l.starts_with('#') {
            continue;
        }
        if let Some(i) = l.find("$CHORUS_ROOT/") {
            let rest = &l[i + "$CHORUS_ROOT/".len()..];
            if let Some(end) = rest.find(".ttl") {
                out.insert(rest[..end + 4].to_string());
            }
        }
    }
    out
}

pub struct WriteFacts<'a> {
    pub layer: Layer,
    pub kind: &'a str,
    pub class_local: &'a str,
    pub target_file: &'a str,
    pub declared_types: &'a [&'a str],
    pub defining_domain: Option<&'a str>,
    pub declared_placement: Option<&'a str>,
    pub override_reason: Option<&'a str>,
    pub has_shape: bool,
    pub required: &'a [String],
    pub supplied: &'a BTreeSet<String>,
    pub deploy_set: &'a BTreeSet<String>,
}

/// Every decidable check in one pass, collecting ALL refusals. A writer that
/// reports one problem per attempt makes the human iterate — the opposite of
/// legibility, which is the whole point of the card.
pub fn check_all(f: &WriteFacts) -> Vec<Refusal> {
    let mut out = vec![];
    for v in [
        check_domain_declaration(f.kind, f.declared_types),
        check_manifest(f.target_file, f.deploy_set),
        check_has_shape(f.class_local, f.has_shape),
        check_required(f.class_local, f.required, f.supplied),
        check_placement_layered(
            f.layer,
            f.class_local,
            f.defining_domain,
            f.declared_placement,
            f.override_reason,
        ),
    ] {
        if let Err(r) = v {
            out.push(r);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(v: &[&str]) -> BTreeSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    // Jeff's ruling — the one that must never regress.
    #[test]
    fn a_missing_required_field_is_refused_never_defaulted() {
        let req = vec!["vision".to_string(), "audience".to_string()];
        let r = check_required("Product", &req, &set(&["vision"])).unwrap_err();
        assert_eq!(r.code(), "absence-stays-absent");
        assert!(r.message().contains("audience"), "names the missing field");
        assert!(r.message().contains("REFUSES"), "states the ruling");
        assert!(check_required("Product", &req, &set(&["vision", "audience"])).is_ok());
    }

    // Silas's Q3 conflict: a blanket "must be an ABox graph" would refuse the
    // three that are RIGHT.
    #[test]
    fn punned_classes_derive_the_ontology_graph_not_a_domain_graph() {
        assert_eq!(derived_placement("Domain", Some("domains")).unwrap(), "urn:chorus:ontology");
        assert_eq!(derived_placement("Service", Some("services")).unwrap(), "urn:chorus:ontology");
        // Product is NOT punned — verified in the store, not assumed.
        assert_eq!(
            derived_placement("Product", Some("products")).unwrap(),
            "urn:chorus:domains:products"
        );
    }

    #[test]
    fn placement_derives_from_the_claiming_domain() {
        assert!(check_placement("Product", Some("products"), None, None).is_ok());
        assert!(check_placement(
            "Product",
            Some("products"),
            Some("urn:chorus:domains:products"),
            None
        )
        .is_ok());
        let r = check_placement("Product", Some("products"), Some("urn:chorus:instances"), None)
            .unwrap_err();
        assert_eq!(r.code(), "adr051x025-placement");
        assert!(check_placement(
            "Product",
            Some("products"),
            Some("urn:chorus:instances"),
            Some("staged migration, drains 2026-08")
        )
        .is_ok());
    }

    // This is how APISurface got 25 live instances from an unbound file.
    #[test]
    fn an_unclaimed_class_is_refused_not_guessed() {
        let r = check_placement("APISurface", None, None, None).unwrap_err();
        assert_eq!(r.code(), "unclaimed-class");
        assert!(r.message().contains("definesVocabulary"));
    }

    #[test]
    fn a_write_outside_the_manifest_binds_nothing_and_is_refused() {
        let ds = set(&["roles/silas/ontology/chorus.ttl"]);
        assert!(check_manifest("roles/silas/ontology/chorus.ttl", &ds).is_ok());
        let r = check_manifest("roles/silas/ontology/borg.ttl", &ds).unwrap_err();
        assert_eq!(r.code(), "manifest-membership");
        assert!(r.message().contains("binds nothing"));
    }

    #[test]
    fn a_shapeless_class_is_not_the_served_model() {
        assert!(check_has_shape("Domain", true).is_ok());
        assert_eq!(check_has_shape("Vertebra", false).unwrap_err().code(), "shape-is-truth");
    }

    #[test]
    fn a_domain_must_be_an_owl_class() {
        assert!(check_domain_declaration("domain", &["owl:Class", "chorus:Domain"]).is_ok());
        assert_eq!(
            check_domain_declaration("domain", &["chorus:Domain"]).unwrap_err().code(),
            "adr045-domain-not-class"
        );
        assert!(check_domain_declaration("product", &["chorus:Product"]).is_ok());
    }

    #[test]
    fn check_all_collects_every_refusal_not_just_the_first() {
        let required = vec!["vision".to_string()];
        let supplied = set(&[]);
        let deploy = set(&["roles/silas/ontology/chorus.ttl"]);
        let facts = WriteFacts {
            layer: Layer::Instance,
            kind: "domain",
            class_local: "Ghost",
            target_file: "roles/wren/artifacts/scratch.ttl",
            declared_types: &["chorus:Domain"],
            defining_domain: None,
            declared_placement: None,
            override_reason: None,
            has_shape: false,
            required: &required,
            supplied: &supplied,
            deploy_set: &deploy,
        };
        let codes: Vec<&str> = check_all(&facts).iter().map(|r| r.code()).collect();
        for expected in [
            "adr045-domain-not-class",
            "manifest-membership",
            "shape-is-truth",
            "absence-stays-absent",
            "unclaimed-class",
        ] {
            assert!(codes.contains(&expected), "missing {}", expected);
        }
        assert_eq!(codes.len(), 5, "all five, one pass");
    }

    // Silas's read-path check: schema MUST stay in urn:chorus:ontology. A
    // one-rule-for-both version would refuse ProductShape landing where athena-make
    // is hardcoded to read it — a refusal the read path cannot satisfy.
    #[test]
    fn schema_writes_belong_in_the_ontology_graph_and_are_never_refused() {
        // schema to the schema graph — correct, whatever the class's domain says
        assert!(check_placement_layered(Layer::Schema, "Product", Some("products"), Some(SCHEMA_GRAPH), None).is_ok());
        assert!(check_placement_layered(Layer::Schema, "Product", Some("products"), None, None).is_ok());
        // schema anywhere else — refused
        let r = check_placement_layered(Layer::Schema, "Product", Some("products"), Some("urn:chorus:domains:products"), None).unwrap_err();
        assert_eq!(r.code(), "adr051x025-placement");
        assert!(r.message().contains("no union"));
        // the SAME class's INSTANCES derive to the domain graph — two layers, two rules
        assert!(check_placement_layered(Layer::Instance, "Product", Some("products"), Some("urn:chorus:domains:products"), None).is_ok());
        let r2 = check_placement_layered(Layer::Instance, "Product", Some("products"), Some(SCHEMA_GRAPH), None).unwrap_err();
        assert_eq!(r2.code(), "adr051x025-placement");
    }

    // Silas's ruling, 2026-08-02: a punned individual is SCHEMA-layer because it
    // IS a class. Two layers, not three. #3647's principle ("write the entity's
    // home, never the legacy bucket; authz reads ownedBy from that home") is
    // intact — the derivation simply supplies ontology as that home for punned
    // individuals, and the live store already agrees (40 Domains there).
    #[test]
    fn a_punned_individual_is_schema_layer_because_it_is_a_class() {
        assert_eq!(layer_of("Domain", false), Layer::Schema, "a Domain individual IS a class");
        assert_eq!(layer_of("Service", false), Layer::Schema);
        assert_eq!(layer_of("Product", false), Layer::Instance, "Product is plain ABox");
        assert_eq!(layer_of("Product", true), Layer::Schema, "a class DEFINITION is always schema");
        // and therefore a Domain individual belongs in the ontology graph
        assert!(check_placement_layered(
            layer_of("Domain", false), "Domain", Some("domains"), Some(SCHEMA_GRAPH), None
        ).is_ok());
        // writing it to a domain graph is the refusal — this is the #3647 test's
        // wrong-as-written assertion, now named rather than argued
        let r = check_placement_layered(
            layer_of("Domain", false), "Domain", Some("domains"), Some("urn:chorus:domains:tests"), None
        ).unwrap_err();
        assert_eq!(r.code(), "adr051x025-placement");
    }

    #[test]
    fn a_conforming_write_is_accepted() {
        let required = vec!["vision".to_string(), "audience".to_string()];
        let supplied = set(&["vision", "audience"]);
        let deploy = set(&["designing/data/product-instances.ttl"]);
        let facts = WriteFacts {
            layer: Layer::Instance,
            kind: "product",
            class_local: "Product",
            target_file: "designing/data/product-instances.ttl",
            declared_types: &["chorus:Product"],
            defining_domain: Some("products"),
            declared_placement: None,
            override_reason: None,
            has_shape: true,
            required: &required,
            supplied: &supplied,
            deploy_set: &deploy,
        };
        assert!(check_all(&facts).is_empty(), "conforming write must pass");
    }
}
