//! chorus-model — the governed RDF/OWL writer (#3257, model-2 of the coherent-model program).
//!
//! The ONLY sanctioned write path to the model store (`urn:chorus:instances`),
//! sibling to `cards` (Vikunja) and `chorus-log` (logs). Implements ADR-040:
//!
//!   Rule 0 — IRIs are MINTED, never typed. Callers pass (kind, name, fields);
//!   `mint()` forms the IRI from the Level-3 table. A non-conformant IRI is
//!   unwritable by construction — there is no API that accepts one.
//!
//! Validation on write (fail-closed, refusals not warnings):
//!   - shape requirements read FROM the ontology graph (sh:minCount / sh:in /
//!     sh:datatype on the class's NodeShape) — never hardcoded;
//!   - referential integrity: every object-property edge must resolve to an
//!     existing subject in the store (SPARQL ASK), unknown target = refusal;
//!   - casing routing (ADR-040 §3): this writer only writes instances, only to
//!     the instances graph — CamelCase subjects are refused outright.
//!
//! Writes are idempotent: DELETE-WHERE on the subject + INSERT DATA in one
//! SPARQL UPDATE — same input, same triples, re-runnable.
//!
//! Zero-dep (ADR-032 §1): std only; Fuseki over `curl` subprocess; the store
//! seam is injected (`Store` trait) so the whole engine unit-tests hermetically.

use std::collections::BTreeMap;
use std::process::Command;

pub const NS: &str = "https://jeffbridwell.com/chorus#";
pub const INSTANCES_GRAPH: &str = "urn:chorus:instances";
pub const ONTOLOGY_GRAPH: &str = "urn:chorus:ontology";
pub const FUSEKI: &str = "http://localhost:3030/pods";

pub type R<T> = Result<T, String>;

/// ADR-040 Level-3: the entity kinds this writer can mint. Bare grain for the
/// governed spine entities (product, domain); type-prefixed for everything else.
/// The class name is the kind's CamelCase form (Level 4).
const KINDS: &[(&str, &str, bool)] = &[
    // (kind, class local name, bare_grain)
    ("product", "Product", true),
    ("domain", "Domain", true),
    ("role", "Role", false),
    // #3522 (Wren, Jeff-authorized 2026-06-20) — ValueStream is a generated owl-api
    // surface + SHACL shape but was missing from the DAL mint-allowlist (the
    // generate-vs-write drift). Type-prefixed like value-stream-step (mints
    // value-stream-<name>). PROVISIONAL pending Silas's ADR-040/OWL-DBA blessing.
    ("value-stream", "ValueStream", false),
    ("value-stream-step", "ValueStreamStep", false),
    ("service", "Service", false),
    ("principle", "Principle", false),
    ("practice", "Practice", false),
    ("policy", "Policy", false),
    ("skill", "Skill", false),
    ("gate", "Gate", false),
    ("decision", "Decision", false),
    ("document", "Document", false),
];

fn kind_entry(kind: &str) -> R<(&'static str, &'static str, bool)> {
    KINDS
        .iter()
        .find(|(k, _, _)| *k == kind)
        .copied()
        .ok_or_else(|| {
            format!(
                "unknown-kind: '{}' — ADR-040 kinds: {}",
                kind,
                KINDS.iter().map(|(k, _, _)| *k).collect::<Vec<_>>().join(", ")
            )
        })
}

/// Deterministic kebab normalization. Lowercases, maps runs of non-alphanumerics
/// to single dashes, trims dashes. Refuses names that normalize to nothing or
/// that LOOK like they already carry a type prefix (double-minting guard).
pub fn normalize_name(kind: &str, name: &str) -> R<String> {
    let mut out = String::new();
    let mut last_dash = true; // suppress leading dash
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() {
        return Err(format!("empty-name: '{}' normalizes to nothing", name));
    }
    let prefix = format!("{}-", kind);
    if out.starts_with(&prefix) {
        return Err(format!(
            "double-prefix: '{}' already starts with '{}' — pass the bare name; the mint adds the prefix (ADR-040 Rule 0)",
            name, prefix
        ));
    }
    Ok(out)
}

/// Rule 0 — the mint. (kind, name) → full IRI per ADR-040 Level 3.
pub fn mint(kind: &str, name: &str) -> R<String> {
    let (kind, _, bare) = kind_entry(kind)?;
    let n = normalize_name(kind, name)?;
    Ok(if bare {
        format!("{}{}", NS, n)
    } else {
        format!("{}{}-{}", NS, kind, n)
    })
}

/// The class IRI for a kind (Level 4: CamelCase, ontology-graph resident).
pub fn class_iri(kind: &str) -> R<String> {
    let (_, class, _) = kind_entry(kind)?;
    Ok(format!("{}{}", NS, class))
}

/// Shape constraints for one class, as read from the ontology graph.
#[derive(Debug, Default, Clone)]
pub struct ShapeReq {
    /// property local names with sh:minCount >= 1
    pub required: Vec<String>,
    /// property local name → allowed values (sh:in)
    pub enums: BTreeMap<String, Vec<String>>,
    /// #3467 — property local name → xsd datatype local (sh:datatype), for value-type enforcement
    pub datatypes: BTreeMap<String, String>,
    /// #3467 — edge property local name → target class local (sh:class), for edge-target-type enforcement
    pub edge_classes: BTreeMap<String, String>,
}

/// The store seam — injected so the engine unit-tests hermetically (the
/// hook-friction deps pattern). Live impl shells `curl` against Fuseki.
pub trait Store {
    /// SPARQL ASK against the union graph. true = exists.
    fn ask(&self, sparql: &str) -> R<bool>;
    /// SPARQL SELECT returning the flat list of bound values for ?v.
    fn select_v(&self, sparql: &str) -> R<Vec<String>>;
    /// SPARQL UPDATE.
    fn update(&self, sparql: &str) -> R<()>;
}

pub struct FusekiStore {
    pub endpoint: String,
}

