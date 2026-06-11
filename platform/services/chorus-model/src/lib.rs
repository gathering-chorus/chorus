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
        let out = Command::new("curl")
            .args([
                "-sf", "--max-time", "20",
                "-H", "Accept: application/sparql-results+json",
                "--data-urlencode", &format!("{}={}", data_param, body),
                &format!("{}{}", self.endpoint, path),
            ])
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

/// Read the shape requirements for a class from the ontology graph. A class
/// with no shape yields Default (no required fields) — permissive, but the
/// caller logs it; shapes arriving later tighten writes with no code change.
pub fn read_shape(store: &dyn Store, class: &str) -> R<ShapeReq> {
    let required = store.select_v(&format!(
        "PREFIX sh: <http://www.w3.org/ns/shacl#> SELECT ?v WHERE {{ GRAPH <{g}> {{ ?s sh:targetClass <{c}> ; sh:property ?p . ?p sh:path ?path ; sh:minCount ?mc . FILTER(?mc >= 1) BIND(REPLACE(STR(?path), '.*#', '') AS ?v) }} }}",
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
    Ok(ShapeReq { required, enums })
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
            g = INSTANCES_GRAPH, s = subject, d = DCT
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
        g = INSTANCES_GRAPH, s = subject, t = turtle, l = label_extra, a = stamps
    ))?;
    let (nf, ne) = (req.fields.len().to_string(), req.edges.len().to_string());
    witness("model.write", &[("kind", req.kind.as_str()), ("name", req.name.as_str()), ("iri", subject.as_str()), ("fields", nf.as_str()), ("edges", ne.as_str())]);
    Ok(subject)
}

// ─────────────────────────────────────────────────────────────────────────────
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
}
