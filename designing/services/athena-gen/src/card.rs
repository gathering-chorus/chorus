// @domain: domains
//! #4515 — `athena-gen generate --card <n>`: the Make step for one card's change.
//!
//! The card's werk and model change come from the shared rule (platform/services/
//! shared/card_scope.rs). What the card's changed model files touch decides which
//! products are generated: every class they declare (owl:Class / rdfs:Class) or
//! shape (sh:targetClass), and every product whose own rows they write (its
//! settings, its domains). A touched class that no product serves is named, never
//! skipped: nothing would generate it.
//!
//! #4547 reopen 1 — a file is not the change. Only the classes and rows whose
//! triples differ from main's are the card's (the shared rule, card_scope::
//! changed_subjects): touching chorus.ttl must not make a card answer for the
//! 26 old classes already in it.

use crate::model::{select_iri, ModelError};
use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::store::Store;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[allow(dead_code)] // each crate uses part of the shared rule
pub mod card_scope {
    include!("../../../../platform/services/shared/card_scope.rs");
}

fn err(e: impl std::fmt::Display) -> ModelError {
    ModelError(e.to_string())
}

/// What a card's model files touch: class IRIs (declared or shaped) and the
/// subjects they write (for products whose own rows changed).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Touched {
    pub classes: BTreeSet<String>,
    pub subjects: BTreeSet<String>,
    /// (shape, class) for every `?shape sh:targetClass ?class` in the files.
    pub targets: BTreeSet<(String, String)>,
    /// #4691 — subjects typed as an instance (`?s a ?c`, ?c not a class, shape or
    /// property type): rows the seed step deploys, which generate nothing.
    pub instances: BTreeSet<String>,
}

/// Read the card's files on their own. A file that does not parse is an error
/// naming the file: the Model step should have stopped it.
pub fn touched(files: &[PathBuf]) -> Result<Touched, ModelError> {
    let store = Store::new().map_err(err)?;
    for f in files {
        let bytes = std::fs::read(f).map_err(|e| err(format!("{}: {e}", f.display())))?;
        store
            .load_from_slice(RdfParser::from_format(RdfFormat::Turtle), &bytes)
            .map_err(|e| err(format!("{} does not parse: {e}", f.display())))?;
    }
    let mut t = Touched::default();
    for r in select_iri(&store, "SELECT DISTINCT ?c WHERE { { ?c a <http://www.w3.org/2002/07/owl#Class> } UNION \
        { ?c a rdfs:Class } UNION { ?s sh:targetClass ?c } FILTER(isIRI(?c)) }")? {
        t.classes.insert(r["c"].clone());
    }
    for r in select_iri(&store, "SELECT DISTINCT ?s WHERE { ?s ?p ?o FILTER(isIRI(?s)) }")? {
        t.subjects.insert(r["s"].clone());
    }
    for r in select_iri(&store, "SELECT DISTINCT ?s ?c WHERE { ?s sh:targetClass ?c FILTER(isIRI(?s) && isIRI(?c)) }")? {
        t.targets.insert((r["s"].clone(), r["c"].clone()));
    }
    for r in select_iri(&store, "SELECT DISTINCT ?s WHERE { ?s a ?c FILTER(isIRI(?s) && ?c NOT IN (\
        <http://www.w3.org/2002/07/owl#Class>, rdfs:Class, sh:NodeShape, sh:PropertyShape, \
        <http://www.w3.org/2002/07/owl#ObjectProperty>, <http://www.w3.org/2002/07/owl#DatatypeProperty>, \
        <http://www.w3.org/1999/02/22-rdf-syntax-ns#Property>)) }")? {
        t.instances.insert(r["s"].clone());
    }
    Ok(t)
}