impl FusekiStore {
    pub fn new() -> Self {
        Self { endpoint: std::env::var("CHORUS_FUSEKI").unwrap_or_else(|_| FUSEKI.to_string()) }
    }
    fn curl(&self, path: &str, data_param: &str, body: &str) -> R<String> {
        // #3641 (#3630 follow-up): the shiro flip requires HTTP Basic auth on
        // :3030 writes. Carry the credential from env — FUSEKI_ADMIN_USER/PASSWORD,
        // the one-door names the bash writers (fuseki-auth.sh) and services already
        // use. Absent/empty → no -u → anonymous, i.e. current behavior on an
        // un-flipped store, so this is safe whether or not the lock is on.
        let mut args: Vec<String> = vec![
            "-sf".into(), "--max-time".into(), "20".into(),
            "-H".into(), "Accept: application/sparql-results+json".into(),
            "--data-urlencode".into(), format!("{}={}", data_param, body),
        ];
        if let Ok(pw) = std::env::var("FUSEKI_ADMIN_PASSWORD") {
            if !pw.is_empty() {
                let user = std::env::var("FUSEKI_ADMIN_USER")
                    .unwrap_or_else(|_| "admin".to_string());
                args.push("-u".into());
                args.push(format!("{}:{}", user, pw));
            }
        }
        args.push(format!("{}{}", self.endpoint, path));
        let out = Command::new("curl")
            .args(&args)
            .output()
            .map_err(|e| format!("curl-spawn: {}", e))?;
        if !out.status.success() {
            return Err(format!(
                "fuseki-{}: HTTP failure — {}",
                data_param,
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    }
}

impl Store for FusekiStore {
    fn ask(&self, sparql: &str) -> R<bool> {
        let body = self.curl("/query", "query", sparql)?;
        Ok(body.contains("\"boolean\" : true") || body.contains("\"boolean\":true"))
    }
    fn select_v(&self, sparql: &str) -> R<Vec<String>> {
        let body = self.curl("/query", "query", sparql)?;
        // minimal SPARQL-JSON value extraction for the single ?v variable —
        // the werk-merge hand-parse pattern, zero-dep.
        let mut vals = Vec::new();
        for chunk in body.split("\"v\"").skip(1) {
            if let Some(i) = chunk.find("\"value\"") {
                let rest = &chunk[i + 7..];
                if let Some(start) = rest.find('"') {
                    let rest = &rest[start + 1..];
                    if let Some(end) = rest.find('"') {
                        vals.push(rest[..end].to_string());
                    }
                }
            }
        }
        Ok(vals)
    }
    fn update(&self, sparql: &str) -> R<()> {
        self.curl("/update", "update", sparql)?;
        Ok(())
    }
}

/// #3467 (B) — does `value` satisfy xsd:`xsd_local`? Strict on the numeric and
/// boolean lexical spaces; permissive on string/anyURI/dateTime/unknown/empty (a
/// string literal accepts anything). The DAL's datatype gate — pure + unit-pinned;
/// the per-property xsd type comes from read_shape (sh:datatype), never hardcoded.
pub fn datatype_ok(value: &str, xsd_local: &str) -> bool {
    match xsd_local {
        "integer" | "int" | "long" | "short" | "byte"
        | "nonNegativeInteger" | "positiveInteger" | "nonPositiveInteger"
        | "negativeInteger" | "unsignedInt" | "unsignedLong" | "unsignedShort" => {
            value.parse::<i64>().is_ok()
        }
        "decimal" | "double" | "float" => value.parse::<f64>().is_ok(),
        "boolean" => matches!(value, "true" | "false" | "1" | "0"),
        // string / anyURI / dateTime / date / unknown / empty → permissive.
        _ => true,
    }
}

/// Read the shape requirements for a class from the ontology graph. A class
/// with no shape yields Default (no required fields) — permissive, but the
/// caller logs it; shapes arriving later tighten writes with no code change.
pub fn read_shape(store: &dyn Store, class: &str) -> R<ShapeReq> {
    let required = store.select_v(&format!(
        "PREFIX sh: <http://www.w3.org/ns/shacl#> SELECT ?v WHERE {{ GRAPH <{g}> {{ ?s sh:targetClass <{c}> ; sh:property ?p . ?p sh:path ?path ; sh:minCount ?mc . FILTER(?mc >= 1) FILTER(isIRI(?path)) BIND(REPLACE(STR(?path), '.*#', '') AS ?v) }} }}",
        g = ONTOLOGY_GRAPH, c = class
    ))?;
    let mut enums: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let enum_rows = store.select_v(&format!(
        "PREFIX sh: <http://www.w3.org/ns/shacl#> PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> SELECT ?v WHERE {{ GRAPH <{g}> {{ ?s sh:targetClass <{c}> ; sh:property ?p . ?p sh:path ?path ; sh:in ?list . ?list rdf:rest*/rdf:first ?val . BIND(CONCAT(REPLACE(STR(?path), '.*#', ''), '|', STR(?val)) AS ?v) }} }}",
        g = ONTOLOGY_GRAPH, c = class
    ))?;
    for row in enum_rows {
        if let Some((prop, val)) = row.split_once('|') {
            enums.entry(prop.to_string()).or_default().push(val.to_string());
        }
    }
    // #3467 — per-property sh:datatype (value-type enforcement) and per-edge sh:class
    // (edge-target-type enforcement). Both read from the SAME shape; local-names only.
    let mut datatypes: BTreeMap<String, String> = BTreeMap::new();
    for row in store.select_v(&format!(
        "PREFIX sh: <http://www.w3.org/ns/shacl#> SELECT ?v WHERE {{ GRAPH <{g}> {{ ?s sh:targetClass <{c}> ; sh:property ?p . ?p sh:path ?path ; sh:datatype ?dt . FILTER(isIRI(?path)) BIND(CONCAT(REPLACE(STR(?path), '.*#', ''), '|', REPLACE(STR(?dt), '.*#', '')) AS ?v) }} }}",
        g = ONTOLOGY_GRAPH, c = class
    ))? {
        if let Some((prop, dt)) = row.split_once('|') {
            datatypes.insert(prop.to_string(), dt.to_string());
        }
    }
    let mut edge_classes: BTreeMap<String, String> = BTreeMap::new();
    for row in store.select_v(&format!(
        "PREFIX sh: <http://www.w3.org/ns/shacl#> SELECT ?v WHERE {{ GRAPH <{g}> {{ ?s sh:targetClass <{c}> ; sh:property ?p . ?p sh:path ?path ; sh:class ?cl . FILTER(isIRI(?path)) BIND(CONCAT(REPLACE(STR(?path), '.*#', ''), '|', REPLACE(STR(?cl), '.*#', '')) AS ?v) }} }}",
        g = ONTOLOGY_GRAPH, c = class
    ))? {
        if let Some((prop, cl)) = row.split_once('|') {
            edge_classes.insert(prop.to_string(), cl.to_string());
        }
    }
    Ok(ShapeReq { required, enums, datatypes, edge_classes })
}

/// Turtle string-literal escape.
fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\r', "\\r")
}

/// One write request — everything the DAL needs to form, validate, and land
/// an instance. Fields are datatype properties (string literals, v1);
/// edges are object properties whose targets are (kind, name) pairs that the
/// mint resolves — callers never pass IRIs anywhere.
#[derive(Debug, Default)]
pub struct WriteReq {
    pub kind: String,
    pub name: String,
    pub fields: BTreeMap<String, String>,
    pub edges: Vec<(String, String, String)>, // (property, target_kind, target_name)
    /// #3647 — the class's model-declared instance HOME graph (owl-api resolves it
    /// via resolve_instances_graph and passes --graph). `None` = the legacy
    /// urn:chorus:instances default (back-compat). Writing the declared home is
    /// what makes the entity readable + authorizable (no orphan): owl-api authz
    /// reads ownedBy from this same graph, so create must land here, not the bucket.
    pub graph: Option<String>,
}

/// Validation + serialization, pure (store only consulted for shapes/integrity
/// by the caller). Produces the canonical Turtle for the subject.
pub fn to_turtle(req: &WriteReq) -> R<(String, String)> {
    let subject = mint(&req.kind, &req.name)?;
    let class = class_iri(&req.kind)?;
    let mut lines = vec![format!("<{}> a <{}>", subject, class)];
    for (prop, val) in &req.fields {
        if prop.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(true) {
            return Err(format!("bad-property: '{}' — properties are camelCase (ADR-040 Level 4)", prop));
        }
        lines.push(format!("    <{}{}> \"{}\"", NS, prop, esc(val)));
    }
    for (prop, tkind, tname) in &req.edges {
        let target = mint(tkind, tname)?;
        lines.push(format!("    <{}{}> <{}>", NS, prop, target));
    }
    Ok((subject, format!("{} .\n", lines.join(" ;\n"))))
}

/// UTC ISO timestamp via the `date` subprocess (zero-dep house pattern);
/// falls back to epoch seconds if `date` is unavailable.
fn now_iso() -> String {
    Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            format!(
                "epoch:{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0)
            )
        })
}

