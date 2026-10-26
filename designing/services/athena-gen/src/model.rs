// @domain: domains
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

/// The modelling language's namespace: the vocabulary the model is AUTHORED in
/// (c:Product, c:hasDomain, sh shapes). Build time only. A product's own namespace,
/// the one its API serves, is its `model.namespace` setting (#4519).
pub const NS: &str = "https://jeffbridwell.com/chorus#";
const PREFIXES: &str = "PREFIX c: <https://jeffbridwell.com/chorus#> \
PREFIX sh: <http://www.w3.org/ns/shacl#> \
PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> \
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#> \
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> ";

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
    /// #4744 — `c:inHeader true` on the shape's property: the field is on its product's header.
    pub header: bool,
    /// #4744 — `c:headerNewest true`: the header shows this class's rows newest first by this field.
    pub newest: bool,
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
    /// #4742 — homed in another product and composed here: served read-only, from the owner's graph.
    pub composed: bool,
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
    // #4519 — a product repo (the wine diary) keeps its model in designing/ontology
    let mut dirs: Vec<PathBuf> = vec![root.join("designing/data"), root.join("designing/ontology")];
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

/// #4519 — like `select`, but IRIs stay whole: a product's classes and fields live in
/// the product's own namespace, so a local name alone cannot find them again.
pub(crate) fn select_iri(store: &Store, q: &str) -> Result<Vec<BTreeMap<String, String>>, ModelError> {
    let query = format!("{PREFIXES}{q}");
    let results = SparqlEvaluator::new().parse_query(&query).map_err(err)?.on_store(store).execute().map_err(err)?;
    let mut rows = Vec::new();
    if let QueryResults::Solutions(solutions) = results {
        for s in solutions {
            let s = s.map_err(err)?;
            rows.push(s.iter().map(|(v, t)| (v.as_str().to_string(), match t {
                Term::NamedNode(n) => n.as_str().to_string(),
                other => local(other),
            })).collect());
        }
    }
    Ok(rows)
}

fn local_name(iri: &str) -> String {
    iri.rsplit(['#', '/']).next().unwrap_or(iri).to_string()
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
    let owned = select(store, &format!("SELECT DISTINCT ?d WHERE {{ c:{name} a c:Product ; c:hasDomain ?d }} ORDER BY ?d"))?;
    if owned.is_empty() {
        return Err(ModelError(format!("product '{name}' has no domains in the model (c:{name} a c:Product ; c:hasDomain …)")));
    }
    let mut out = Product { name: name.to_string(), domains: Vec::new() };
    // #4519 — a class with no instancesGraph lives under the product's own graph
    // prefix, never a Chorus default
    let declared = settings(store, name)?;
    let prefix = declared.iter().find(|(k, _)| k == "store.graph_prefix").map(|(_, v)| v.clone());
    // #4519 reopen — the product's classes are named in its own namespace (wine-diary:Wine).
    // A product that declares none is refused, never read as a Chorus product.
    let Some(ns) = declared.iter().find(|(k, _)| k == "model.namespace").map(|(_, v)| v.clone()) else {
        return Err(ModelError(format!("{name} declares no model.namespace: add a Property row (c:{name} c:hasProperty …)")));
    };
    // #4742 — the product's child routes are the domains it owns, then the ones it composes
    // (a composition, not an orchestration); a domain it also owns is served once, as owned.
    let composed = select(store, &format!(
        "SELECT DISTINCT ?d WHERE {{ c:{name} c:composes ?d FILTER NOT EXISTS {{ c:{name} c:hasDomain ?d }} }} ORDER BY ?d"))?;
    let domains = owned.into_iter().map(|d| (d, false)).chain(composed.into_iter().map(|d| (d, true)));
    for (d, composed) in domains {
        let dname = d["d"].clone();
        let mut domain = Domain { name: dname.clone(), composed, classes: Vec::new() };
        for c in select_iri(store, &format!("SELECT DISTINCT ?c WHERE {{ c:{dname} c:definesVocabulary ?c }} ORDER BY ?c"))? {
            let iri = &c["c"];
            if !iri.starts_with(&ns) {
                return Err(ModelError(format!(
                    "{dname} defines {iri}, which is not in {name}'s namespace {ns}: a product serves only its own classes")));
            }
            domain.classes.push(class(store, &dname, iri, prefix.as_deref(), name)?);
        }
        out.domains.push(domain);
    }
    Ok(out)
}

