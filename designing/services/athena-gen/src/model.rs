<<<<<<< HEAD
// @domain: domains
=======
>>>>>>> a0f8b1ded (wren: #4490 — athena-gen reads a product's slice of the model from TTL (in-memory Oxigraph, no Fuseki): domains, classes, fields, sh:in in order, v1 path segments; 4 tests incl. 2 negative proofs)
//! #4490 — read one product's slice of the model from the source TTL files.
//!
//! The generator reads the model the repo authors (roles/*/ontology, designing/data),
//! never a running store: the same commit always yields the same API, and generating
//! needs no Fuseki. Files load into an in-memory Oxigraph store and are asked with
//! SPARQL, the same language the generated APIs speak.

use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::Term;
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const NS: &str = "https://jeffbridwell.com/chorus#";
const PREFIXES: &str = "PREFIX c: <https://jeffbridwell.com/chorus#> \
PREFIX sh: <http://www.w3.org/ns/shacl#> \
PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> \
<<<<<<< HEAD
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#> \
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> ";
=======
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#> ";
>>>>>>> a0f8b1ded (wren: #4490 — athena-gen reads a product's slice of the model from TTL (in-memory Oxigraph, no Fuseki): domains, classes, fields, sh:in in order, v1 path segments; 4 tests incl. 2 negative proofs)

/// What a field holds: a typed literal, or a link to a row of a class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Literal { datatype: String },
    Link { class: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub kind: Kind,
    pub required: bool,
    pub many: bool,
    /// sh:in — the only values a field may hold, in model order.
    pub allowed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Class {
    pub name: String,
    /// The collection segment: `/<domain>/<segment>` (v1's #4158 rule, kept so paths match).
    pub segment: String,
    pub graph: String,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Domain {
    pub name: String,
    pub classes: Vec<Class>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Product {
    pub name: String,
    pub domains: Vec<Domain>,
}

#[derive(Debug)]
pub struct ModelError(pub String);

impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ModelError {}

fn err(e: impl std::fmt::Display) -> ModelError {
    ModelError(e.to_string())
}

/// The model's source files: every .ttl under roles/*/ontology and designing/data,
/// sorted so the load order (and so the output) never depends on the filesystem.
pub fn model_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut dirs: Vec<PathBuf> = vec![root.join("designing/data")];
    if let Ok(roles) = std::fs::read_dir(root.join("roles")) {
        for r in roles.flatten() {
            dirs.push(r.path().join("ontology"));
        }
    }
    for d in dirs {
        if let Ok(rd) = std::fs::read_dir(&d) {
            for f in rd.flatten() {
                let p = f.path();
                if p.extension().is_some_and(|e| e == "ttl") {
                    out.push(p);
                }
            }
        }
    }
    out.sort();
    out
}

/// Load the files. A file that does not parse is reported and left out; it never
/// half-loads, because each file loads in its own transaction.
pub fn load(files: &[PathBuf]) -> Result<(Store, Vec<String>), ModelError> {
    let store = Store::new().map_err(err)?;
    let mut skipped = Vec::new();
    for f in files {
        let bytes = std::fs::read(f).map_err(err)?;
        if let Err(e) = store.load_from_slice(RdfParser::from_format(RdfFormat::Turtle), &bytes) {
            skipped.push(format!("{}: {}", f.display(), e));
        }
    }
    Ok((store, skipped))
}

fn local(t: &Term) -> String {
    match t {
        Term::NamedNode(n) => n.as_str().rsplit(['#', '/']).next().unwrap_or("").to_string(),
        Term::Literal(l) => l.value().to_string(),
        other => other.to_string(),
    }
}

/// Run a SELECT and return each solution as name → local value.
fn select(store: &Store, q: &str) -> Result<Vec<BTreeMap<String, String>>, ModelError> {
    let query = format!("{PREFIXES}{q}");
    let results = SparqlEvaluator::new().parse_query(&query).map_err(err)?.on_store(store).execute().map_err(err)?;
    let mut rows = Vec::new();
    if let QueryResults::Solutions(solutions) = results {
        for s in solutions {
            let s = s.map_err(err)?;
            let mut row = BTreeMap::new();
            for (v, t) in s.iter() {
                row.insert(v.as_str().to_string(), local(t));
            }
            rows.push(row);
        }
    }
    Ok(rows)
}

/// v1's English plural (athena-make lib.rs pluralize), kept so generated paths match.
pub fn pluralize(s: &str) -> String {
    let s = s.to_lowercase();
    if let Some(stem) = s.strip_suffix('y') {
        let last = stem.chars().last();
        if last.is_some() && !matches!(last, Some('a' | 'e' | 'i' | 'o' | 'u')) {
            return format!("{stem}ies");
        }
    }
    if ["s", "x", "z", "ch", "sh"].iter().any(|suf| s.ends_with(suf)) {
        return format!("{s}es");
    }
    format!("{s}s")
}

fn singularize(w: &str) -> String {
    if let Some(stem) = w.strip_suffix("ies") {
        return format!("{stem}y");
    }
    for suf in ["ses", "xes", "zes", "ches", "shes"] {
        if w.ends_with(suf) {
            return w[..w.len() - 2].to_string();
        }
    }
    if w.ends_with("ss") {
        return w.to_string();
    }
    match w.strip_suffix('s') {
        Some(stem) if !stem.is_empty() => stem.to_string(),
        _ => w.to_string(),
    }
}

/// v1's #4158 rule: the class's plural with the domain's own prefix stripped
/// (StreamEvent in `streams` → `events`; RoleState in `awareness` → `rolestates`).
pub fn segment(domain: &str, class: &str) -> String {
    let prefix: String = domain
        .split('-')
        .enumerate()
        .map(|(i, w)| {
            let w = if i == domain.split('-').count() - 1 { singularize(w) } else { w.to_string() };
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect();
    let stem = class
        .strip_prefix(&prefix)
        .filter(|rest| !rest.is_empty() && rest.starts_with(char::is_uppercase))
        .unwrap_or(class);
    pluralize(stem)
}

/// One product: its domains (hasDomain), each domain's classes (definesVocabulary),
/// each class's fields (its SHACL shape). Everything sorted by name.
pub fn product(store: &Store, name: &str) -> Result<Product, ModelError> {
    let domains = select(store, &format!("SELECT DISTINCT ?d WHERE {{ c:{name} a c:Product ; c:hasDomain ?d }} ORDER BY ?d"))?;
    if domains.is_empty() {
        return Err(ModelError(format!("product '{name}' has no domains in the model (c:{name} a c:Product ; c:hasDomain …)")));
    }
    let mut out = Product { name: name.to_string(), domains: Vec::new() };
    for d in domains {
        let dname = d["d"].clone();
        let mut domain = Domain { name: dname.clone(), classes: Vec::new() };
        for c in select(store, &format!("SELECT DISTINCT ?c WHERE {{ c:{dname} c:definesVocabulary ?c }} ORDER BY ?c"))? {
            domain.classes.push(class(store, &dname, &c["c"])?);
        }
        out.domains.push(domain);
    }
    Ok(out)
}

fn class(store: &Store, domain: &str, name: &str) -> Result<Class, ModelError> {
    let graph = select(store, &format!(
        "SELECT ?g WHERE {{ ?s sh:targetClass c:{name} ; c:instancesGraph ?g }} ORDER BY ?g LIMIT 1"))?
        .into_iter().next().map(|r| r["g"].clone())
        .unwrap_or_else(|| format!("urn:chorus:domains:{domain}"));
    let props = select(store, &format!(
        "SELECT ?path (SAMPLE(?dt) AS ?datatype) (SAMPLE(?cl) AS ?class) (SAMPLE(?nk) AS ?nodeKind) \
         (MAX(?mn) AS ?min) (MIN(?mx) AS ?max) WHERE {{ \
<<<<<<< HEAD
           c:{name} rdfs:subClassOf* ?anc . ?s sh:targetClass ?anc ; sh:property ?p . ?p sh:path ?path . \
=======
           ?s sh:targetClass c:{name} ; sh:property ?p . ?p sh:path ?path . \
>>>>>>> a0f8b1ded (wren: #4490 — athena-gen reads a product's slice of the model from TTL (in-memory Oxigraph, no Fuseki): domains, classes, fields, sh:in in order, v1 path segments; 4 tests incl. 2 negative proofs)
           OPTIONAL {{ ?p sh:datatype ?dt }} OPTIONAL {{ ?p sh:class ?cl }} OPTIONAL {{ ?p sh:nodeKind ?nk }} \
           OPTIONAL {{ ?p sh:minCount ?mn }} OPTIONAL {{ ?p sh:maxCount ?mx }} \
         }} GROUP BY ?path ORDER BY ?path"))?;
    let mut fields = Vec::new();
    for p in props {
        let fname = p["path"].clone();
        // sh:in in model order: a member's position is how many list cells precede it
        let allowed: Vec<String> = select(store, &format!(
<<<<<<< HEAD
            "SELECT ?v (COUNT(?mid) AS ?pos) WHERE {{ c:{name} rdfs:subClassOf* ?anc . ?s sh:targetClass ?anc ; sh:property ?p . \
=======
            "SELECT ?v (COUNT(?mid) AS ?pos) WHERE {{ ?s sh:targetClass c:{name} ; sh:property ?p . \
>>>>>>> a0f8b1ded (wren: #4490 — athena-gen reads a product's slice of the model from TTL (in-memory Oxigraph, no Fuseki): domains, classes, fields, sh:in in order, v1 path segments; 4 tests incl. 2 negative proofs)
             ?p sh:path c:{fname} ; sh:in ?l . ?l rdf:rest* ?mid . ?mid rdf:rest* ?node . ?node rdf:first ?v }} \
             GROUP BY ?v ?node ORDER BY ?pos"))?
            .into_iter().map(|r| r["v"].clone()).collect();
        let is_link = p.contains_key("class") || p.get("nodeKind").is_some_and(|k| k == "IRI");
        let kind = if is_link {
            Kind::Link { class: p.get("class").cloned() }
        } else {
            Kind::Literal { datatype: p.get("datatype").cloned().unwrap_or_else(|| "string".into()) }
        };
        fields.push(Field {
            name: fname,
            kind,
            required: p.get("min").and_then(|m| m.parse::<u32>().ok()).unwrap_or(0) > 0,
<<<<<<< HEAD
            // v1's rule, kept for route parity: only a LINK with no cap is many-valued;
            // an uncapped literal is served as one value
            many: is_link && p.get("max").and_then(|m| m.parse::<u32>().ok()).is_none_or(|m| m > 1),
=======
            many: p.get("max").and_then(|m| m.parse::<u32>().ok()).is_none_or(|m| m > 1),
>>>>>>> a0f8b1ded (wren: #4490 — athena-gen reads a product's slice of the model from TTL (in-memory Oxigraph, no Fuseki): domains, classes, fields, sh:in in order, v1 path segments; 4 tests incl. 2 negative proofs)
            allowed,
        });
    }
    Ok(Class { name: name.to_string(), segment: segment(domain, name), graph, fields })
}
<<<<<<< HEAD

/// The model's version (designing/data/model-version.ttl), so every response can say
/// which model it was generated from. "unclassified" when the model declares none.
pub fn model_version(store: &Store) -> Result<String, ModelError> {
    Ok(select(store, "SELECT ?v WHERE { c:model c:vocabVersion ?v } LIMIT 1")?
        .into_iter().next().map(|r| r["v"].clone()).unwrap_or_else(|| "unclassified".into()))
}

/// The settings a product's API reads, from its Property rows (#3433/#3435):
/// `c:<product> c:hasProperty ?p . ?p c:propertyKey ?k ; c:propertyValue ?v`.
/// Sorted by key. The runtime reads exactly these; nothing else is a setting.
pub fn settings(store: &Store, product: &str) -> Result<Vec<(String, String)>, ModelError> {
    Ok(select(store, &format!(
        "SELECT ?k ?v WHERE {{ c:{product} c:hasProperty ?p . ?p c:propertyKey ?k ; c:propertyValue ?v }} ORDER BY ?k"))?
        .into_iter().map(|r| (r["k"].clone(), r.get("v").cloned().unwrap_or_default())).collect())
}

/// The keys the generated runtime reads. A product whose model declares fewer cannot
/// be generated: the refusal names the missing key instead of a default being invented.
pub const RUNTIME_SETTINGS: &[&str] =
    &["events.command", "identity.issuer", "server.bind", "store.endpoint", "store.kind", "store.password_var", "store.user_var"];

pub fn missing_settings(declared: &[(String, String)]) -> Vec<&'static str> {
    RUNTIME_SETTINGS.iter().copied().filter(|k| !declared.iter().any(|(d, _)| d == k)).collect()
}
=======
>>>>>>> a0f8b1ded (wren: #4490 — athena-gen reads a product's slice of the model from TTL (in-memory Oxigraph, no Fuseki): domains, classes, fields, sh:in in order, v1 path segments; 4 tests incl. 2 negative proofs)