/// Spine witness — every write and every refusal is logged via chorus-log
/// (the crawler's zero-dep pattern). Best-effort: a logging failure goes to
/// stderr but never changes the write's outcome.
fn witness(event: &str, kvs: &[(&str, &str)]) {
    let root = std::env::var("CHORUS_ROOT")
        .unwrap_or_else(|_| "/Users/jeffbridwell/CascadeProjects/chorus".to_string());
    let role = std::env::var("DEPLOY_ROLE").unwrap_or_else(|_| "silas".to_string());
    let mut args: Vec<String> = vec![event.to_string(), role];
    for (k, v) in kvs {
        args.push(format!("{}={}", k, v)); // the crawler's exact arg shape
    }
    let r = Command::new(format!("{}/platform/scripts/chorus-log", root)).args(&args).output();
    if let Err(e) = r {
        eprintln!("chorus-model: witness emit failed ({}): {}", e, event);
    }
}

/// Full governed write: shape check → referential integrity → idempotent UPDATE.
pub fn write(store: &dyn Store, req: &WriteReq) -> R<String> {
    let class = class_iri(&req.kind)?;
    let (subject, turtle) = to_turtle(req)?;
    // #3647 — write the class's model-declared home (or the legacy default). This
    // is the same graph owl-api authz reads ownedBy from; a mismatch mints an orphan.
    let g = req.graph.as_deref().unwrap_or(INSTANCES_GRAPH);

    // SHACL requirements from the ontology graph — fail-closed on missing required.
    let shape = read_shape(store, &class)?;
    for need in &shape.required {
        let satisfied = req.fields.contains_key(need)
            || req.edges.iter().any(|(p, _, _)| p == need)
            || need == "label"; // label is auto-derived below when absent
        if !satisfied {
            witness("model.refused", &[("kind", req.kind.as_str()), ("name", req.name.as_str()), ("reason", "shape-violation"), ("field", need)]);
            return Err(format!("shape-violation: {} requires '{}' (sh:minCount 1, from {})", class, need, ONTOLOGY_GRAPH));
        }
    }
    for (prop, allowed) in &shape.enums {
        if let Some(v) = req.fields.get(prop) {
            if !allowed.contains(v) {
                return Err(format!("shape-violation: '{}' not in sh:in {:?} for {}", v, allowed, prop));
            }
        }
    }
    // #3467 (B) — DATATYPE enforcement: a field value must satisfy its sh:datatype.
    // Constraint-blind no more — a wrong-type value (non-numeric for xsd:integer,
    // bad xsd:boolean) is rejected, not silently stored. Derived from the shape.
    for (prop, v) in &req.fields {
        if let Some(dt) = shape.datatypes.get(prop) {
            if !datatype_ok(v, dt) {
                witness("model.refused", &[("kind", req.kind.as_str()), ("name", req.name.as_str()), ("reason", "shape-violation"), ("field", prop)]);
                return Err(format!("shape-violation: '{}' is not a valid xsd:{} for '{}'", v, dt, prop));
            }
        }
    }

    // Referential integrity: every edge target must already exist. Fail-closed.
    for (prop, tkind, tname) in &req.edges {
        let target = mint(tkind, tname)?;
        let exists = store.ask(&format!("ASK {{ GRAPH ?g {{ <{}> ?p ?o }} }}", target))?;
        if !exists {
            witness("model.refused", &[("kind", req.kind.as_str()), ("name", req.name.as_str()), ("reason", "unknown-target"), ("edge", prop)]);
            return Err(format!(
                "unknown-target: {} → <{}> does not exist in the store — create the target first (referential integrity, fail-closed)",
                prop, target
            ));
        }
        // #3467 (B) — EDGE-TARGET-TYPE enforcement: existence is not enough; the
        // target's rdf:type must match the edge property's sh:class (e.g. partOf →
        // a Product, not any node). Beyond referential integrity. Derived from the shape.
        if let Some(want_class) = shape.edge_classes.get(prop) {
            let typed = store.ask(&format!(
                "ASK {{ GRAPH ?g {{ <{t}> a <{ns}{cl}> }} }}",
                t = target, ns = NS, cl = want_class
            ))?;
            if !typed {
                witness("model.refused", &[("kind", req.kind.as_str()), ("name", req.name.as_str()), ("reason", "shape-violation"), ("edge", prop)]);
                return Err(format!(
                    "shape-violation: {} → <{}> is not a {} (sh:class edge-target-type, fail-closed)",
                    prop, target, want_class
                ));
            }
        }
    }

    // Idempotent: replace the subject's triples wholesale in one UPDATE.
    // KNOWN v2 CONSTRAINT (named in the 2026-06-11 authored/derived convergence,
    // #3345): replace-subject semantics assume ONE writer per subject. When the
    // two-lane design lands (authored via domains API, derived via crawler), a
    // full rewrite on a shared subject would wipe the other lane's triples —
    // v2 scopes updates per-lane/per-property. Single-writer v1 is safe.
    let label_extra = if req.fields.contains_key("label") {
        String::new()
    } else {
        format!("<{}> <{}label> \"{}\" .\n", subject, NS, esc(&req.name))
    };

    // Audit envelope (Jeff's ruling, 2026-06-11: one write path = unforgeable
    // audit): dcterms created/modified/creator stamped on every write.
    // created survives rewrites — read the existing value before the replace.
    const DCT: &str = "http://purl.org/dc/terms/";
    let now = now_iso();
    // Kade's review catch: never mis-attribute the audit trail — unknown caller
    // is stamped "system", not a default role.
    let creator = std::env::var("DEPLOY_ROLE").unwrap_or_else(|_| "system".to_string());
    let existing_created = store
        .select_v(&format!(
            "SELECT ?v WHERE {{ GRAPH <{g}> {{ <{s}> <{d}created> ?v }} }}",
            g = g, s = subject, d = DCT
        ))?
        .into_iter()
        .next();
    let created = existing_created.unwrap_or_else(|| now.clone());
    let stamps = format!(
        "<{s}> <{d}created> \"{c}\" .\n<{s}> <{d}modified> \"{m}\" .\n<{s}> <{d}creator> \"{cr}\" .\n",
        s = subject, d = DCT, c = esc(&created), m = esc(&now), cr = esc(&creator)
    );

    store.update(&format!(
        "DELETE WHERE {{ GRAPH <{g}> {{ <{s}> ?p ?o }} }} ;\nINSERT DATA {{ GRAPH <{g}> {{ {t}{l}{a} }} }}",
        g = g, s = subject, t = turtle, l = label_extra, a = stamps
    ))?;
    let (nf, ne) = (req.fields.len().to_string(), req.edges.len().to_string());
    witness("model.write", &[("kind", req.kind.as_str()), ("name", req.name.as_str()), ("iri", subject.as_str()), ("fields", nf.as_str()), ("edges", ne.as_str())]);
    Ok(subject)
}