fn class(store: &Store, domain: &str, iri: &str, prefix: Option<&str>, product: &str) -> Result<Class, ModelError> {
    let name = local_name(iri);
    let graph = match select(store, &format!(
        "SELECT ?g WHERE {{ ?s sh:targetClass <{iri}> ; c:instancesGraph ?g }} ORDER BY ?g LIMIT 1"))?
        .into_iter().next() {
        Some(r) => r["g"].clone(),
        None => match prefix {
            Some(p) => format!("{p}{domain}"),
            None => return Err(ModelError(format!(
                "{name} names no instancesGraph and {product} declares no store.graph_prefix: add one or the other"))),
        },
    };
    let props = select_iri(store, &format!(
        "SELECT ?path (SAMPLE(?dt) AS ?datatype) (SAMPLE(?cl) AS ?class) (SAMPLE(?nk) AS ?nodeKind) \
         (MAX(?mn) AS ?min) (MIN(?mx) AS ?max) (MAX(?ih) AS ?inHeader) (MAX(?hn) AS ?headerNewest) WHERE {{ \
           <{iri}> rdfs:subClassOf* ?anc . ?s sh:targetClass ?anc ; sh:property ?p . ?p sh:path ?path . \
           OPTIONAL {{ ?p sh:datatype ?dt }} OPTIONAL {{ ?p sh:class ?cl }} OPTIONAL {{ ?p sh:nodeKind ?nk }} \
           OPTIONAL {{ ?p sh:minCount ?mn }} OPTIONAL {{ ?p sh:maxCount ?mx }} OPTIONAL {{ ?p c:inHeader ?ih }} OPTIONAL {{ ?p c:headerNewest ?hn }} \
         }} GROUP BY ?path ORDER BY ?path"))?;
    let mut fields = Vec::new();
    for p in props {
        let path = p["path"].clone();
        let fname = local_name(&path);
        // sh:in in model order: a member's position is how many list cells precede it
        // #4514 — a path that is not an IRI (a blank-node sh:inversePath) has no sh:in to read
        let allowed: Vec<String> = if !path.starts_with("http") { Vec::new() } else { select(store, &format!(
            "SELECT ?v (COUNT(?mid) AS ?pos) WHERE {{ <{iri}> rdfs:subClassOf* ?anc . ?s sh:targetClass ?anc ; sh:property ?p . \
             ?p sh:path <{path}> ; sh:in ?l . ?l rdf:rest* ?mid . ?mid rdf:rest* ?node . ?node rdf:first ?v }} \
             GROUP BY ?v ?node ORDER BY ?pos"))?
            .into_iter().map(|r| r["v"].clone()).collect() };
        let is_link = p.contains_key("class") || p.get("nodeKind").is_some_and(|k| k.ends_with("#IRI"));
        let kind = if is_link {
            Kind::Link { class: p.get("class").map(|c| local_name(c)) }
        } else {
            Kind::Literal { datatype: p.get("datatype").map_or_else(|| "string".into(), |d| local_name(d)) }
        };
        fields.push(Field {
            name: fname,
            kind,
            required: p.get("min").and_then(|m| m.parse::<u32>().ok()).unwrap_or(0) > 0,
            // v1's rule, kept for route parity: only a LINK with no cap is many-valued;
            // an uncapped literal is served as one value
            many: is_link && p.get("max").and_then(|m| m.parse::<u32>().ok()).is_none_or(|m| m > 1),
            allowed,
            header: p.get("inHeader").is_some_and(|h| h == "true"),
            newest: p.get("headerNewest").is_some_and(|h| h == "true"),
        });
    }
    Ok(Class { segment: segment(domain, &name), name, graph, fields })
}

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
/// #4519 — `model.namespace`, `model.prefix` and `store.graph_prefix` make the API the
/// product's own: its terms, the short prefix callers may write, and where its rows live.
pub const RUNTIME_SETTINGS: &[&str] = &[
    "events.command", "identity.graph_prefix", "identity.issuer", "model.namespace", "model.prefix", "server.bind",
    "store.endpoint", "store.graph_prefix", "store.kind", "store.password_var", "store.path", "store.user_var",
];