/// #4547 reopen 1 — keep only what the card changed: a class whose own triples
/// changed or whose shape changed, and the rows whose triples changed. `changed`
/// holds bare IRIs (card_scope::changed_subjects without the angle brackets).
pub fn only_changed(t: Touched, changed: &BTreeSet<String>) -> Touched {
    let shape_changed = |c: &String| t.targets.iter().any(|(s, tc)| tc == c && changed.contains(s));
    Touched {
        classes: t.classes.iter().filter(|c| changed.contains(*c) || shape_changed(c)).cloned().collect(),
        subjects: t.subjects.iter().filter(|s| changed.contains(*s)).cloned().collect(),
        targets: t.targets.iter().filter(|(s, _)| changed.contains(s)).cloned().collect(),
        instances: t.instances.iter().filter(|s| changed.contains(*s)).cloned().collect(),
    }
}

/// #4590 — a changed shared shape (a node shape with no target class, named by
/// other shapes through sh:node) reaches every class whose shape names it. Those
/// classes were not changed by the card, so a reached class no product serves is
/// not the card's to answer for; the caller maps the rest to their products.
pub fn via_shared_shapes(store: &Store, changed: &BTreeSet<String>) -> Result<BTreeSet<String>, ModelError> {
    let mut out = BTreeSet::new();
    if changed.is_empty() {
        return Ok(out);
    }
    let values: Vec<String> = changed.iter().map(|s| format!("<{s}>")).collect();
    let q = format!(
        "SELECT DISTINCT ?c WHERE {{ VALUES ?shared {{ {} }} ?shared a sh:NodeShape . \
         FILTER NOT EXISTS {{ ?shared sh:targetClass ?any }} \
         ?s sh:targetClass ?c ; (sh:node|sh:property)+ ?shared . FILTER(isIRI(?c)) }}",
        values.join(" ")
    );
    for r in select_iri(store, &q)? {
        out.insert(r["c"].clone());
    }
    Ok(out)
}

/// The bare IRIs a card's files changed against main, by the shared rule. A file
/// riot cannot read is an error naming it.
pub fn changed_since_main(werk: &str, rels: &[String]) -> Result<BTreeSet<String>, ModelError> {
    let mut out = BTreeSet::new();
    for rel in rels {
        let abs = format!("{}/{rel}", werk.trim_end_matches('/'));
        let werk_nt = card_scope::ntriples(&[abs]).map_err(|e| err(format!("{rel}: {e}")))?;
        let main_nt = card_scope::main_ntriples(werk, rel);
        for s in card_scope::changed_subjects(&werk_nt, &main_nt) {
            out.insert(s.trim_matches(|c| c == '<' || c == '>').to_string());
        }
    }
    Ok(out)
}

/// product name → the classes it serves that the card touched; then the unserved classes.
pub type ProductsFor = (BTreeMap<String, Vec<String>>, Vec<String>);

/// Which products to generate, and why: product name → the touched classes it
/// serves (empty when only the product's own rows changed). Classes no product
/// serves come back as the second value.
pub fn products_for(store: &Store, t: &Touched) -> Result<ProductsFor, ModelError> {
    let rows = select_iri(store, "SELECT DISTINCT ?p ?c WHERE { ?p a c:Product ; c:hasDomain ?d . \
        OPTIONAL { ?d c:definesVocabulary ?c } }")?;
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut served: BTreeSet<String> = BTreeSet::new();
    for r in &rows {
        let p = &r["p"];
        let name = card_scope::local_name(p);
        if let Some(c) = r.get("c") {
            if t.classes.contains(c) {
                served.insert(c.clone());
                let e = out.entry(name.clone()).or_default();
                let cn = card_scope::local_name(c);
                if !e.contains(&cn) {
                    e.push(cn);
                }
            }
        }
        if t.subjects.contains(p) {
            out.entry(name).or_default();
        }
    }
    let unserved = t.classes.iter().filter(|c| !served.contains(*c)).map(|c| card_scope::local_name(c)).collect();
    Ok((out, unserved))
}

/// Where a card's product crate is written: `<out>/<product>/api`, the place the
/// committed crates live today (directing/products/<product>/api).
pub fn product_out(out_base: &Path, product: &str) -> PathBuf {
    out_base.join(product).join("api")
}