/// An edge property local-name must be camelCase (ADR-040 Level 4) — the same law
/// to_turtle enforces on fields, applied to incremental edge ops.
fn check_edge_prop(prop: &str) -> R<()> {
    let ok = !prop.is_empty()
        && prop.chars().next().map(|c| c.is_ascii_lowercase()).unwrap_or(false)
        && prop.chars().all(|c| c.is_ascii_alphanumeric());
    if !ok {
        return Err(format!("bad-property: '{}' — edge properties are camelCase (ADR-040 Level 4)", prop));
    }
    Ok(())
}

/// #3468 — DELETE an entity wholesale (governed). Fail-closed: refuses a subject
/// that does not exist (so a typo can't be a silent no-op). Witnesses the delete.
/// owl-api's DELETE delegates here instead of a raw SPARQL DELETE — one governed
/// write path, audited, never silent.
pub fn delete_entity(store: &dyn Store, kind: &str, name: &str, graph: Option<&str>) -> R<String> {
    let subject = mint(kind, name)?;
    if !store.ask(&format!("ASK {{ GRAPH ?g {{ <{}> ?p ?o }} }}", subject))? {
        witness("model.refused", &[("kind", kind), ("name", name), ("reason", "not-found")]);
        return Err(format!("not-found: <{}> does not exist", subject));
    }
    // #3647 — delete from the class's declared home (or legacy default).
    let g = graph.unwrap_or(INSTANCES_GRAPH);
    store.update(&format!(
        "DELETE WHERE {{ GRAPH <{g}> {{ <{s}> ?p ?o }} }}",
        g = g, s = subject
    ))?;
    witness("model.delete", &[("kind", kind), ("name", name), ("iri", subject.as_str())]);
    Ok(subject)
}

/// #3468 — ADD one edge INCREMENTALLY (governed). Unlike `write` (full-subject
/// replace), this touches only the single triple — so adding a partOf edge never
/// wipes the node's other data. Referential integrity on BOTH endpoints
/// (fail-closed). Witnesses the link. The governed replacement for owl-api's raw
/// build_edge_update.
pub fn add_edge(store: &dyn Store, kind: &str, name: &str, prop: &str, tkind: &str, tname: &str, graph: Option<&str>) -> R<String> {
    check_edge_prop(prop)?;
    let subject = mint(kind, name)?;
    let target = mint(tkind, tname)?;
    for iri in [&subject, &target] {
        if !store.ask(&format!("ASK {{ GRAPH ?g {{ <{}> ?p ?o }} }}", iri))? {
            witness("model.refused", &[("kind", kind), ("name", name), ("reason", "unknown-endpoint"), ("iri", iri.as_str())]);
            return Err(format!("unknown-endpoint: <{}> does not exist — referential integrity, fail-closed", iri));
        }
    }
    let g = graph.unwrap_or(INSTANCES_GRAPH); // #3647 — declared home or legacy default
    store.update(&format!(
        "INSERT DATA {{ GRAPH <{g}> {{ <{s}> <{ns}{p}> <{t}> }} }}",
        g = g, ns = NS, s = subject, p = prop, t = target
    ))?;
    witness("model.link", &[("subject", subject.as_str()), ("prop", prop), ("target", target.as_str())]);
    Ok(subject)
}