pub fn missing_settings(declared: &[(String, String)]) -> Vec<&'static str> {
    RUNTIME_SETTINGS.iter().copied().filter(|k| !declared.iter().any(|(d, _)| d == k)).collect()
}

/// #4576 — a product the generator builds runs its own server, so it declares
/// `server.bind`. `model.namespace` alone is not enough: Athena declares it (#4519)
/// and is served by athena-make v1, never generated.
pub fn runs_own_server(declared: &[(String, String)]) -> bool {
    declared.iter().any(|(k, _)| k == "server.bind")
}

/// #4513 — one argument of a tool, from a ToolInput shape's property.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolArg {
    /// The JSON name the tool takes it by (sh:name).
    pub name: String,
    /// #4514 — no sh:maxCount: a list of values, in order (an athena verb's args).
    pub many: bool,
    pub datatype: String,
    pub required: bool,
    pub allowed: Vec<String>,
    pub description: String,
    /// #4515 — `chorus:namesRole "agent"|"peer"`: the value must be a role the
    /// roles door lists (the runner checks; the old server did it per tool).
    pub names_role: Option<String>,
    /// #4515 — sh:pattern: the regex every value must match (the runner checks
    /// it; before this athena-gen dropped it, so a shape's pattern was never enforced).
    pub pattern: Option<String>,
}

/// #4513 — a tool's arguments: every `c:ToolInput` shape in the model (a SHACL
/// node shape with no target class, so it never becomes a door collection),
/// its properties in sh:order. Sorted by shape name so the output is stable.
pub fn tool_inputs(store: &Store) -> Result<Vec<(String, Vec<ToolArg>)>, ModelError> {
    let shapes = select(store, "SELECT ?s WHERE { ?s a c:ToolInput , sh:NodeShape } ORDER BY ?s")?;
    let mut out = Vec::new();
    for s in shapes {
        let shape = s["s"].clone();
        let props = select(store, &format!(
            "SELECT ?path (SAMPLE(?n) AS ?name) (SAMPLE(?dt) AS ?datatype) (SAMPLE(?mn) AS ?min) (SAMPLE(?mx) AS ?max) \
                    (SAMPLE(?d) AS ?desc) (SAMPLE(?o) AS ?order) (SAMPLE(?r) AS ?role) (SAMPLE(?pat) AS ?pattern) WHERE {{ \
               c:{shape} sh:property ?p . ?p sh:path ?path . \
               OPTIONAL {{ ?p sh:name ?n }} OPTIONAL {{ ?p sh:datatype ?dt }} OPTIONAL {{ ?p sh:minCount ?mn }} OPTIONAL {{ ?p sh:maxCount ?mx }} \
               OPTIONAL {{ ?p sh:description ?d }} OPTIONAL {{ ?p sh:order ?o }} OPTIONAL {{ ?p c:namesRole ?r }} OPTIONAL {{ ?p sh:pattern ?pat }} \
             }} GROUP BY ?path"))?;
        let mut args: Vec<(i64, ToolArg)> = Vec::new();
        for p in props {
            let path = p["path"].clone();
            let allowed: Vec<String> = select(store, &format!(
                "SELECT ?v (COUNT(?mid) AS ?pos) WHERE {{ c:{shape} sh:property ?p . ?p sh:path c:{path} ; sh:in ?l . \
                 ?l rdf:rest* ?mid . ?mid rdf:rest* ?node . ?node rdf:first ?v }} GROUP BY ?v ?node ORDER BY ?pos"))?
                .into_iter().map(|r| r["v"].clone()).collect();
            let order = p.get("order").and_then(|o| o.parse::<f64>().ok()).unwrap_or(f64::MAX) as i64;
            args.push((order, ToolArg {
                name: p.get("name").cloned().unwrap_or(path),
                many: !p.contains_key("max"),
                datatype: p.get("datatype").cloned().unwrap_or_else(|| "string".into()),
                required: p.get("min").and_then(|m| m.parse::<u32>().ok()).unwrap_or(0) > 0,
                allowed,
                description: p.get("desc").cloned().unwrap_or_default(),
                names_role: p.get("role").cloned(),
                pattern: p.get("pattern").cloned(),
            }));
        }
        args.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.name.cmp(&b.1.name)));
        out.push((shape, args.into_iter().map(|(_, a)| a).collect()));
    }
    Ok(out)
}

/// #4444 reopen 2 — every AgentRuntime row, read generically: each literal
/// property by its local name, one value where the shape says sh:maxCount 1 and
/// a sorted list otherwise. A row missing a property the shape requires is
/// refused by name (nothing here knows which properties those are).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuntimeTable {
    /// runtime name (the row's local name without "runtime-") → property → values
    pub runtimes: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    /// properties the shape allows at most once
    pub single: Vec<String>,
    /// principal name (without "principal-") → runtime name
    pub runs_on: BTreeMap<String, String>,
}

pub fn agent_runtimes(store: &Store) -> Result<RuntimeTable, ModelError> {
    let strip = |s: &str, p: &str| s.strip_prefix(p).unwrap_or(s).to_string();
    let mut t = RuntimeTable::default();
    let required: Vec<String> = select(store, "SELECT ?path WHERE { c:AgentRuntimeShape sh:property ?ps . ?ps sh:path ?path ; sh:minCount ?m FILTER(?m >= 1) }")?
        .into_iter().map(|r| r["path"].clone()).collect();
    t.single = select(store, "SELECT ?path WHERE { c:AgentRuntimeShape sh:property ?ps . ?ps sh:path ?path ; sh:maxCount 1 }")?
        .into_iter().map(|r| r["path"].clone()).collect();
    t.single.sort();
    // #4576 — a runtime's table is its configuration: the core attributes every
    // row carries (CoreShape's name and description, the DAL's dcterms stamps)
    // are about the row, not the program, and stay out of agent_runtimes.json.
    for r in select(store, "SELECT ?r ?p ?v WHERE { ?r a c:AgentRuntime ; ?p ?v FILTER(isLiteral(?v)) FILTER(!STRSTARTS(STR(?p), \"http://purl.org/dc/terms/\")) FILTER NOT EXISTS { c:CoreShape sh:property/sh:path ?p } }")? {
        let row = t.runtimes.entry(strip(&r["r"], "runtime-")).or_default();
        let key = if r["p"] == "label" { "label".to_string() } else { r["p"].clone() };
        row.entry(key).or_default().push(r["v"].clone());
    }
    for row in t.runtimes.values_mut() {
        for v in row.values_mut() { v.sort(); }
    }
    for (name, row) in &t.runtimes {
        let missing: Vec<&String> = required.iter().filter(|p| !row.contains_key(*p)).collect();
        if !missing.is_empty() {
            return Err(err(format!("AgentRuntime {name} is missing {}", missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "))));
        }
    }
    for r in select(store, "SELECT ?p ?r WHERE { ?p c:agentRuntime ?r }")? {
        let runtime = strip(&r["r"], "runtime-");
        if !t.runtimes.contains_key(&runtime) {
            return Err(err(format!("{} runs on {runtime}, which is not an AgentRuntime row", r["p"])));
        }
        t.runs_on.insert(strip(&r["p"], "principal-"), runtime);
    }
    Ok(t)
}