/// #3468 — REMOVE one edge (governed). Single DELETE DATA, witnessed. Idempotent:
/// removing an absent edge is a no-op success (removal toward absence is safe).
/// The governed replacement for owl-api's raw edge-delete.
pub fn remove_edge(store: &dyn Store, kind: &str, name: &str, prop: &str, tkind: &str, tname: &str, graph: Option<&str>) -> R<String> {
    check_edge_prop(prop)?;
    let subject = mint(kind, name)?;
    let target = mint(tkind, tname)?;
    let g = graph.unwrap_or(INSTANCES_GRAPH); // #3647 — declared home or legacy default
    store.update(&format!(
        "DELETE DATA {{ GRAPH <{g}> {{ <{s}> <{ns}{p}> <{t}> }} }}",
        g = g, ns = NS, s = subject, p = prop, t = target
    ))?;
    witness("model.unlink", &[("subject", subject.as_str()), ("prop", prop), ("target", target.as_str())]);
    Ok(subject)
}

// ─────────────────────────────────────────────────────────────────────────────
// ─── #3573 governed BATCH op ───────────────────────────────────────────────
// The migration target for chorus's ~10 raw batch writers (crawler-hydrate,
// enrichment, facet, tag-tests, seed-loom, migrate-aliases — they all do
// DELETE-WHERE + INSERT-DATA loops direct to Fuseki :3030 today). Wren's door
// floor (2026-07-03): TYPED SLOTS ONLY — no writer-supplied SPARQL text ever
// reaches Fuseki. The door assembles `GRAPH <g> { s p o }`; each slot is a value
// validated as a well-formed IRI or literal. Empty/off-realm graph = HARD REFUSE
// (no default graph, EVER). This is the property the whole write door exists for:
// the embedded-GRAPH-in-WHERE escape can't happen if there's no writer text.

/// An IRI term: `<...>` with no delimiter/injection chars inside.
fn is_iri_term(t: &str) -> bool {
    t.len() >= 2
        && t.starts_with('<')
        && t.ends_with('>')
        && !t[1..t.len() - 1]
            .contains(['<', '>', '"', '{', '}', '|', '^', '`', ' ', '\n', '\r', '\t', ';'])
}

/// A string literal: plain `"..."` or typed `"..."^^<datatype-iri>` (#3622 —
/// riot emits every SHACL cardinality as `"1"^^<xsd:integer>`; the door must
/// carry it). Blocks the chars that could break OUT of the door-assembled
/// `"..."`: an unescaped quote (close early), newline/CR, tab (the batch
/// delimiter), and `{ } ;` (open a block / start a new op). With those blocked,
/// arbitrary text INSIDE the quotes — including the word "GRAPH" — is inert
/// content (Wren gate 2026-07-03: don't refuse "photograph" / real prose).
/// The typed form's datatype is just another IRI check (is_iri_term — no
/// injection chars). Language tags (`"x"@en`) stay rejected until a real
/// writer emits them — widen deliberately, don't pre-open the parser surface.
fn is_literal_term(t: &str) -> bool {
    fn quoted_ok(q: &str) -> bool {
        q.len() >= 2 && q.starts_with('"') && q.ends_with('"') && {
            let inner = &q[1..q.len() - 1];
            !inner.contains(['"', '\n', '\r', '\t', '{', '}', ';'])
        }
    }
    if t.ends_with('"') {
        return quoted_ok(t); // plain literal — `^^` INSIDE the quotes is content
    }
    // typed literal: the value can't contain `"` (charset), so the LAST `"^^<`
    // is unambiguously the value/datatype seam.
    if let Some(pos) = t.rfind("\"^^<") {
        let quoted = &t[..pos + 1];
        let datatype = &t[pos + 3..];
        return quoted_ok(quoted) && is_iri_term(datatype);
    }
    false
}

/// Subject/predicate must be IRIs; a delete object may also be the single wildcard `?o`.
fn subj_pred_ok(t: &str) -> bool { is_iri_term(t) }
fn obj_ok(t: &str, allow_wildcard: bool) -> bool {
    (allow_wildcard && t == "?o") || is_iri_term(t) || is_literal_term(t)
}

/// Governed batch write — structural single-graph, typed-slot only, one transaction.
/// `deletes`: (s,p,o) patterns, o may be "?o" (delete all matching) → DELETE WHERE.
/// `inserts`: (s,p,o) ground triples → INSERT DATA. Returns count of triples touched.
pub fn batch(
    store: &dyn Store,
    graph: &str,
    deletes: &[(String, String, String)],
    inserts: &[(String, String, String)],
) -> R<usize> {
    // Wren gate 1 — a batch with no target graph is a REFUSAL, never a default-graph fallback.
    if graph.trim().is_empty() {
        witness("model.batch.refused", &[("reason", "empty-graph")]);
        return Err("batch: target graph is required (no default graph, ever)".into());
    }
    // Wren gate 2 — defense-in-depth: the DAL only ever writes urn:chorus:* (scope is
    // enforced upstream at the door; this ensures the DAL itself can't write off-realm).
    if !graph.starts_with("urn:chorus:") || graph.contains(['<', '>', '{', '}', ' ', ';']) {
        witness("model.batch.refused", &[("graph", graph), ("reason", "off-realm-graph")]);
        return Err(format!("batch: graph '{}' is outside urn:chorus:* or malformed (refused)", graph));
    }
    for (s, p, o) in deletes {
        if !subj_pred_ok(s) || !subj_pred_ok(p) || !obj_ok(o, true) {
            witness("model.batch.refused", &[("graph", graph), ("reason", "bad-delete-slot")]);
            return Err("batch: a delete triple has an invalid/injection-shaped slot".into());
        }
    }
    for (s, p, o) in inserts {
        if !subj_pred_ok(s) || !subj_pred_ok(p) || !obj_ok(o, false) {
            witness("model.batch.refused", &[("graph", graph), ("reason", "bad-insert-slot")]);
            return Err("batch: an insert triple has an invalid/injection-shaped slot".into());
        }
    }
    if deletes.is_empty() && inserts.is_empty() {
        return Err("batch: nothing to do (no deletes and no inserts)".into());
    }
    // Door-assembled SPARQL. Every clause is GRAPH <graph>-scoped by construction.
    let mut sparql = String::new();
    for (s, p, o) in deletes {
        sparql.push_str(&format!(
            "DELETE WHERE {{ GRAPH <{g}> {{ {s} {p} {o} }} }} ;\n",
            g = graph, s = s, p = p, o = o
        ));
    }
    if !inserts.is_empty() {
        let mut body = String::new();
        for (s, p, o) in inserts {
            body.push_str(&format!("{s} {p} {o} . ", s = s, p = p, o = o));
        }
        sparql.push_str(&format!("INSERT DATA {{ GRAPH <{g}> {{ {b} }} }}", g = graph, b = body));
    }
    store.update(&sparql)?;
    let (nd, ni) = (deletes.len().to_string(), inserts.len().to_string());
    witness("model.batch", &[("graph", graph), ("deletes", nd.as_str()), ("inserts", ni.as_str())]);
    Ok(deletes.len() + inserts.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Rule 0 / Level 3 — the ADR-040 mint table as regression tests ──────
    #[test]
    fn bare_grain_for_product_and_domain() {
        assert_eq!(mint("product", "loom").unwrap(), format!("{}loom", NS));
        assert_eq!(mint("domain", "principles").unwrap(), format!("{}principles", NS));
    }

    #[test]
    fn type_prefix_for_everything_else() {
        assert_eq!(mint("role", "wren").unwrap(), format!("{}role-wren", NS));
        assert_eq!(
            mint("value-stream-step", "proving").unwrap(),
            format!("{}value-stream-step-proving", NS)
        );
        assert_eq!(mint("service", "crawler").unwrap(), format!("{}service-crawler", NS));
        assert_eq!(mint("principle", "be direct").unwrap(), format!("{}principle-be-direct", NS));
    }

    #[test]
    fn the_3242_mismatch_cannot_recur() {
        // 'Proving' as a hand-typed CamelCase instance was the live wreckage.
        // Through the mint it can only come out one way.
        assert_eq!(
            mint("value-stream-step", "Proving").unwrap(),
            format!("{}value-stream-step-proving", NS)
        );
    }

    #[test]
    fn unknown_kind_refused_with_the_kind_list() {
        let e = mint("vertebra", "proving").unwrap_err();
        assert!(e.starts_with("unknown-kind"));
        assert!(e.contains("value-stream-step"), "refusal teaches the right kind");
    }

    #[test]
    fn double_prefix_refused() {
        let e = mint("role", "role-wren").unwrap_err();
        assert!(e.starts_with("double-prefix"));
    }

    #[test]
    fn normalization_is_deterministic_and_total() {
        assert_eq!(normalize_name("role", "  Wren  T. ").unwrap(), "wren-t");
        assert!(normalize_name("role", "—  —").is_err()); // normalizes to nothing
    }

    // ── Turtle formation ────────────────────────────────────────────────────
    #[test]
    fn turtle_is_valid_escaped_and_deterministic() {
        let mut req = WriteReq {
            kind: "role".into(),
            name: "test-subject".into(),
            ..Default::default()
        };
        req.fields.insert("label".into(), "He said \"hi\"\nand left".into());
        let (subj, t1) = to_turtle(&req).unwrap();
        let (_, t2) = to_turtle(&req).unwrap();
        assert_eq!(t1, t2, "same input, same triples (idempotent serialization)");
        assert_eq!(subj, format!("{}role-test-subject", NS));
        assert!(t1.contains("\\\"hi\\\""), "quotes escaped");
        assert!(t1.contains("\\n"), "newline escaped");
        assert!(!t1.contains("\"hi\"\n"), "no raw breakage");
    }

    #[test]
    fn camelcase_property_refused() {
        let mut req = WriteReq { kind: "role".into(), name: "x".into(), ..Default::default() };
        req.fields.insert("OwnedBy".into(), "y".into());
        assert!(to_turtle(&req).unwrap_err().starts_with("bad-property"));
    }

    // ── Governed write against a stub store ────────────────────────────────
    struct StubStore {
        existing: Vec<String>,
        required: Vec<String>,
        pub updates: std::cell::RefCell<Vec<String>>,
    }
    impl Store for StubStore {
        fn ask(&self, sparql: &str) -> R<bool> {
            Ok(self.existing.iter().any(|e| sparql.contains(e.as_str())))
        }
        fn select_v(&self, sparql: &str) -> R<Vec<String>> {
            if sparql.contains("sh:minCount") {
                Ok(self.required.clone())
            } else {
                Ok(vec![])
            }
        }
        fn update(&self, sparql: &str) -> R<()> {
            self.updates.borrow_mut().push(sparql.to_string());
            Ok(())
        }
    }

    fn stub(existing: &[&str], required: &[&str]) -> StubStore {
        StubStore {
            existing: existing.iter().map(|s| s.to_string()).collect(),
            required: required.iter().map(|s| s.to_string()).collect(),
            updates: Default::default(),
        }
    }

    // ── #3573 batch-op security guards (the door's reason to exist) ──
    fn t(s: &str, p: &str, o: &str) -> (String, String, String) { (s.into(), p.into(), o.into()) }

    #[test]
    fn batch_refuses_empty_graph_no_default_ever() {
        let store = stub(&[], &[]);
        let ins = vec![t("<urn:chorus:x>", "<urn:chorus:p>", "<urn:chorus:o>")];
        assert!(batch(&store, "", &[], &ins).is_err(), "empty graph must refuse");
        assert!(store.updates.borrow().is_empty(), "nothing written on empty-graph refusal");
    }

    #[test]
    fn batch_refuses_off_realm_graph() {
        let store = stub(&[], &[]);
        let ins = vec![t("<urn:chorus:x>", "<urn:chorus:p>", "<urn:chorus:o>")];
        assert!(batch(&store, "urn:gathering:photos", &[], &ins).is_err(), "off-realm graph must refuse");
        assert!(store.updates.borrow().is_empty());
    }

    #[test]
    fn batch_refuses_injection_shaped_slots_and_writes_nothing() {
        let store = stub(&[], &[]);
        // object tries to break out into another GRAPH via ; INSERT ... GRAPH <other>
        let evil_o = vec![t("<urn:chorus:x>", "<urn:chorus:p>",
            "<urn:chorus:o> } } ; INSERT DATA { GRAPH <urn:gathering:x> { <a> <b> <c> } } #")];
        assert!(batch(&store, "urn:chorus:instances", &[], &evil_o).is_err());
        // predicate is the bare GRAPH keyword (not an IRI)
        let evil_p = vec![t("<urn:chorus:y>", "GRAPH", "?o")];
        assert!(batch(&store, "urn:chorus:instances", &evil_p, &[]).is_err());
        // a raw variable object (not the single allowed ?o wildcard)
        let evil_v = vec![t("<urn:chorus:z>", "<urn:chorus:p>", "?anything")];
        assert!(batch(&store, "urn:chorus:instances", &[], &evil_v).is_err());
        assert!(store.updates.borrow().is_empty(), "no injection-shaped batch may write");
    }

    #[test]
    fn batch_accepts_valid_and_is_single_graph_scoped() {
        let store = stub(&[], &[]);
        let dels = vec![t("<urn:chorus:file/a>", "<https://jeffbridwell.com/chorus#fileInDomain>", "?o")];
        let ins = vec![t("<urn:chorus:file/a>", "<https://jeffbridwell.com/chorus#fileInDomain>", "<urn:chorus:domain/x>")];
        let n = batch(&store, "urn:chorus:instances", &dels, &ins).unwrap();
        assert_eq!(n, 2, "two triples touched");
        let ups = store.updates.borrow();
        assert_eq!(ups.len(), 1, "one transaction");
        let s = &ups[0];
        assert!(s.contains("GRAPH <urn:chorus:instances>"), "must be graph-scoped: {}", s);
        assert!(s.contains("DELETE WHERE") && s.contains("INSERT DATA"), "{}", s);
        assert!(!s.contains("urn:gathering"), "never another graph");
    }

    // ── #3622 typed literals — SHACL cardinalities must pass, injection must not ──
    #[test]
    fn batch_accepts_typed_integer_literal_the_shacl_cardinality_form() {
        let store = stub(&[], &[]);
        let ins = vec![t(
            "<urn:chorus:shape/x>",
            "<http://www.w3.org/ns/shacl#minCount>",
            "\"1\"^^<http://www.w3.org/2001/XMLSchema#integer>",
        )];
        let n = batch(&store, "urn:chorus:instances", &[], &ins).unwrap();
        assert_eq!(n, 1, "typed integer literal must pass (riot's SHACL form)");
        assert!(store.updates.borrow()[0].contains("^^<http://www.w3.org/2001/XMLSchema#integer>"));
    }

    #[test]
    fn batch_refuses_typed_literal_with_bad_datatype_or_injected_value() {
        let store = stub(&[], &[]);
        // datatype not an IRI
        let bad_dt = vec![t("<urn:chorus:s>", "<urn:chorus:p>", "\"1\"^^not-an-iri")];
        assert!(batch(&store, "urn:chorus:instances", &[], &bad_dt).is_err());
        // datatype IRI with injection chars
        let evil_dt = vec![t("<urn:chorus:s>", "<urn:chorus:p>", "\"1\"^^<urn:x> } ; INSERT")];
        assert!(batch(&store, "urn:chorus:instances", &[], &evil_dt).is_err());
        // injection inside the value of a typed literal
        let evil_val = vec![t("<urn:chorus:s>", "<urn:chorus:p>", "\"1} ; DROP\"^^<urn:x>")];
        assert!(batch(&store, "urn:chorus:instances", &[], &evil_val).is_err());
        assert!(store.updates.borrow().is_empty(), "no typed-literal injection may write");
    }

    #[test]
    fn batch_still_accepts_plain_literal_including_carets_inside() {
        let store = stub(&[], &[]);
        let ins = vec![
            t("<urn:chorus:s>", "<urn:chorus:p>", "\"plain value\""),
            t("<urn:chorus:s>", "<urn:chorus:p>", "\"a^^b inside quotes is content\""),
        ];
        assert_eq!(batch(&store, "urn:chorus:instances", &[], &ins).unwrap(), 2);
    }

    #[test]
    fn batch_language_tag_decision_rejected_until_needed() {
        // #3622 AC note: "x"@en is NOT accepted yet — no writer emits it; widen
        // deliberately when one does, don't pre-open the parser surface.
        let store = stub(&[], &[]);
        let ins = vec![t("<urn:chorus:s>", "<urn:chorus:p>", "\"x\"@en")];
        assert!(batch(&store, "urn:chorus:instances", &[], &ins).is_err());
    }

    #[test]
    fn batch_accepts_literal_containing_graph_word() {
        // Wren gate: GRAPH-as-substring inside a properly-quoted literal is inert content;
        // real values like a "/graphs/" path or "photograph" must NOT be refused.
        let store = stub(&[], &[]);
        let ins = vec![t(
            "<urn:chorus:file/p>",
            "<https://jeffbridwell.com/chorus#filePath>",
            "\"/tmp/graphs/photograph.txt\"",
        )];
        let n = batch(&store, "urn:chorus:instances", &[], &ins).unwrap();
        assert_eq!(n, 1, "a literal containing 'graph' must pass");
        assert!(store.updates.borrow()[0].contains("photograph"), "real content preserved");
    }

    #[test]
    fn write_refuses_unknown_edge_target_fail_closed() {
        let store = stub(&[], &[]);
        let req = WriteReq {
            kind: "role".into(),
            name: "test-z".into(),
            edges: vec![("ownedBy".into(), "role".into(), "nonexistent-q".into())],
            ..Default::default()
        };
        let e = write(&store, &req).unwrap_err();
        assert!(e.starts_with("unknown-target"), "{}", e);
        assert!(store.updates.borrow().is_empty(), "nothing written on refusal");
    }

    #[test]
    fn write_passes_with_existing_target_and_is_idempotent_shape() {
        let target = format!("{}value-stream-step-proving", NS);
        let store = stub(&[target.as_str()], &[]);
        let req = WriteReq {
            kind: "domain".into(),
            name: "tests".into(),
            edges: vec![("atStep".into(), "value-stream-step".into(), "proving".into())],
            ..Default::default()
        };
        let subj = write(&store, &req).unwrap();
        assert_eq!(subj, format!("{}tests", NS));
        let ups = store.updates.borrow();
        assert_eq!(ups.len(), 1);
        assert!(ups[0].contains("DELETE WHERE"), "idempotent replace-subject");
        assert!(ups[0].contains(INSTANCES_GRAPH), "casing-routed to instances graph");
    }

    #[test]
    fn write_routes_to_declared_home_when_provided() {
        // #3647 — with a model-declared home graph, the write lands THERE, not the
        // legacy urn:chorus:instances bucket. This is the orphan fix: owl-api authz
        // reads ownedBy from the declared home, so create must write the same graph.
        let target = format!("{}value-stream-step-proving", NS);
        let store = stub(&[target.as_str()], &[]);
        let home = "urn:chorus:domains:security";
        let req = WriteReq {
            kind: "domain".into(),
            name: "tests".into(),
            edges: vec![("atStep".into(), "value-stream-step".into(), "proving".into())],
            graph: Some(home.into()),
            ..Default::default()
        };
        write(&store, &req).unwrap();
        let ups = store.updates.borrow();
        assert!(ups[0].contains(home), "write must land in the declared home graph");
        assert!(!ups[0].contains(INSTANCES_GRAPH), "must NOT write the legacy instances bucket when a home is declared");
    }

    #[test]
    fn delete_and_edge_route_to_declared_home_when_provided() {
        // #3647 — delete + edge ops honor the declared home too (owner-deletes-own
        // must target the same graph the create wrote, else it fail-closed 403s).
        let subj = format!("{}gate-x", NS);
        let store = stub(&[subj.as_str()], &[]);
        delete_entity(&store, "gate", "x", Some("urn:chorus:domains:security")).unwrap();
        let ups = store.updates.borrow();
        assert!(ups[0].contains("urn:chorus:domains:security"), "delete targets the declared home");
        assert!(!ups[0].contains(INSTANCES_GRAPH), "delete must not target the legacy bucket when a home is given");
    }

    #[test]
    fn write_stamps_audit_envelope_and_preserves_created() {
        // Jeff's ruling 2026-06-11: dcterms created/modified/creator on every
        // write; created survives a rewrite (read before replace).
        let store = stub(&[], &[]);
        let req = WriteReq { kind: "role".into(), name: "audit-x".into(), ..Default::default() };
        write(&store, &req).unwrap();
        let up = store.updates.borrow()[0].clone();
        assert!(up.contains("dc/terms/created"), "created stamped");
        assert!(up.contains("dc/terms/modified"), "modified stamped");
        assert!(up.contains("dc/terms/creator"), "creator stamped");
    }

    #[test]
    fn write_enforces_shape_required_fields_from_store() {
        let store = stub(&[], &["vision"]);
        let req = WriteReq { kind: "product".into(), name: "testprod".into(), ..Default::default() };
        let e = write(&store, &req).unwrap_err();
        assert!(e.starts_with("shape-violation"), "{}", e);
        assert!(e.contains("vision"));
    }

    // ── #3468 — delete / link / unlink (the governed verbs owl-api delegates to) ──

    #[test]
    fn delete_entity_refuses_unknown_subject_fail_closed() {
        let store = stub(&[], &[]);
        let e = delete_entity(&store, "domain", "ghost", None).unwrap_err();
        assert!(e.starts_with("not-found"), "{}", e);
        assert!(store.updates.borrow().is_empty(), "nothing deleted on a missing subject");
    }

    #[test]
    fn delete_entity_deletes_existing_subject() {
        let subj = format!("{}tests", NS);
        let store = stub(&[subj.as_str()], &[]);
        let got = delete_entity(&store, "domain", "tests", None).unwrap();
        assert_eq!(got, subj);
        let ups = store.updates.borrow();
        assert_eq!(ups.len(), 1);
        assert!(ups[0].contains("DELETE WHERE"), "wholesale subject delete");
        assert!(ups[0].contains(INSTANCES_GRAPH), "routed to instances graph");
    }

    #[test]
    fn add_edge_is_referential_and_incremental() {
        // both endpoints exist → one INSERT DATA of the single triple (NOT a full
        // replace — the subject's other data is untouched).
        let subj = format!("{}tests", NS);
        let tgt = format!("{}athena", NS);
        let store = stub(&[subj.as_str(), tgt.as_str()], &[]);
        let got = add_edge(&store, "domain", "tests", "partOf", "product", "athena", None).unwrap();
        assert_eq!(got, subj);
        let ups = store.updates.borrow();
        assert_eq!(ups.len(), 1);
        assert!(ups[0].contains("INSERT DATA"), "incremental, not DELETE-WHERE");
        assert!(ups[0].contains("partOf"), "the edge predicate is written");
    }

    #[test]
    fn add_edge_refuses_missing_target_fail_closed() {
        let subj = format!("{}tests", NS);
        let store = stub(&[subj.as_str()], &[]); // target absent
        let e = add_edge(&store, "domain", "tests", "partOf", "product", "ghost", None).unwrap_err();
        assert!(e.starts_with("unknown-endpoint"), "{}", e);
        assert!(store.updates.borrow().is_empty(), "no edge written when an endpoint is missing");
    }

    #[test]
    fn add_edge_refuses_non_camelcase_property() {
        let store = stub(&[], &[]);
        let e = add_edge(&store, "domain", "tests", "Part-Of", "product", "athena", None).unwrap_err();
        assert!(e.starts_with("bad-property"), "{}", e);
    }

    #[test]
    fn remove_edge_is_idempotent_delete_data() {
        let store = stub(&[], &[]); // no existence requirement — removal toward absence
        let got = remove_edge(&store, "domain", "tests", "partOf", "product", "athena", None).unwrap();
        assert_eq!(got, format!("{}tests", NS));
        let ups = store.updates.borrow();
        assert_eq!(ups.len(), 1);
        assert!(ups[0].contains("DELETE DATA"), "single-triple removal");
        assert!(ups[0].contains("partOf"));
    }
}
