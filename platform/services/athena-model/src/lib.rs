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

// #3690 — clippy-ratchet: chorus-model baseline is 0, and touching the crate now
// runs the workspace ratchet. Two lint CLASSES are accepted here (not defects):
//  - doc_lazy_continuation: our house doc style wraps prose across `///` lines;
//    clippy misreads a wrapped line beginning with a word as an unindented list
//    item. A false positive on prose — reflowing to satisfy it is pure churn.
//  - too_many_arguments: the governed write verbs (add_edge/remove_edge/seed)
//    carry (store, kind, name, prop, tkind, tname, graph, id) — cohesive typed
//    slots on the ONE write path; splitting scatters the door (the #3429
//    rationale). 7→8 is intentional, not a smell.
#![allow(clippy::doc_lazy_continuation)]
#![allow(clippy::too_many_arguments)]

pub mod adr; // #3718 — the ADR refusal core (pure; no store, no fs)
pub mod tbox;
pub mod vocab_version; // #3902 — semver at the pen (ledger + TTL projection) // #3718 — the TBox half: class/property/shape, refusal-first, no defaults

use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

use serde::Deserialize;

pub const NS: &str = "https://jeffbridwell.com/chorus#";
pub const INSTANCES_GRAPH: &str = "urn:chorus:instances";
pub const ONTOLOGY_GRAPH: &str = "urn:chorus:ontology";
pub const SECURITY_GRAPH: &str = "urn:chorus:domains:security";
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
    // #3522 (Wren, Jeff-authorized 2026-06-20) — ValueStream is a generated athena-make
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
    // #4089 — Commitment rows (a service design's promises, #4064) deploy through
    // the INSTANCE_SET like every cross-domain instance (Silas's OWL-DBA ruling
    // 2026-09-03: not a domain's harvested graph). Type-prefixed: commitment-<name>.
    ("commitment", "Commitment", false),
    // #3680 — Test as a REFERENCE kind: TestResult.ofTest must mint the target
    // IRI, and the 4,617 Test entities are crawler-minted BARE (NS#<name>, names
    // already test-*-slugged). bare_grain=true reproduces exactly that IRI. The
    // DAL can now also create tests — governed, harmless; the point is edges.
    ("test", "Test", true),
    // #3592 (Kade, Jeff-driven card 2026-07-23) — Test's run-evidence kinds. Same
    // generate-vs-write drift #3522 named: TestResult/TestSuiteRun are modeled
    // classes with SHACL shapes and athena-make already generates their write routes,
    // but the DAL allowlist never grew, so every werk-test wire-back 502'd
    // unknown-kind. Type-prefixed like the rest. PROVISIONAL pending Silas's
    // ADR-040/OWL-DBA blessing (nudged 2026-07-23).
    ("test-result", "TestResult", false),
    ("test-suite-run", "TestSuiteRun", false),
    // #3654 (Wren, Jeff-driven 2026-07-24) — the board domain's kinds. Chunk +
    // ChunkMembership are board-native; Card is a THIN FK stub (Vikunja SoR until
    // #2159 — the board mints id+label only so a membership resolves). All three
    // type-prefixed (bare_grain=false): they are NEW authored kinds, nothing to
    // reproduce (bare is only for crawler-minted IRIs, #3680). Mints:
    // card-<vikunja-id>, chunk-<slug>, chunkmembership-<...>. PROVISIONAL pending
    // Silas's ADR-040/OWL-DBA blessing (paired #3654, ADR-054 seam).
    ("card", "Card", false),
    ("chunk", "Chunk", false),
    ("chunkmembership", "ChunkMembership", false),
    // #4040 (Kade, Jeff-GO'd 2026-08-31) — the pipelines domain's kinds. Same
    // generate-vs-write drift as #3522/#3592/#3654: Pipeline + PipelineStep are
    // modeled with SHACL shapes and claimed on the pipelines domain, but the
    // land's seed lane refused unknown-kind. Type-prefixed (mints
    // pipeline-<name>; pipeline-step is its own kind so a subject is never
    // claimed by two kinds in one batch — the 19:23 refusal). PROVISIONAL
    // pending Silas's ADR-040/OWL-DBA blessing (nudged 2026-08-31).
    ("pipeline", "Pipeline", false),
    ("pipeline-step", "PipelineStep", false),
    // #4047 — the run rows themselves. #4040 admitted the two authored kinds and
    // stopped there, so the nightly's emit could never mint: unknown-kind 502 on
    // every attempt while the collection served an empty list. PROVISIONAL, same
    // ADR-040 lineage Silas blessed 2026-08-31.
    ("pipeline-run", "PipelineRun", false),
    // #3773 (Silas, Wren-blessed 2026-08-06) — the generate-vs-write drift again,
    // and this time it is the WHOLE SECURITY DOMAIN. athena-make serves 24 classes;
    // this table admitted 19. The 11 missing: the seven security classes below,
    // plus EmitContract, Metric, Property, PropertyKey.
    //
    // The consequence, measured: GET /principals returns 200 with count 0 while
    // ten Principal instances sit a graph away — and nobody could write them to
    // fix it. The security fitness function (#3765) reads 1 of 7 conforming. Of
    // course it does: six of its seven rows name classes the governed writer
    // refused. A domain cannot conform to a model it is not permitted to populate.
    //
    // THIS TABLE IS THE LAST HAND-MADE LINK in a chain that is otherwise
    // generated, and this is the fourth card it has blocked — #3522
    // (ValueStream), #3592 (TestResult/TestSuiteRun), #3654 (board kinds), now
    // this one. Every fix was a single line; every one cost a day of confusion
    // first, because a class can be declared, claimed, and SERVED and still be
    // unwritable, and nothing announces that until someone tries to write.
    //
    // TOMBSTONE, written in advance: chorus-athena's generate step derives this
    // list from the declared classes and this array is deleted. Jeff, 2026-08-06:
    // "it really can be mostly config and api calls like a workflow like
    // chorus-werk just chorus-athena." When that lands, delete from the #3773
    // comment through the end of this block. The rows are not the asset; the
    // generation is.
    //
    // bare_grain=false throughout: authored kinds, type-prefixed like the rest,
    // nothing crawler-minted to reproduce (#3680).
    ("principal", "Principal", false),
    ("credential", "Credential", false),
    ("permission", "Permission", false),
    ("api-surface", "APISurface", false),
    ("auth-boundary", "AuthBoundary", false),
    ("key-registry-entry", "KeyRegistryEntry", false),
    ("security-probe", "SecurityProbe", false),
    ("emit-contract", "EmitContract", false),
    ("metric", "Metric", false),
    ("property", "Property", false),
    ("property-key", "PropertyKey", false),
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
fn normalize_slug(name: &str) -> R<String> {
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
    Ok(out)
}

pub fn normalize_name(kind: &str, name: &str) -> R<String> {
    let out = normalize_slug(name)?;
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
    // #3680 — the double-prefix guard (ADR-040 Rule 0) protects PREFIXED mints
    // from test-result-test-result-x. A BARE mint adds no prefix, so a name that
    // legitimately starts with the kind word (every crawler-minted Test is
    // test-*) is not a double-prefix. Guard only the prefixed grain.
    let n = if bare { normalize_name_bare(name)? } else { normalize_name(kind, name)? };
    Ok(if bare {
        format!("{}{}", NS, n)
    } else {
        format!("{}{}-{}", NS, kind, n)
    })
}

/// #3680 — bare-grain normalization: slug rules only, no kind-prefix guard.
pub fn normalize_name_bare(name: &str) -> R<String> {
    normalize_slug(name)
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
    /// Direct-path properties whose SHACL value channel is a literal field.
    /// This includes both sh:datatype properties and plain properties with no
    /// sh:class. Keeping the channel set separate from `datatypes` lets the DAL
    /// refuse a modeled plain property submitted as an IRI edge without closing
    /// the otherwise-open shape to truly unmodeled properties.
    pub field_properties: BTreeSet<String>,
    /// #3467 — edge property local name → target class local (sh:class), for edge-target-type enforcement
    pub edge_classes: BTreeMap<String, String>,
    /// #3681 — property local name → partition property local (chorus:uniqueWithin): the
    /// value must be unique among instances sharing the same partition-property value.
    pub unique_within: BTreeMap<String, String>,
    /// #3681 — property local names declared chorus:uniqueGlobal true: the value must be
    /// unique across ALL instances of the class (partition = the class itself).
    pub unique_global: Vec<String>,
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

impl Default for FusekiStore {
    fn default() -> Self {
        Self::new()
    }
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
        // #3392 — body via @file, never argv: a bulk seed (6.4K triples) blows
        // the OS ARG_MAX ceiling as an argument. curl reads + urlencodes the
        // file contents; behavior identical for small bodies.
        let body_file = std::env::temp_dir().join(format!(
            "athena-model-body-{}-{}.rq",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::write(&body_file, body).map_err(|e| format!("curl-bodyfile: {}", e))?;
        let mut args: Vec<String> = vec![
            "-sf".into(), "--max-time".into(), "60".into(),
            "-H".into(), "Accept: application/sparql-results+json".into(),
            "--data-urlencode".into(), format!("{}@{}", data_param, body_file.display()),
        ];
        if let Some(pw) = fuseki_admin_password() {
            let user = std::env::var("FUSEKI_ADMIN_USER")
                .unwrap_or_else(|_| "admin".to_string());
            args.push("-u".into());
            args.push(format!("{}:{}", user, pw));
        }
        args.push(format!("{}{}", self.endpoint, path));
        let out = Command::new("curl")
            .args(&args)
            .output()
            .map_err(|e| format!("curl-spawn: {}", e));
        let _ = std::fs::remove_file(&body_file);
        let out = out?;
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
    let mut field_properties: BTreeSet<String> = BTreeSet::new();
    for row in store.select_v(&format!(
        "PREFIX sh: <http://www.w3.org/ns/shacl#> SELECT ?v WHERE {{ GRAPH <{g}> {{ ?s sh:targetClass <{c}> ; sh:property ?p . ?p sh:path ?path . FILTER(isIRI(?path)) FILTER NOT EXISTS {{ ?p sh:class ?edgeClass }} OPTIONAL {{ ?p sh:datatype ?dt }} BIND(CONCAT(REPLACE(STR(?path), '.*#', ''), '|', IF(BOUND(?dt), REPLACE(STR(?dt), '.*#', ''), '')) AS ?v) }} }}",
        g = ONTOLOGY_GRAPH, c = class
    ))? {
        if let Some((prop, dt)) = row.split_once('|') {
            field_properties.insert(prop.to_string());
            if !dt.is_empty() {
                datatypes.insert(prop.to_string(), dt.to_string());
            }
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
    // #3681 — uniqueness-within-scope annotations on the sh:property node (same
    // read style as sh:datatype/sh:class). `chorus:uniqueWithin <partitionProp>`
    // partitions by another property's value; `chorus:uniqueGlobal true` is class-wide.
    let mut unique_within: BTreeMap<String, String> = BTreeMap::new();
    for row in store.select_v(&format!(
        "PREFIX sh: <http://www.w3.org/ns/shacl#> PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <{g}> {{ ?s sh:targetClass <{c}> ; sh:property ?p . ?p sh:path ?path ; chorus:uniqueWithin ?part . FILTER(isIRI(?path)) FILTER(isIRI(?part)) BIND(CONCAT(REPLACE(STR(?path), '.*#', ''), '|', REPLACE(STR(?part), '.*#', '')) AS ?v) }} }}",
        ns = NS, g = ONTOLOGY_GRAPH, c = class
    ))? {
        if let Some((prop, part)) = row.split_once('|') {
            unique_within.insert(prop.to_string(), part.to_string());
        }
    }
    let unique_global: Vec<String> = store.select_v(&format!(
        "PREFIX sh: <http://www.w3.org/ns/shacl#> PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <{g}> {{ ?s sh:targetClass <{c}> ; sh:property ?p . ?p sh:path ?path ; chorus:uniqueGlobal true . FILTER(isIRI(?path)) BIND(REPLACE(STR(?path), '.*#', '') AS ?v) }} }}",
        ns = NS, g = ONTOLOGY_GRAPH, c = class
    ))?;

    // Every constrained value property is also a field unless the same shape
    // explicitly declares it as an sh:class edge. Unioning the declarative
    // sources keeps the channel classification complete even when a store's
    // projection omits an unconstrained/plain row from the combined read.
    for prop in required
        .iter()
        .chain(enums.keys())
        .chain(datatypes.keys())
        .chain(unique_within.keys())
        .chain(unique_global.iter())
    {
        if !edge_classes.contains_key(prop) {
            field_properties.insert(prop.clone());
        }
    }

    Ok(ShapeReq {
        required,
        enums,
        datatypes,
        field_properties,
        edge_classes,
        unique_within,
        unique_global,
    })
}

/// Turtle string-literal escape.
fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\r', "\\r")
}

/// One write request — everything the DAL needs to form, validate, and land
/// an instance. Fields are datatype properties (string literals, v1);
/// edges are object properties whose targets are (kind, name) pairs that the
/// mint resolves — callers never pass IRIs anywhere.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriteReq {
    /// #3718 — a stated reason for writing outside the DERIVED placement.
    /// ADR-051 x 025 allows an override, never a silent one: an unexplained
    /// override is how a placement stops matching the model without anyone
    /// noticing (11 Product instances in two wrong graphs, months unseen).
    #[serde(default, alias = "placementOverrideReason")]
    pub placement_override_reason: Option<String>,
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
    #[serde(default)]
    pub edges: Vec<(String, String, String)>, // (property, target_kind, target_name)
    /// #3647 — the class's model-declared instance HOME graph (athena-make resolves it
    /// via resolve_instances_graph and passes --graph). `None` = the legacy
    /// urn:chorus:instances default (back-compat). Writing the declared home is
    /// what makes the entity readable + authorizable (no orphan): athena-make authz
    /// reads ownedBy from this same graph, so create must land here, not the bucket.
    #[serde(default)]
    pub graph: Option<String>,
}

/// Decode the `add-batch` stdin NDJSON stream: one WriteReq-shaped object per
/// nonblank line. Edges use the existing typed tuple representation:
/// `["partOf","product","athena"]`. Serde rejects unknown fields, wrong
/// value types, malformed escapes, wrong tuple cardinality, and trailing input;
/// the error names the exact record line.
pub fn parse_add_batch_ndjson(input: &str) -> R<Vec<WriteReq>> {
    if input.trim().is_empty() {
        return Err("add-batch: stdin is empty — expected one WriteReq JSON object per line".into());
    }
    let mut reqs = Vec::new();
    for (index, line) in input.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let req: WriteReq = serde_json::from_str(line).map_err(|e| {
            format!(
                "add-batch: invalid NDJSON record at line {}, column {}: {}",
                index + 1,
                e.column(),
                e
            )
        })?;
        reqs.push(req);
    }
    if reqs.is_empty() {
        return Err("add-batch: stdin contains no entity records".into());
    }
    Ok(reqs)
}

/// Validation + serialization, pure (store only consulted for shapes/integrity
/// by the caller). Produces the canonical Turtle for the subject.
pub fn to_turtle(req: &WriteReq) -> R<(String, String)> {
    let subject = mint(&req.kind, &req.name)?;
    let class = class_iri(&req.kind)?;
    let mut lines = vec![format!("<{}> a <{}>", subject, class)];
    for (prop, val) in &req.fields {
        check_property_local(prop)?;
        lines.push(format!("    <{}{}> \"{}\"", NS, prop, esc(val)));
    }
    for (prop, tkind, tname) in &req.edges {
        check_property_local(prop)?;
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

/// A valid high-resolution variant used only by create-only batch commits. The
/// existing audit predicates double as an outcome marker, avoiding any private
/// proof predicate or graph while leaving single-write timestamp semantics
/// unchanged.
static CREATE_STAMP_COUNTER: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

fn create_only_stamp() -> String {
    let clock = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    // Concurrent processes on one Fuseki host have distinct PIDs; commits in
    // one process have distinct counters even when the wall clock exposes the
    // same nanosecond. PID reuse cannot overlap, and the second+nanos prefix
    // distinguishes successive process lifetimes. Fractional xsd:dateTime
    // permits arbitrary precision, so the concatenated digits remain a valid
    // timestamp rather than introducing a model-private transaction token.
    let sequence = CREATE_STAMP_COUNTER
        .fetch_update(
            std::sync::atomic::Ordering::Relaxed,
            std::sync::atomic::Ordering::Relaxed,
            |value| value.checked_add(1),
        )
        .expect("create-only audit sequence exhausted");
    let suffix = format!(
        "{:09}{:010}{:020}",
        clock.subsec_nanos(),
        std::process::id(),
        sequence,
    );
    let base = now_iso();
    if base.ends_with('Z') {
        format!("{}.{}Z", base.trim_end_matches('Z'), suffix)
    } else {
        // `date` is a required production utility; this branch retains the
        // old fail-soft behavior but cannot claim xsd:dateTime syntax.
        format!("epoch:{}.{}", clock.as_secs(), suffix)
    }
}

#[cfg(test)]
thread_local! {
    /// A thread-local recorder keeps witness-cardinality tests hermetic and
    /// parallel-safe. In production each `witness` invocation below is exactly
    /// one synchronous chorus-log subprocess, so bounding calls bounds spawns.
    static TEST_WITNESSES: std::cell::RefCell<Vec<String>> = const {
        std::cell::RefCell::new(Vec::new())
    };
}

/// Spine witness — every write and every refusal is logged via chorus-log
/// (the crawler's zero-dep pattern). Best-effort: a logging failure goes to
/// stderr but never changes the write's outcome.
fn witness(event: &str, kvs: &[(&str, &str)]) {
    #[cfg(test)]
    {
        let _ = kvs;
        TEST_WITNESSES.with(|events| events.borrow_mut().push(event.to_string()));
    }
    #[cfg(not(test))]
    {
        let root = std::env::var("CHORUS_ROOT")
            .unwrap_or_else(|_| "/Users/jeffbridwell/CascadeProjects/chorus".to_string());
        // #3651 — NEVER mint a role identity as a default. An env-less caller logs
        // as "unattributed"; the identity gate (verify_identity) has already refused
        // its writes, so this only labels the refusal events themselves.
        let role = std::env::var("DEPLOY_ROLE").unwrap_or_else(|_| "unattributed".to_string());
        let mut args: Vec<String> = vec![event.to_string(), role];
        for (k, v) in kvs {
            args.push(format!("{}={}", k, v)); // the crawler's exact arg shape
        }
        let r = Command::new(format!("{}/platform/scripts/chorus-log", root)).args(&args).output();
        if let Err(e) = r {
            eprintln!("athena-model: witness emit failed ({}): {}", e, event);
        }
    }
}

/// #3651 — a VERIFIED writer identity. The ONLY constructor is `verify_identity`
/// (Principal-registry-checked, fail-closed); every mutating verb requires one,
/// so an unverified caller cannot reach a write — the type is the proof. This
/// closes the audit's top gap: the DAL defaulted DEPLOY_ROLE to "silas"/"system",
/// letting any process shelling the binary bypass the door and self-attribute.
#[derive(Debug)]
pub struct Identity(String);

impl Identity {
    pub fn role(&self) -> &str {
        &self.0
    }
    /// Resolve at the CLI boundary. #3687 — the migration flip (#3356 design
    /// step 3): a verified CSS identity **token** (`CHORUS_IDENTITY_TOKEN`) is
    /// now the ONLY attribution path. `DEPLOY_ROLE` env-trust is retired — the
    /// env string was as forgeable as `export DEPLOY_ROLE=<any registered
    /// principal>`, the exact hole this closes. A token that is PRESENT but does
    /// not verify fails closed in `verify_identity_token` (never degrades). A
    /// token that is ABSENT/blank now REFUSES here, before any store contact —
    /// where it used to fall back to the env path.
    ///
    /// Callers mint a token with `chorus-identity-token <role>` (the #3690
    /// cred-reader); the sourced `chorus-model()` wrapper does this transparently
    /// for role sessions. `--dry-run` needs no identity (handled upstream in
    /// main.rs — it writes nothing).
    pub fn resolve(store: &dyn Store) -> R<Identity> {
        match std::env::var("CHORUS_IDENTITY_TOKEN") {
            Ok(token) if !token.trim().is_empty() => {
                let now = now_secs();
                let verifier = OidcTokenVerifier::new(now);
                verify_identity_token(&token, &verifier, store, now)
            }
            _ => {
                witness("model.refused", &[("reason", "identity-token-required")]);
                Err("identity-token-required: no verified CHORUS_IDENTITY_TOKEN — the DAL \
                     no longer accepts DEPLOY_ROLE env-trust (retired #3687, fail closed). \
                     Mint one: export CHORUS_IDENTITY_TOKEN=\"$(chorus-identity-token <role>)\"."
                    .into())
            }
        }
    }
}

/// Verify a claimed identity against the Principal registry (ADR-052 §5 — the
/// same allow-set-as-data the athena-make door resolves). Fail-closed at every step:
/// absent → refuse; malformed (before any query — no claim text ever reaches
/// SPARQL) → refuse; not a registered chorus:Principal → refuse; registry
/// unreachable → the store error propagates, still a refusal.
pub fn verify_identity(claim: Option<&str>, store: &dyn Store) -> R<Identity> {
    let claim = match claim.map(str::trim).filter(|c| !c.is_empty()) {
        Some(c) => c,
        None => {
            witness("model.refused", &[("reason", "identity-missing")]);
            return Err(
                "identity-missing: DEPLOY_ROLE is unset — the DAL refuses unattributed writes \
                 (fail closed, #3651). Set DEPLOY_ROLE=<registered principal>."
                    .into(),
            );
        }
    };
    let ok_syntax = claim.len() <= 32
        && claim.chars().next().map(|c| c.is_ascii_lowercase()).unwrap_or(false)
        && claim.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !ok_syntax {
        witness("model.refused", &[("reason", "identity-malformed")]);
        return Err(format!(
            "identity-malformed: '{}' — a principal claim is lowercase kebab, ≤32 chars",
            claim
        ));
    }
    let known = store.ask(&format!(
        "ASK {{ GRAPH <{g}> {{ <{ns}principal-{c}> a <{ns}Principal> }} }}",
        g = SECURITY_GRAPH, ns = NS, c = claim
    ))?;
    if !known {
        witness("model.refused", &[("reason", "identity-unknown"), ("claim", claim)]);
        return Err(format!(
            "identity-unknown: '{}' is not a registered chorus:Principal in <{}> — writes refuse (fail closed, #3651)",
            claim, SECURITY_GRAPH
        ));
    }
    Ok(Identity(claim.to_string()))
}

/// #3356 step 1 — verify a real CSS identity **token** (ES256/JWKS), not a
/// self-declared env string. The concrete verifier is the SHARED oidc crate
/// reused from athena-make's door (NOT a second implementation — principle:
/// no-competing-implementations); this trait is the seam so the DAL's identity
/// BINDING logic is built + tested independently of the crate/transport wiring.
pub trait TokenVerifier {
    /// Verify signature + claims (iss/exp/aud); return the token's verified WebID,
    /// or an error string. Anything unverifiable MUST error (fail-closed).
    fn verify_webid(&self, token: &str, now_secs: u64) -> Result<String, String>;
}

/// #3356 step 1 — the identity TOKEN path (additive; `verify_identity`'s
/// DEPLOY_ROLE path is untouched and stays the fallback until the migration flip,
/// step 3). Verify the token, then bind its **verified** WebID to the Principal
/// that owns it via `chorus:webId` in the security graph — a graph lookup, not a
/// string. Forging a writer now requires the credential, not `export DEPLOY_ROLE=`.
pub fn verify_identity_token(
    token: &str,
    verifier: &dyn TokenVerifier,
    store: &dyn Store,
    now_secs: u64,
) -> R<Identity> {
    let webid = match verifier.verify_webid(token, now_secs) {
        Ok(w) => w,
        Err(e) => {
            witness("model.refused", &[("reason", "identity-token-invalid")]);
            return Err(format!(
                "identity-token-invalid: {} — the CSS token did not verify (fail closed, #3356)",
                e
            ));
        }
    };
    // Bind the VERIFIED WebID → its Principal (graph, not a self-declared string).
    // #3690 — chorus:webId is an xsd:string LITERAL (per NostrCredentialShape /
    // the seeded principals), so the object is "{w}", NOT <{w}>. The IRI form
    // silently matched nothing and every real minted token reported
    // identity-webid-unregistered — the #3356 identity_gate stub matched on a
    // substring so this never surfaced until the first live client-credentials
    // token ran end-to-end (#3690). STR-compare so a stray typed literal still
    // matches on value.
    let claim = store
        .select_v(&format!(
            "PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <{g}> {{ ?p a chorus:Principal ; chorus:webId ?wid . FILTER(STR(?wid) = \"{w}\") BIND(REPLACE(STR(?p), '.*#principal-', '') AS ?v) }} }}",
            ns = NS, g = SECURITY_GRAPH, w = webid
        ))?
        .into_iter()
        .next();
    match claim {
        // Reuse the existing syntax + registry gate — one identity door, two entrances.
        Some(c) => verify_identity(Some(&c), store),
        None => {
            witness("model.refused", &[("reason", "identity-webid-unregistered"), ("webid", &webid)]);
            Err(format!(
                "identity-webid-unregistered: verified WebID <{}> owns no chorus:Principal in <{}> — writes refuse (fail closed, #3356)",
                webid, SECURITY_GRAPH
            ))
        }
    }
}

/// #3356 stage 3 — the CONCRETE `TokenVerifier`: the shared chorus-oidc
/// `OidcVerifier` (the exact ES256/JWKS path athena-make's door runs) adapted to the
/// DAL's seam. One verifier, two consumers — the whole point of the extraction.
/// Boot wiring mirrors athena-make: issuer from `CSS_ISSUER`, the Principal allow-set
/// resolved from the model on the ALLOW_TTL cadence, JWKS fetched via curl with a
/// kid-keyed cache. `StubVerifier` in the tests pins the BINDING logic; this pins
/// the real crypto path against the live store.
pub struct OidcTokenVerifier {
    inner: chorus_oidc::oidc::OidcVerifier,
}

impl OidcTokenVerifier {
    /// Build against the deployment's CSS issuer (JWKS source) and Fuseki endpoint
    /// (allow-set source). Warms both caches; never blocks (a cold CSS just means
    /// the first verify fetches). The endpoint is env-derived, same as `FusekiStore`.
    pub fn new(now_secs: u64) -> Self {
        let css_issuer =
            std::env::var("CSS_ISSUER").unwrap_or_else(|_| "http://localhost:3001/".to_string());
        // #3690 — the JWKS hairpin. The token's `iss` is the LOGICAL issuer
        // (https://id.…, browser-facing), which `css_issuer` must equal for the
        // iss-check. But that origin is behind Cloudflare — a server-side curl
        // to its /.oidc/jwks gets 1010-blocked. So the JWKS FETCH targets CSS
        // locally (CHORUS_JWKS_URL, e.g. http://localhost:3001/.oidc/jwks) with
        // the trusted-proxy headers CSS honors on loopback (Host +
        // X-Forwarded-Proto/Host) so it serves keys AS the logical issuer — the
        // same Host-override the Clearing's #3669 exchange uses. Absent the
        // override, fall back to {issuer}/.oidc/jwks (off-box deploys).
        let jwks_url = std::env::var("CHORUS_JWKS_URL")
            .unwrap_or_else(|_| format!("{}/.oidc/jwks", css_issuer.trim_end_matches('/')));
        let issuer_host = css_issuer
            .split("://").nth(1).unwrap_or(&css_issuer)
            .trim_end_matches('/').to_string();
        let allow_endpoint =
            std::env::var("CHORUS_FUSEKI").unwrap_or_else(|_| FUSEKI.to_string());
        let role_endpoint = allow_endpoint.clone();
        let scope_endpoint = allow_endpoint.clone();
        let inner = chorus_oidc::oidc::OidcVerifier::new(
            &css_issuer,
            // allow-set: re-resolve lazily on the ALLOW_TTL cadence so a model
            // revocation propagates within one token TTL (athena-make §5 parity).
            move || {
                chorus_oidc::oidc::resolve_principal_webids(|q| {
                    fuseki_query_json(&allow_endpoint, q).ok()
                })
            },
            // #3688 — the holdsRole map (ADR-054 §3.3). The DAL binds its own
            // attribution through the Principal lookup in `verify_identity_token`,
            // so it doesn't read Claims.agent_id; the resolver is wired anyway so
            // the shared verifier runs ONE configuration in both consumers.
            move || {
                chorus_oidc::oidc::resolve_principal_roles(|q| {
                    fuseki_query_json(&role_endpoint, q).ok()
                })
            },
            // #3689 — hasScope resolver, same one-configuration rule as roles.
            move || {
                chorus_oidc::oidc::resolve_principal_scopes(|q| {
                    fuseki_query_json(&scope_endpoint, q).ok()
                })
            },
            move || {
                // forwarded headers matter for the local hairpin; harmless
                // (ignored) when jwks_url is the real off-box issuer.
                let out = Command::new("curl")
                    .args([
                        "-sf", "--max-time", "3",
                        "-H", &format!("Host: {}", issuer_host),
                        "-H", "X-Forwarded-Proto: https",
                        "-H", &format!("X-Forwarded-Host: {}", issuer_host),
                        &jwks_url,
                    ])
                    .output()
                    .ok()?;
                if !out.status.success() {
                    return None;
                }
                Some(String::from_utf8_lossy(&out.stdout).into_owned())
            },
        );
        inner.warm_allow(now_secs);
        inner.warm_fetch(now_secs);
        Self { inner }
    }
}

impl TokenVerifier for OidcTokenVerifier {
    fn verify_webid(&self, token: &str, now_secs: u64) -> Result<String, String> {
        self.inner
            .verify(token, now_secs)
            .map(|claims| claims.web_id)
            .map_err(|e| format!("{:?}", e))
    }
}

/// Epoch seconds (zero-dep; the `now_iso` sibling for the subprocess-free path).
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Raw SPARQL-JSON GET against Fuseki `/query` — the allow-set resolver's source
/// (`Store::select_v` parses to values; the resolver needs the raw JSON body).
/// Carries `FUSEKI_ADMIN_*` like `FusekiStore::curl`; anonymous when unset.
fn fuseki_query_json(endpoint: &str, sparql: &str) -> R<String> {
    let mut args: Vec<String> = vec![
        "-sf".into(),
        "--max-time".into(),
        "20".into(),
        "-H".into(),
        "Accept: application/sparql-results+json".into(),
        "--data-urlencode".into(),
        format!("query={}", sparql),
    ];
    if let Some(pw) = fuseki_admin_password() {
        let user = std::env::var("FUSEKI_ADMIN_USER").unwrap_or_else(|_| "admin".to_string());
        args.push("-u".into());
        args.push(format!("{}:{}", user, pw));
    }
    args.push(format!("{}/query", endpoint));
    let out = Command::new("curl")
        .args(&args)
        .output()
        .map_err(|e| format!("curl-spawn: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "fuseki-query: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// #3356 AC4 — per-graph authz, the DAL side. This module is the INSTANCES
/// writer by contract (line 3); two graphs are reserved for the DBA path and MUST
/// be unwritable from here regardless of a verified identity:
///   - `ONTOLOGY_GRAPH` — the schema is athena-make's (the DBA path). A DAL write
///     would let an instance writer mutate the very shapes that validate it.
///   - `SECURITY_GRAPH` — the Principal registry. A DAL write is the exact
///     priv-esc the design names (a writer minting itself a Principal, or
///     rewriting the allow-set that authenticates it). Bootstrap/DBA only.
/// Every other `urn:chorus:*` (instances + domain graphs) stays writable. This is
/// the DOOR-LAYER half of per-graph authz — shiro stays binary auth (#3630
/// scope-note / ADR-050): Fuseki answers "authenticated writer?", the DAL answers
/// "which graph?". Applied at every mutation choke point (write/delete/edges/batch)
/// so the guard can't be bypassed by picking a different verb.
fn assert_dal_writable(graph: &str) -> R<()> {
    if graph == ONTOLOGY_GRAPH || graph == SECURITY_GRAPH {
        witness("model.refused", &[("graph", graph), ("reason", "graph-dba-only")]);
        return Err(format!(
            "graph-dba-only: <{}> is a DBA-path graph — the instances DAL is refused write \
             (per-graph authz, fail closed, #3356 AC4)",
            graph
        ));
    }
    Ok(())
}

/// The entity writer accepts only the two instance-placement forms generated by
/// athena-make: the legacy instances bucket and a lowercase domain home. This is
/// intentionally narrower than `seed`'s realm policy: migrations may target
/// governed gathering graphs, but caller-authored add/add-batch requests may not
/// interpolate arbitrary graph IRIs into SPARQL.
fn assert_instance_graph(graph: &str) -> R<()> {
    // Preserve the typed authorization refusal for the two reserved graphs.
    assert_dal_writable(graph)?;
    let domain_home = graph
        .strip_prefix("urn:chorus:domains:")
        .map(|local| normalize_slug(local).map(|normalized| normalized == local).unwrap_or(false))
        .unwrap_or(false);
    if graph != INSTANCES_GRAPH && !domain_home {
        witness("model.refused", &[("graph", graph), ("reason", "graph-not-instance-home")]);
        return Err(format!(
            "graph-not-instance-home: <{}> is not urn:chorus:instances or a safe urn:chorus:domains:<name> instance graph",
            graph
        ));
    }
    Ok(())
}

/// #3718 — which domain CLAIMS this class in `definesVocabulary`. The meta-model
/// is self-describing (Jeff, 2026-08-02): the `products` domain's
/// `definesVocabulary` IS `chorus:Product`. So a class's instance placement is
/// DERIVED from the domain that claims it — no per-shape annotation needed.
/// Read from the schema graph, which is where `definesVocabulary` lives.
pub fn defining_domain_of(store: &dyn Store, class_iri: &str) -> Option<String> {
    let q = format!(
        "PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <{g}> {{ ?d a chorus:Domain ; \
         chorus:definesVocabulary <{c}> BIND(REPLACE(STR(?d), '.*#', '') AS ?v) }} }} LIMIT 1",
        ns = NS, g = ONTOLOGY_GRAPH, c = class_iri
    );
    store.select_v(&q).ok()?.into_iter().next()
}

#[derive(Debug)]
struct WritePlanError {
    identity: String,
    message: String,
}

impl WritePlanError {
    fn for_req(req: &WriteReq, message: String) -> Self {
        Self { identity: format!("{}:{}", req.kind, req.name), message }
    }

    fn for_batch(self) -> String {
        format!("add-batch: entity '{}': {}", self.identity, self.message)
    }
}

#[derive(Debug)]
struct PlannedWrite<'a> {
    req: &'a WriteReq,
    identity: String,
    class: String,
    subject: String,
    turtle: String,
    graph: String,
}

#[derive(Debug)]
struct UniquenessCandidate<'a> {
    req: &'a WriteReq,
    prop: String,
    partition: Option<String>,
    graph: String,
    class: String,
    partition_iri: Option<String>,
    value: String,
}

/// Successful result of one entity-generic `add-batch` transaction.
#[derive(Debug, PartialEq, Eq)]
pub struct AddBatchReport {
    pub subjects: Vec<String>,
}

#[derive(Debug, Default)]
struct ExternalTargetFacts {
    existing: BTreeSet<String>,
    typed: BTreeSet<(String, String)>,
}

/// Read every edge target outside the transaction in ONE query. The result
/// encodes `subject|rdf:type`; an empty type still proves existence. This is the
/// hot-path difference between a 200-TestResult chunk taking one Fuseki read and
/// taking ~400 sequential ASK/curl round trips (existence + type per result).
fn prefetch_external_targets(
    store: &dyn Store,
    plans: &[PlannedWrite<'_>],
    batch_classes: &BTreeMap<String, String>,
) -> Result<ExternalTargetFacts, WritePlanError> {
    let mut owners: BTreeMap<String, &WriteReq> = BTreeMap::new();
    for plan in plans {
        for (_, target_kind, target_name) in &plan.req.edges {
            let target = mint(target_kind, target_name)
                .map_err(|e| WritePlanError::for_req(plan.req, e))?;
            if !batch_classes.contains_key(&target) {
                owners.entry(target).or_insert(plan.req);
            }
        }
    }
    if owners.is_empty() {
        return Ok(ExternalTargetFacts::default());
    }

    let values = owners
        .keys()
        .map(|target| format!("<{}>", target))
        .collect::<Vec<_>>()
        .join(" ");
    let query = format!(
        "PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> \
         SELECT DISTINCT ?v WHERE {{ \
           VALUES ?target {{ {values} }} \
           GRAPH ?g {{ ?target ?p ?o }} \
           OPTIONAL {{ GRAPH ?typeGraph {{ ?target rdf:type ?type }} }} \
           BIND(CONCAT(STR(?target), '|', IF(BOUND(?type), STR(?type), '')) AS ?v) \
         }}",
        values = values
    );
    let first_owner = owners.values().next().expect("nonempty target owner set");
    let rows = store
        .select_v(&query)
        .map_err(|e| WritePlanError::for_req(first_owner, e))?;
    let mut facts = ExternalTargetFacts::default();
    for row in rows {
        let Some((target, class)) = row.split_once('|') else { continue };
        if !owners.contains_key(target) {
            continue;
        }
        facts.existing.insert(target.to_string());
        if !class.is_empty() {
            facts.typed.insert((target.to_string(), class.to_string()));
        }
    }
    Ok(facts)
}

/// Render the shared O(N) store-conflict pattern. Preflight supplies a planned-
/// subject exclusion because single writes are upserts; the atomic create-only
/// commit supplies none because its identity-absence predicate guarantees those
/// subjects do not exist in the successful snapshot.
fn uniqueness_conflict_pattern(
    candidates: &[UniquenessCandidate<'_>],
    exclude_replacements: Option<&str>,
) -> String {
    let global_rows = candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| candidate.partition.is_none())
        .map(|(index, candidate)| {
            format!(
                "(\"{index}\" <{graph}> <{class}> <{ns}{prop}> \"{value}\")",
                index = index,
                graph = candidate.graph,
                class = candidate.class,
                ns = NS,
                prop = candidate.prop,
                value = esc(&candidate.value),
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    let within_rows = candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            Some(format!(
                "(\"{index}\" <{graph}> <{ns}{prop}> <{ns}{partition}> <{partition_iri}> \"{value}\")",
                index = index,
                graph = candidate.graph,
                ns = NS,
                prop = candidate.prop,
                partition = candidate.partition.as_ref()?,
                partition_iri = candidate.partition_iri.as_ref()?,
                value = esc(&candidate.value),
            ))
        })
        .collect::<Vec<_>>()
        .join(" ");
    let exclude_replacements = exclude_replacements.unwrap_or("");
    let mut branches = Vec::new();
    if !global_rows.is_empty() {
        branches.push(format!(
            "{{ \
               VALUES (?v ?graph ?class ?property ?wanted) {{ {rows} }} \
               GRAPH ?graph {{ ?other a ?class ; ?property ?actual }} \
               FILTER(STR(?actual) = ?wanted) \
               {exclude} \
             }}",
            rows = global_rows,
            exclude = exclude_replacements,
        ));
    }
    if !within_rows.is_empty() {
        branches.push(format!(
            "{{ \
               VALUES (?v ?graph ?property ?partition ?partitionIri ?wanted) {{ {rows} }} \
               GRAPH ?graph {{ ?other ?property ?actual ; ?partition ?partitionIri }} \
               FILTER(STR(?actual) = ?wanted) \
               {exclude} \
             }}",
            rows = within_rows,
            exclude = exclude_replacements,
        ));
    }
    branches.join(" UNION ")
}

/// Resolve every store-side uniqueness candidate in one SELECT. Compact VALUES
/// tables bind stable input-order indexes as `?v`; the caller then reports the
/// first conflicting entity deterministically. This replaces one synchronous
/// ASK/curl subprocess per unique field with one O(N)-sized read for the batch.
fn prefetch_uniqueness_conflicts(
    store: &dyn Store,
    plans: &[PlannedWrite<'_>],
    candidates: &[UniquenessCandidate<'_>],
) -> Result<BTreeSet<usize>, WritePlanError> {
    if candidates.is_empty() {
        return Ok(BTreeSet::new());
    }
    let replacements = plans
        .iter()
        .map(|plan| format!("(<{}> <{}>)", plan.graph, plan.subject))
        .collect::<Vec<_>>()
        .join(" ");
    let exclude_replacements = format!(
        "FILTER NOT EXISTS {{ \
           VALUES (?replacementGraph ?replacement) {{ {replacements} }} \
           FILTER(?replacementGraph = ?graph && ?replacement = ?other) \
         }}",
        replacements = replacements,
    );
    let pattern = uniqueness_conflict_pattern(candidates, Some(&exclude_replacements));
    let rows = store
        .select_v(&format!(
            "# athena-model uniqueness candidates\nSELECT DISTINCT ?v WHERE {{ {} }}",
            pattern
        ))
        .map_err(|e| WritePlanError::for_req(candidates[0].req, e))?;
    let mut conflicts = BTreeSet::new();
    for row in rows {
        let index = row.parse::<usize>().map_err(|_| {
            WritePlanError::for_req(
                candidates[0].req,
                format!("uniqueness-read-invalid: unexpected candidate id '{}'", row),
            )
        })?;
        if index >= candidates.len() {
            return Err(WritePlanError::for_req(
                candidates[0].req,
                format!("uniqueness-read-invalid: out-of-range candidate id '{}'", row),
            ));
        }
        conflicts.insert(index);
    }
    Ok(conflicts)
}

/// Shared add planner. Every request reaches the same mint, graph, SHACL,
/// uniqueness, referential-integrity, and audit preparation whether the caller
/// used `add` or `add-batch`. The planner performs reads only; callers cannot
/// observe a partial write because no Store::update occurs here.
fn plan_writes<'a>(
    store: &dyn Store,
    reqs: &'a [WriteReq],
) -> Result<(Vec<PlannedWrite<'a>>, Vec<UniquenessCandidate<'a>>), WritePlanError> {
    let mut plans: Vec<PlannedWrite<'a>> = Vec::with_capacity(reqs.len());
    let mut claimed: BTreeMap<String, String> = BTreeMap::new();

    for req in reqs {
        let identity = format!("{}:{}", req.kind, req.name);
        let class = class_iri(&req.kind).map_err(|e| WritePlanError::for_req(req, e))?;
        let (subject, turtle) = to_turtle(req).map_err(|e| WritePlanError::for_req(req, e))?;
        let graph = req.graph.clone().unwrap_or_else(|| INSTANCES_GRAPH.to_string());
        assert_instance_graph(&graph).map_err(|e| WritePlanError::for_req(req, e))?;
        if let Some(first) = claimed.insert(subject.clone(), identity.clone()) {
            witness(
                "model.refused",
                &[("kind", req.kind.as_str()), ("name", req.name.as_str()), ("reason", "duplicate-identity")],
            );
            return Err(WritePlanError::for_req(
                req,
                format!(
                    "duplicate-identity: '{}' and '{}' both mint <{}> — one transaction may replace an identity only once",
                    first, identity, subject
                ),
            ));
        }
        plans.push(PlannedWrite { req, identity, class, subject, turtle, graph });
    }

    // Shape reads are six SELECTs today. Cache by class so 1,000 entities of one
    // kind issue those six reads once, not 6,000 times.
    let mut shapes: BTreeMap<String, ShapeReq> = BTreeMap::new();
    for plan in &plans {
        if !shapes.contains_key(&plan.class) {
            let shape = read_shape(store, &plan.class)
                .map_err(|e| WritePlanError::for_req(plan.req, e))?;
            shapes.insert(plan.class.clone(), shape);
        }
    }

    // A SHACL property's value channel is part of its type contract. Accepting
    // an sh:class property in `fields` serializes an RDF literal and bypasses
    // referential-integrity/target-type checks; accepting a modeled field
    // property in `edges` serializes an IRI and bypasses its literal contract.
    // Refuse either mismatch before target or uniqueness reads, and therefore
    // before the transaction's sole update.
    for plan in &plans {
        let req = plan.req;
        let shape = &shapes[&plan.class];
        if let Some((prop, target_class)) = req
            .fields
            .keys()
            .find_map(|prop| shape.edge_classes.get(prop).map(|class| (prop, class)))
        {
            witness(
                "model.refused",
                &[
                    ("kind", req.kind.as_str()),
                    ("name", req.name.as_str()),
                    ("reason", "shape-channel-violation"),
                    ("field", prop.as_str()),
                ],
            );
            return Err(WritePlanError::for_req(
                req,
                format!(
                    "shape-channel-violation: '{}' declares sh:class {} and must be supplied through edges, not fields",
                    prop, target_class
                ),
            ));
        }
        if let Some(prop) = req
            .edges
            .iter()
            .map(|(prop, _, _)| prop)
            .find(|prop| shape.field_properties.contains(*prop))
        {
            witness(
                "model.refused",
                &[
                    ("kind", req.kind.as_str()),
                    ("name", req.name.as_str()),
                    ("reason", "shape-channel-violation"),
                    ("edge", prop.as_str()),
                ],
            );
            return Err(WritePlanError::for_req(
                req,
                format!(
                    "shape-channel-violation: '{}' is a modeled literal property and must be supplied through fields, not edges",
                    prop
                ),
            ));
        }
    }

    // The transaction-local world: edge targets in this set will exist after
    // the same commit. Their planned rdf:type is authoritative for sh:class.
    let batch_classes: BTreeMap<String, String> = plans
        .iter()
        .map(|p| (p.subject.clone(), p.class.clone()))
        .collect();
    let external_targets = prefetch_external_targets(store, &plans, &batch_classes)?;
    let mut seen_global: BTreeMap<(String, String, String, String), String> = BTreeMap::new();
    let mut seen_within: BTreeMap<(String, String, String, String, String, String), String> = BTreeMap::new();
    let mut uniqueness_candidates = Vec::new();

    for plan in &plans {
        let req = plan.req;
        let shape = &shapes[&plan.class];

        for need in &shape.required {
            let satisfied = req.fields.contains_key(need)
                || req.edges.iter().any(|(p, _, _)| p == need)
                || need == "label";
            if !satisfied {
                witness("model.refused", &[("kind", req.kind.as_str()), ("name", req.name.as_str()), ("reason", "shape-violation"), ("field", need)]);
                return Err(WritePlanError::for_req(req, format!(
                    "shape-violation: {} requires '{}' (sh:minCount 1, from {})",
                    plan.class, need, ONTOLOGY_GRAPH
                )));
            }
        }
        for (prop, allowed) in &shape.enums {
            if let Some(value) = req.fields.get(prop) {
                if !allowed.contains(value) {
                    return Err(WritePlanError::for_req(req, format!(
                        "shape-violation: '{}' not in sh:in {:?} for {}", value, allowed, prop
                    )));
                }
            }
        }
        for (prop, value) in &req.fields {
            if let Some(datatype) = shape.datatypes.get(prop) {
                if !datatype_ok(value, datatype) {
                    witness("model.refused", &[("kind", req.kind.as_str()), ("name", req.name.as_str()), ("reason", "shape-violation"), ("field", prop)]);
                    return Err(WritePlanError::for_req(req, format!(
                        "shape-violation: '{}' is not a valid xsd:{} for '{}'", value, datatype, prop
                    )));
                }
            }
        }

        // Store uniqueness excludes every planned subject in its destination
        // graph. Single write replaces its one subject; create-only add-batch
        // separately rejects any planned identity already present. Final values
        // are checked by seen_global/seen_within before the compact store read.
        for (prop, partition) in &shape.unique_within {
            let Some(value) = req.fields.get(prop) else { continue };
            match req.edges.iter().find(|(p, _, _)| p == partition) {
                Some((_, target_kind, target_name)) => {
                    let partition_iri = mint(target_kind, target_name)
                        .map_err(|e| WritePlanError::for_req(req, e))?;
                    let key = (
                        plan.graph.clone(), plan.class.clone(), prop.clone(), partition.clone(),
                        partition_iri.clone(), value.clone(),
                    );
                    if let Some(first) = seen_within.insert(key, plan.identity.clone()) {
                        witness("model.refused", &[("kind", req.kind.as_str()), ("name", req.name.as_str()), ("reason", "uniqueness-violation"), ("field", prop)]);
                        return Err(WritePlanError::for_req(req, format!(
                            "shape-violation: duplicate '{}' within '{}' inside this batch (also used by entity '{}'; chorus:uniqueWithin, from {})",
                            prop, partition, first, ONTOLOGY_GRAPH
                        )));
                    }
                    uniqueness_candidates.push(UniquenessCandidate {
                        req,
                        prop: prop.clone(),
                        partition: Some(partition.clone()),
                        graph: plan.graph.clone(),
                        class: plan.class.clone(),
                        partition_iri: Some(partition_iri),
                        value: value.clone(),
                    });
                }
                None => {
                    witness(
                        "model.refused",
                        &[
                            ("kind", req.kind.as_str()),
                            ("name", req.name.as_str()),
                            ("reason", "missing-uniqueness-partition"),
                            ("field", prop.as_str()),
                        ],
                    );
                    return Err(WritePlanError::for_req(
                        req,
                        format!(
                            "shape-violation: '{}' declares chorus:uniqueWithin '{}' but entity '{}' has no '{}' edge — uniqueness cannot be scoped (fail-closed, from {})",
                            prop, partition, plan.identity, partition, ONTOLOGY_GRAPH
                        ),
                    ));
                }
            }
        }
        for prop in &shape.unique_global {
            let Some(value) = req.fields.get(prop) else { continue };
            let key = (plan.graph.clone(), plan.class.clone(), prop.clone(), value.clone());
            if let Some(first) = seen_global.insert(key, plan.identity.clone()) {
                witness("model.refused", &[("kind", req.kind.as_str()), ("name", req.name.as_str()), ("reason", "uniqueness-violation"), ("field", prop)]);
                return Err(WritePlanError::for_req(req, format!(
                    "shape-violation: duplicate '{}' across all {} inside this batch (also used by entity '{}'; chorus:uniqueGlobal, from {})",
                    prop, req.kind, first, ONTOLOGY_GRAPH
                )));
            }
            uniqueness_candidates.push(UniquenessCandidate {
                req,
                prop: prop.clone(),
                partition: None,
                graph: plan.graph.clone(),
                class: plan.class.clone(),
                partition_iri: None,
                value: value.clone(),
            });
        }

    }

    let uniqueness_conflicts =
        prefetch_uniqueness_conflicts(store, &plans, &uniqueness_candidates)?;
    if let Some(index) = uniqueness_conflicts.into_iter().next() {
        let candidate = &uniqueness_candidates[index];
        witness(
            "model.refused",
            &[
                ("kind", candidate.req.kind.as_str()),
                ("name", candidate.req.name.as_str()),
                ("reason", "uniqueness-violation"),
                ("field", candidate.prop.as_str()),
            ],
        );
        let message = match &candidate.partition {
            Some(partition) => format!(
                "shape-violation: duplicate '{}' within '{}' (chorus:uniqueWithin, from {})",
                candidate.prop, partition, ONTOLOGY_GRAPH
            ),
            None => format!(
                "shape-violation: duplicate '{}' across all {} (chorus:uniqueGlobal, from {})",
                candidate.prop, candidate.req.kind, ONTOLOGY_GRAPH
            ),
        };
        return Err(WritePlanError::for_req(candidate.req, message));
    }

    for plan in &plans {
        let req = plan.req;
        let shape = &shapes[&plan.class];
        for (prop, target_kind, target_name) in &req.edges {
            let target = mint(target_kind, target_name)
                .map_err(|e| WritePlanError::for_req(req, e))?;
            let exists = batch_classes.contains_key(&target)
                || external_targets.existing.contains(&target);
            if !exists {
                witness("model.refused", &[("kind", req.kind.as_str()), ("name", req.name.as_str()), ("reason", "unknown-target"), ("edge", prop)]);
                return Err(WritePlanError::for_req(req, format!(
                    "unknown-target: {} → <{}> exists neither in the store nor in this batch (referential integrity, fail-closed)",
                    prop, target
                )));
            }
            if let Some(want_class) = shape.edge_classes.get(prop) {
                let wanted = format!("{}{}", NS, want_class);
                let typed = if let Some(actual) = batch_classes.get(&target) {
                    actual == &wanted
                } else {
                    external_targets.typed.contains(&(target.clone(), wanted))
                };
                if !typed {
                    witness("model.refused", &[("kind", req.kind.as_str()), ("name", req.name.as_str()), ("reason", "shape-violation"), ("edge", prop)]);
                    return Err(WritePlanError::for_req(req, format!(
                        "shape-violation: {} → <{}> is not a {} (sh:class edge-target-type, fail-closed)",
                        prop, target, want_class
                    )));
                }
            }
        }
    }

    Ok((plans, uniqueness_candidates))
}

/// Resolve pre-existing batch identities from the model in one bounded SELECT
/// per destination graph. Subjects come from the DAL mint in `PlannedWrite`,
/// never from a caller's raw name, so prefixed kinds and normalization aliases
/// cannot evade create-only conflict detection. Any read error propagates and
/// refuses the batch before mutation.
fn existing_batch_subjects(
    store: &dyn Store,
    plans: &[PlannedWrite<'_>],
) -> Result<BTreeSet<String>, WritePlanError> {
    let mut by_graph: BTreeMap<&str, Vec<&PlannedWrite<'_>>> = BTreeMap::new();
    for plan in plans {
        by_graph.entry(plan.graph.as_str()).or_default().push(plan);
    }
    let mut existing = BTreeSet::new();
    for (graph, group) in by_graph {
        let values = group
            .iter()
            .map(|plan| format!("<{}>", plan.subject))
            .collect::<Vec<_>>()
            .join(" ");
        let rows = store
            .select_v(&format!(
                "SELECT DISTINCT ?v WHERE {{ VALUES ?candidate {{ {values} }} GRAPH <{graph}> {{ ?candidate ?p ?o }} BIND(STR(?candidate) AS ?v) }}",
                values = values,
                graph = graph,
            ))
            .map_err(|e| WritePlanError::for_req(group[0].req, e))?;
        existing.extend(rows);
    }
    Ok(existing)
}

/// Load every existing dcterms:created value in one SELECT per target graph.
/// This preserves the single-write upsert's created-once audit rule; create-only
/// `add_batch` deliberately skips this read because every subject must be new.
fn existing_created(
    store: &dyn Store,
    plans: &[PlannedWrite<'_>],
) -> Result<BTreeMap<String, String>, WritePlanError> {
    const DCT: &str = "http://purl.org/dc/terms/";
    let mut by_graph: BTreeMap<&str, Vec<&PlannedWrite<'_>>> = BTreeMap::new();
    for plan in plans {
        by_graph.entry(plan.graph.as_str()).or_default().push(plan);
    }
    let mut found = BTreeMap::new();
    for (graph, group) in by_graph {
        let values = group.iter().map(|p| format!("<{}>", p.subject)).collect::<Vec<_>>().join(" ");
        let rows = store.select_v(&format!(
            "SELECT ?v WHERE {{ VALUES ?s {{ {values} }} GRAPH <{graph}> {{ ?s <{dct}created> ?created }} BIND(CONCAT(STR(?s), '|', STR(?created)) AS ?v) }}",
            values = values, graph = graph, dct = DCT
        )).map_err(|e| WritePlanError::for_req(group[0].req, e))?;
        for row in rows {
            if let Some((subject, created)) = row.split_once('|') {
                found.insert(subject.to_string(), created.to_string());
            }
        }
    }
    Ok(found)
}

/// Prove that the conditional create-only update was the writer that produced
/// every requested subject. `created` and `modified` are stamped with the same
/// nanosecond-resolution value only by this commit, so the proof needs no
/// private marker predicate or proof graph. A missing row means the atomic
/// FILTER NOT EXISTS branch did not run (normally an identity race).
fn prove_create_only_commit(
    store: &dyn Store,
    plans: &[PlannedWrite<'_>],
    uniqueness_candidates: &[UniquenessCandidate<'_>],
    creator: &str,
    stamp: &str,
) -> R<()> {
    const DCT: &str = "http://purl.org/dc/terms/";
    let values = plans
        .iter()
        .map(|plan| format!("(<{}> <{}>)", plan.graph, plan.subject))
        .collect::<Vec<_>>()
        .join(" ");
    let query = format!(
        "# athena-model create-only outcome proof\n\
         SELECT DISTINCT ?v WHERE {{ \
           VALUES (?candidateGraph ?candidate) {{ {values} }} \
           GRAPH ?candidateGraph {{ \
             ?candidate <{dct}created> \"{stamp}\" ; \
                        <{dct}modified> \"{stamp}\" ; \
                        <{dct}creator> \"{creator}\" . \
           }} \
           BIND(STR(?candidate) AS ?v) \
         }}",
        values = values,
        dct = DCT,
        stamp = esc(stamp),
        creator = esc(creator),
    );
    let rows = store.select_v(&query).map_err(|error| {
        format!(
            "create-only-outcome-unknown: proof read failed for entity '{}': {}",
            plans[0].identity, error
        )
    })?;
    let proven = rows.into_iter().collect::<BTreeSet<_>>();
    if plans.iter().all(|plan| proven.contains(&plan.subject)) {
        return Ok(());
    }

    // The atomic WHERE may have refused either an identity race or a
    // different-subject uniqueness race. Diagnose only after proof is absent;
    // every diagnostic read is fail-closed because success is no longer an
    // available outcome.
    let existing = existing_batch_subjects(store, plans).map_err(|error| {
        format!(
            "create-only-outcome-unknown: identity-conflict diagnosis failed for entity '{}': {}",
            error.identity, error.message
        )
    })?;
    if let Some(plan) = plans.iter().find(|plan| existing.contains(&plan.subject)) {
        return Err(format!(
            "entity '{}': already-exists: atomic create did not insert <{}> in <{}> (commit outcome proof absent; concurrent identity race)",
            plan.identity, plan.subject, plan.graph
        ));
    }

    let uniqueness_conflicts = prefetch_uniqueness_conflicts(
        store,
        plans,
        uniqueness_candidates,
    )
    .map_err(|error| {
        format!(
            "create-only-outcome-unknown: uniqueness-conflict diagnosis failed for entity '{}': {}",
            error.identity, error.message
        )
    })?;
    if let Some(index) = uniqueness_conflicts.into_iter().next() {
        let candidate = &uniqueness_candidates[index];
        let scope = match &candidate.partition {
            Some(partition) => format!("within '{}'", partition),
            None => format!("across all {}", candidate.req.kind),
        };
        return Err(format!(
            "entity '{}:{}': concurrent-uniqueness-conflict: duplicate '{}' {} appeared before the atomic create committed (fail-closed, from {})",
            candidate.req.kind,
            candidate.req.name,
            candidate.prop,
            scope,
            ONTOLOGY_GRAPH
        ));
    }

    let missing = plans
        .iter()
        .find(|plan| !proven.contains(&plan.subject))
        .expect("proof is incomplete");
    Err(format!(
        "create-only-outcome-unknown: entity '{}': conditional insert has no audit proof and no current identity/uniqueness conflict; refusing to report success",
        missing.identity
    ))
}

/// Commit prevalidated entity plans in exactly one Store::update. Witnessing is
/// deliberately owned by the caller after this returns: single-add preserves
/// its entity witness, while add-batch emits one aggregate rather than spawning
/// chorus-log once per member inside the request's latency.
fn commit_writes(
    store: &dyn Store,
    plans: &[PlannedWrite<'_>],
    uniqueness_candidates: &[UniquenessCandidate<'_>],
    id: &Identity,
    replace_existing: bool,
) -> R<Vec<String>> {
    const DCT: &str = "http://purl.org/dc/terms/";
    let created_by_subject = if replace_existing {
        existing_created(store, plans).map_err(|e| e.message)?
    } else {
        BTreeMap::new()
    };
    let now = if replace_existing { now_iso() } else { create_only_stamp() };
    let creator = id.role();
    let mut sparql = String::new();
    let mut inserts: BTreeMap<&str, String> = BTreeMap::new();

    for plan in plans {
        if replace_existing {
            sparql.push_str(&format!(
                "DELETE WHERE {{ GRAPH <{graph}> {{ <{subject}> ?p ?o }} }} ;\n",
                graph = plan.graph, subject = plan.subject
            ));
        }
        let label = if plan.req.fields.contains_key("label") {
            String::new()
        } else {
            format!("<{}> <{}label> \"{}\" .\n", plan.subject, NS, esc(&plan.req.name))
        };
        let created = created_by_subject.get(&plan.subject).unwrap_or(&now);
        let stamps = format!(
            "<{s}> <{d}created> \"{c}\" .\n<{s}> <{d}modified> \"{m}\" .\n<{s}> <{d}creator> \"{creator}\" .\n",
            s = plan.subject, d = DCT, c = esc(created), m = esc(&now), creator = esc(creator)
        );
        let body = inserts.entry(plan.graph.as_str()).or_default();
        body.push_str(&plan.turtle);
        body.push_str(&label);
        body.push_str(&stamps);
    }
    if replace_existing {
        sparql.push_str("INSERT DATA {\n");
    } else {
        sparql.push_str("INSERT {\n");
    }
    for (graph, body) in inserts {
        sparql.push_str(&format!("GRAPH <{}> {{ {} }}\n", graph, body));
    }
    if replace_existing {
        sparql.push('}');
    } else {
        let candidates = plans
            .iter()
            .map(|plan| format!("(<{}> <{}>)", plan.graph, plan.subject))
            .collect::<Vec<_>>()
            .join(" ");
        let uniqueness_guard = if uniqueness_candidates.is_empty() {
            String::new()
        } else {
            format!(
                "# athena-model atomic uniqueness guard\n\
                 FILTER NOT EXISTS {{ {} }}",
                uniqueness_conflict_pattern(uniqueness_candidates, None)
            )
        };
        sparql.push_str(&format!(
            "}}\nWHERE {{ \
               FILTER NOT EXISTS {{ \
                 VALUES (?existingGraph ?existingSubject) {{ {candidates} }} \
                 GRAPH ?existingGraph {{ ?existingSubject ?existingPredicate ?existingObject }} \
               }} \
               {uniqueness_guard} \
             }}",
            candidates = candidates,
            uniqueness_guard = uniqueness_guard,
        ));
    }
    store.update(&sparql)?;
    if !replace_existing {
        prove_create_only_commit(store, plans, uniqueness_candidates, creator, &now)?;
    }
    Ok(plans.iter().map(|p| p.subject.clone()).collect())
}

/// Full governed single write. This is a one-element call through the same
/// planner and committer as `add_batch`, preserving the existing public errors,
/// replace-subject semantics, and per-entity audit witness.
pub fn write(store: &dyn Store, req: &WriteReq, id: &Identity) -> R<String> {
    let (plans, uniqueness_candidates) =
        plan_writes(store, std::slice::from_ref(req)).map_err(|e| e.message)?;
    let subjects = commit_writes(store, &plans, &uniqueness_candidates, id, true)?;
    let subject = subjects.into_iter().next().expect("one request plans one subject");
    let fields = req.fields.len().to_string();
    let edges = req.edges.len().to_string();
    witness("model.write", &[
        ("kind", req.kind.as_str()), ("name", req.name.as_str()),
        ("iri", subject.as_str()), ("fields", fields.as_str()), ("edges", edges.as_str()),
    ]);
    Ok(subject)
}

/// Governed entity batch: validate the complete final state, authoritatively
/// prove every minted subject absent, then insert all subjects in one update.
/// This is deliberately NOT `seed_multi`: identities are minted from
/// `(kind,name)`, caller IRIs are impossible, add's label/audit semantics are
/// retained, and no migration provenance/content-hash is added.
pub fn add_batch(store: &dyn Store, reqs: &[WriteReq], id: &Identity) -> R<AddBatchReport> {
    if reqs.is_empty() {
        return Err("add-batch: entities must contain at least one entity".into());
    }
    let (plans, uniqueness_candidates) =
        plan_writes(store, reqs).map_err(WritePlanError::for_batch)?;
    let existing = existing_batch_subjects(store, &plans).map_err(WritePlanError::for_batch)?;
    if let Some(plan) = plans.iter().find(|plan| existing.contains(&plan.subject)) {
        return Err(WritePlanError::for_req(
            plan.req,
            format!(
                "already-exists: <{}> already exists in <{}> — add-batch is create-only",
                plan.subject, plan.graph
            ),
        )
        .for_batch());
    }
    let identities = plans.iter().map(|p| p.identity.as_str()).collect::<Vec<_>>().join(", ");
    // Create-only commit: never DELETE a subject. The model-authoritative read
    // above has proved every minted identity absent in its destination graph.
    let subjects = commit_writes(store, &plans, &uniqueness_candidates, id, false)
        .map_err(|e| format!("add-batch: commit failed for [{}]: {}", identities, e))?;
    let entities = subjects.len().to_string();
    let fields = reqs.iter().map(|req| req.fields.len()).sum::<usize>().to_string();
    let edges = reqs.iter().map(|req| req.edges.len()).sum::<usize>().to_string();
    let graphs = plans.iter().map(|plan| plan.graph.as_str()).collect::<BTreeSet<_>>().len().to_string();
    let uniqueness_checks = uniqueness_candidates.len().to_string();
    witness("model.add-batch", &[
        ("entities", entities.as_str()), ("fields", fields.as_str()),
        ("edges", edges.as_str()), ("graphs", graphs.as_str()),
        ("uniqueness_checks", uniqueness_checks.as_str()), ("creator", id.role()),
    ]);
    Ok(AddBatchReport { subjects })
}

/// An edge property local-name must be camelCase (ADR-040 Level 4) — the same law
/// to_turtle enforces on fields, applied to incremental edge ops.
fn check_property_local(prop: &str) -> R<()> {
    let ok = !prop.is_empty()
        && prop.chars().next().map(|c| c.is_ascii_lowercase()).unwrap_or(false)
        && prop.chars().all(|c| c.is_ascii_alphanumeric());
    if !ok {
        return Err(format!(
            "bad-property: '{}' — property local names are strict camelCase ASCII (ADR-040 Level 4)",
            prop
        ));
    }
    Ok(())
}

/// #3686 — SET one datatype field INCREMENTALLY (governed). The datatype-prop
/// sibling of link/unlink: touches ONLY the named predicate (DELETE that
/// predicate + INSERT the value, one tx) so re-sequencing a rich subject
/// ("security first") never wipes its other authored props — `add`'s
/// full-subject replace is the #3587 wipe class in miniature for this use.
///
/// Same gates as add (Silas's bless, 2026-07-25): identity (#3651, caller passes
/// the verified Identity), sh:datatype / sh:in from the shape, and the #3681
/// uniqueness ASK — which self-excludes, so re-setting the same value is not a
/// false dup. ONE designed difference from add: the uniqueWithin partition value
/// is read FROM THE STORE (the subject's existing partition edge) since a set
/// carries no edges; a subject missing its partition edge is refused rather than
/// checked unscoped. Datatype props only — edges stay link/unlink.
pub fn set_field(
    store: &dyn Store,
    kind: &str,
    name: &str,
    prop: &str,
    value: &str,
    graph: Option<&str>,
    _id: &Identity,
) -> R<String> {
    check_property_local(prop)?; // same camelCase law (ADR-040 Level 4) for field local-names
    let subject = mint(kind, name)?;
    if !store.ask(&format!("ASK {{ GRAPH ?g {{ <{}> ?p ?o }} }}", subject))? {
        witness("model.refused", &[("kind", kind), ("name", name), ("reason", "not-found"), ("field", prop)]);
        return Err(format!(
            "not-found: <{}> does not exist — set updates existing subjects only (create with add)",
            subject
        ));
    }
    let class = class_iri(kind)?;
    let shape = read_shape(store, &class)?;
    if let Some(dt) = shape.datatypes.get(prop) {
        if !datatype_ok(value, dt) {
            witness("model.refused", &[("kind", kind), ("name", name), ("reason", "shape-violation"), ("field", prop)]);
            return Err(format!("shape-violation: '{}' is not a valid xsd:{} for {} (sh:datatype)", value, dt, prop));
        }
    }
    if let Some(allowed) = shape.enums.get(prop) {
        if !allowed.iter().any(|a| a == value) {
            witness("model.refused", &[("kind", kind), ("name", name), ("reason", "shape-violation"), ("field", prop)]);
            return Err(format!("shape-violation: '{}' not in sh:in {:?} for {}", value, allowed, prop));
        }
    }
    let g = graph.unwrap_or(INSTANCES_GRAPH);
    // Uniqueness (#3681 idiom, self-excluding). Partition value comes from the
    // subject's OWN partition edge in the store — fail-closed if absent.
    if let Some(part) = shape.unique_within.get(prop) {
        let part_iri = store
            .select_v(&format!(
                "SELECT ?v WHERE {{ GRAPH <{g}> {{ <{s}> <{ns}{p}> ?t }} BIND(STR(?t) AS ?v) }}",
                g = g, s = subject, ns = NS, p = part
            ))?
            .into_iter()
            .next();
        let part_iri = match part_iri {
            Some(t) => t,
            None => {
                witness("model.refused", &[
                    ("kind", kind),
                    ("name", name),
                    ("reason", "missing-uniqueness-partition"),
                    ("field", prop),
                ]);
                return Err(format!(
                    "missing-partition: <{}> has no {} edge — cannot scope uniqueness for {} (set the {} edge first via link)",
                    subject, part, prop, part
                ));
            }
        };
        let dup = store.ask(&format!(
            "ASK {{ GRAPH <{g}> {{ ?other <{ns}{prop}> ?v ; <{ns}{part}> <{pi}> . FILTER(?other != <{s}> && STR(?v) = \"{val}\") }} }}",
            g = g, ns = NS, prop = prop, part = part, pi = part_iri, s = subject, val = esc(value)
        ))?;
        if dup {
            witness("model.refused", &[("kind", kind), ("name", name), ("reason", "uniqueness-violation"), ("field", prop)]);
            return Err(format!("shape-violation: duplicate '{}' within '{}' (chorus:uniqueWithin, from {})", prop, part, ONTOLOGY_GRAPH));
        }
    }
    if shape.unique_global.iter().any(|p| p == prop) {
        let dup = store.ask(&format!(
            "ASK {{ GRAPH <{g}> {{ ?other a <{cls}> ; <{ns}{prop}> ?v . FILTER(?other != <{s}> && STR(?v) = \"{val}\") }} }}",
            g = g, cls = class, ns = NS, prop = prop, s = subject, val = esc(value)
        ))?;
        if dup {
            witness("model.refused", &[("kind", kind), ("name", name), ("reason", "uniqueness-violation"), ("field", prop)]);
            return Err(format!("shape-violation: duplicate '{}' across all {} (chorus:uniqueGlobal, from {})", prop, kind, ONTOLOGY_GRAPH));
        }
    }
    // Single-predicate replace + the modified stamp — never `?p ?o` on the subject.
    const DCT: &str = "http://purl.org/dc/terms/";
    let now = now_iso();
    store.update(&format!(
        "DELETE WHERE {{ GRAPH <{g}> {{ <{s}> <{ns}{p}> ?o }} }} ;\nDELETE WHERE {{ GRAPH <{g}> {{ <{s}> <{d}modified> ?o }} }} ;\nINSERT DATA {{ GRAPH <{g}> {{ <{s}> <{ns}{p}> \"{v}\" . <{s}> <{d}modified> \"{m}\" }} }}",
        g = g, s = subject, ns = NS, p = prop, v = esc(value), d = DCT, m = esc(&now)
    ))?;
    witness("model.set", &[("kind", kind), ("name", name), ("iri", subject.as_str()), ("field", prop), ("value", value)]);
    Ok(subject)
}

/// #3468 — DELETE an entity wholesale (governed). Fail-closed: refuses a subject
/// that does not exist (so a typo can't be a silent no-op). Witnesses the delete.
/// athena-make's DELETE delegates here instead of a raw SPARQL DELETE — one governed
/// write path, audited, never silent.
pub fn delete_entity(store: &dyn Store, kind: &str, name: &str, graph: Option<&str>, _id: &Identity) -> R<String> {
    let subject = mint(kind, name)?;
    if !store.ask(&format!("ASK {{ GRAPH ?g {{ <{}> ?p ?o }} }}", subject))? {
        witness("model.refused", &[("kind", kind), ("name", name), ("reason", "not-found")]);
        return Err(format!("not-found: <{}> does not exist", subject));
    }
    // #3647 — delete from the class's declared home (or legacy default).
    let g = graph.unwrap_or(INSTANCES_GRAPH);
    assert_dal_writable(g)?; // #3356 AC4
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
/// (fail-closed). Witnesses the link. The governed replacement for athena-make's raw
/// build_edge_update.
pub fn add_edge(store: &dyn Store, kind: &str, name: &str, prop: &str, tkind: &str, tname: &str, graph: Option<&str>, _id: &Identity) -> R<String> {
    check_property_local(prop)?;
    let subject = mint(kind, name)?;
    let target = mint(tkind, tname)?;
    for iri in [&subject, &target] {
        if !store.ask(&format!("ASK {{ GRAPH ?g {{ <{}> ?p ?o }} }}", iri))? {
            witness("model.refused", &[("kind", kind), ("name", name), ("reason", "unknown-endpoint"), ("iri", iri.as_str())]);
            return Err(format!("unknown-endpoint: <{}> does not exist — referential integrity, fail-closed", iri));
        }
    }
    let g = graph.unwrap_or(INSTANCES_GRAPH); // #3647 — declared home or legacy default
    assert_dal_writable(g)?; // #3356 AC4
    store.update(&format!(
        "INSERT DATA {{ GRAPH <{g}> {{ <{s}> <{ns}{p}> <{t}> }} }}",
        g = g, ns = NS, s = subject, p = prop, t = target
    ))?;
    witness("model.link", &[("subject", subject.as_str()), ("prop", prop), ("target", target.as_str())]);
    Ok(subject)
}

/// #3468 — REMOVE one edge (governed). Single DELETE DATA, witnessed. Idempotent:
/// removing an absent edge is a no-op success (removal toward absence is safe).
/// The governed replacement for athena-make's raw edge-delete.
pub fn remove_edge(store: &dyn Store, kind: &str, name: &str, prop: &str, tkind: &str, tname: &str, graph: Option<&str>, _id: &Identity) -> R<String> {
    check_property_local(prop)?;
    let subject = mint(kind, name)?;
    let target = mint(tkind, tname)?;
    let g = graph.unwrap_or(INSTANCES_GRAPH); // #3647 — declared home or legacy default
    assert_dal_writable(g)?; // #3356 AC4
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

/// #3392 — literal check for riot-CANONICALIZED N-Triples input (seed's
/// contract; batch() keeps the stricter `is_literal_term` for hand-assembled
/// slots — different input contract, not a competing validator). NT guarantees
/// inner quotes arrive backslash-escaped, so `{ } ;` inside a properly-escaped
/// literal are inert in the door-assembled INSERT; refusing them refuses real
/// data (ICD mermaid diagrams, `{artist}/{album}` slug templates — the
/// "photograph" ruling extended). What still refuses, escape-aware:
///   - any UNESCAPED inner quote (the actual breakout vector)
///   - raw control chars (\n \r \t as characters — NT emits them escaped)
///   - a trailing dangling backslash
fn is_nt_literal(t: &str) -> bool {
    fn body_ok(inner: &str) -> bool {
        let mut esc = false;
        for c in inner.chars() {
            if matches!(c, '\n' | '\r' | '\t') {
                return false;
            }
            if esc {
                esc = false;
                continue;
            }
            match c {
                '\\' => esc = true,
                '"' => return false, // unescaped quote — breakout-shaped
                _ => {}
            }
        }
        !esc
    }
    fn quoted_ok(q: &str) -> bool {
        q.len() >= 2 && q.starts_with('"') && q.ends_with('"') && body_ok(&q[1..q.len() - 1])
    }
    if let Some(pos) = t.rfind("\"^^<") {
        let quoted = &t[..pos + 1];
        let datatype = &t[pos + 3..];
        return quoted_ok(quoted) && is_iri_term(datatype);
    }
    quoted_ok(t)
}

/// #3839 — a stable content hash for one subject's authored triples (FNV-1a
/// 64-bit, hex). Not cryptographic; it only has to change when the content does.
///
/// Used for idempotence: the deploy re-runs nightly over instances that have not
/// changed. Without this, every run DELETEs and re-INSERTs all 67 subjects and
/// bumps dcterms:modified on each — which destroys the only signal saying when a
/// thing actually changed. Comparing stored triples directly was the other
/// option and it is worse: through a single-variable SELECT the datatype falls
/// off a literal, so `"1"` and `"1"^^xsd:integer` compare equal and a real change
/// reads as unchanged. A hash of the exact terms we are about to write cannot
/// make that mistake.
pub fn content_hash(props: &[(String, String)]) -> String {
    let mut sorted: Vec<String> = props.iter().map(|(p, o)| format!("{} {}", p, o)).collect();
    sorted.sort();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in sorted.join("\n").bytes() {
        h ^= byte as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    format!("{:016x}", h)
}

/// #3839 — which graph does this class's shape say its instances live in?
///
/// WHY it is checked at the door: a shape with no `chorus:instancesGraph` pin
/// lets instances land in a default graph nothing reads. That is the exact
/// zero-rows failure #3581 / #3675 / #3838 each hit separately — the data was
/// written, the write succeeded, and the surface served nothing. A write that
/// lands somewhere unread is not a successful write, so the door refuses rather
/// than reporting success.
/// #4089 — where a manifest kind's rows LAND on `seed --deploy`: the shape's
/// declared instancesGraph when it has exactly one, else the caller's default.
/// (seed --deploy used to force every kind into urn:chorus:instances, so a
/// shape that declared a per-domain home — CommitmentShape → the services
/// graph — was written where nothing served it: the #3581 zero-rows class.)
/// Two pins is a modelling error and is refused, never averaged.
pub fn deploy_home(store: &dyn Store, kind: &str, default: &str) -> R<String> {
    let class = class_iri(kind)?;
    let pins = instances_graph_pins(store, &class)?;
    match pins.as_slice() {
        [] => Ok(default.to_string()),
        [one] => Ok(one.clone()),
        many => Err(format!(
            "seed --deploy: {} declares {} instancesGraph pins ({}) — one home per class",
            class, many.len(), many.join(", ")
        )),
    }
}

/// #4089 — group manifest entries by home graph, FIRST-SEEN ORDER preserved on
/// both axes: the graphs come out in the order the manifest first names them,
/// and each group keeps its entries in manifest order. Order is load-bearing —
/// a later group's edges may point at an earlier group's subjects (Card stubs
/// before Commitment rows), and the FK check asks the store across graphs.
pub fn deploy_partitions(homes: &[String]) -> Vec<(String, Vec<usize>)> {
    let mut out: Vec<(String, Vec<usize>)> = Vec::new();
    for (i, h) in homes.iter().enumerate() {
        match out.iter_mut().find(|(g, _)| g == h) {
            Some((_, idx)) => idx.push(i),
            None => out.push((h.clone(), vec![i])),
        }
    }
    out
}

fn instances_graph_pins(store: &dyn Store, class: &str) -> R<Vec<String>> {
    store.select_v(&format!(
        "PREFIX sh: <http://www.w3.org/ns/shacl#> PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <{g}> {{ ?s sh:targetClass <{c}> ; chorus:instancesGraph ?v }} }}",
        ns = NS, g = ONTOLOGY_GRAPH, c = class
    ))
}

/// #3839 — split an N-Triples literal into its lexical form and its carried
/// datatype local name (`"1"^^<...#integer>` → `("1", Some("integer"))`).
///
/// WHY this exists: the shape checks used `o.trim_matches('"')`, which on a
/// typed literal yields `1"^^<http://...#integer>` — the value plus its own
/// datatype tail. Every typed literal therefore failed its own sh:datatype
/// check, and every sh:in check compared against a string no enum could match.
/// It never surfaced because the only seeds run so far carried plain literals.
/// Found by routing the real deploy data through this door (#3839).
///
/// Same `rfind("\"^^<")` rule the NT-contract check uses (a literal's value
/// cannot contain an unescaped quote, so the LAST one is the terminator).
pub fn nt_literal_parts(o: &str) -> (&str, Option<&str>) {
    if let Some(pos) = o.rfind("\"^^<") {
        let lexical = &o[1..pos];
        let dt = o[pos + 4..].trim_end_matches('>');
        let local = dt.rsplit(['#', '/']).next().unwrap_or(dt);
        return (lexical, Some(local));
    }
    (o.trim_matches('"'), None)
}

/// Governed batch write — structural single-graph, typed-slot only, one transaction.
/// `deletes`: (s,p,o) patterns, o may be "?o" (delete all matching) → DELETE WHERE.
/// `inserts`: (s,p,o) ground triples → INSERT DATA. Returns count of triples touched.
pub fn batch(
    store: &dyn Store,
    graph: &str,
    deletes: &[(String, String, String)],
    inserts: &[(String, String, String)],
    _id: &Identity,
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
    assert_dal_writable(graph)?; // #3356 AC4 — batch cannot reach ontology/security either
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

// ── #3692 — the `seed` verb: bulk TTL ingest (5th DAL verb) ─────────────────
//
// The migration path (#3392 and every future TTL move): load a TTL whose IRIs
// were already minted, PRESERVING them. `add` mints an IRI from (kind,name);
// `seed` inverts that — the TTL is the authority on identity, the DAL is the
// authority on validity. Silas design (2026-07-25): IRI-guard per subject,
// SHACL via read_shape (REUSED, no parallel validator), fail-closed whole-batch,
// created-preserve + provenance stamp, idempotent, same identity + graph doors.

/// What seed() did — for the CLI line and the spine witness.
#[derive(Debug)]
pub struct SeedReport {
    pub subjects: usize,
    pub triples: usize,
}

/// Parse N-Triples text (the CLI feeds seed via `riot --output=ntriples`, so
/// prefixes/multi-line Turtle are already normalized away). Strict subset:
/// `<s> <p> <o|"literal"> .` per line; comments/blank lines skipped; blank
/// nodes REFUSED — a blank node has no preservable IRI, and preserving IRIs
/// is seed's whole contract.
pub fn parse_ntriples(nt: &str) -> R<Vec<(String, String, String)>> {
    let mut out = Vec::new();
    for (i, raw) in nt.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let body = line
            .strip_suffix('.')
            .ok_or_else(|| format!("nt-parse: line {} has no terminal '.'", i + 1))?
            .trim_end();
        if body.starts_with("_:") {
            return Err(format!("nt-parse: line {} has a blank-node subject — no preservable IRI (refused)", i + 1));
        }
        let s_end = body
            .find('>')
            .ok_or_else(|| format!("nt-parse: line {} subject is not an IRI", i + 1))?;
        let subject = &body[..=s_end];
        let rest = body[s_end + 1..].trim_start();
        if !rest.starts_with('<') {
            return Err(format!("nt-parse: line {} predicate is not an IRI", i + 1));
        }
        let p_end = rest
            .find('>')
            .ok_or_else(|| format!("nt-parse: line {} predicate is not an IRI", i + 1))?;
        let predicate = &rest[..=p_end];
        let object = rest[p_end + 1..].trim();
        if object.starts_with("_:") {
            return Err(format!("nt-parse: line {} has a blank-node object — no preservable IRI (refused)", i + 1));
        }
        if object.is_empty() {
            return Err(format!("nt-parse: line {} has no object", i + 1));
        }
        out.push((subject.to_string(), predicate.to_string(), object.to_string()));
    }
    Ok(out)
}

/// The IRI guard: a seeded subject must live in the chorus namespace and its
/// local name must satisfy the kind's minting convention (bare kinds: the bare
/// slug; prefixed kinds: `<kind>-slug`). The mint table stays the single
/// authority — a subject the mint could never produce is refused.
fn seed_iri_ok(kind: &str, subject_term: &str) -> R<String> {
    if !is_iri_term(subject_term) {
        return Err(format!("seed: iri-guard — '{}' is not a well-formed IRI term", subject_term));
    }
    let iri = &subject_term[1..subject_term.len() - 1];
    let local = iri
        .strip_prefix(NS)
        .ok_or_else(|| format!("seed: iri-guard — <{}> is outside the chorus namespace {}", iri, NS))?;
    let (_, _, bare) = KINDS
        .iter()
        .find(|(k, _, _)| *k == kind)
        .ok_or_else(|| format!("unknown-kind: '{}'", kind))?;
    let convention_ok = if *bare {
        !local.is_empty()
            && local.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    } else {
        local
            .strip_prefix(&format!("{}-", kind))
            .map(|rest| !rest.is_empty())
            .unwrap_or(false)
    };
    if !convention_ok {
        return Err(format!(
            "seed: iri-guard — <{}> does not match the '{}' kind convention (ADR-040 mint table)",
            iri, kind
        ));
    }
    Ok(iri.to_string())
}

/// Governed bulk ingest. Validates EVERY subject first (IRI guard, term shapes,
/// SHACL required/enums/datatypes via read_shape, referential integrity for
/// edges — batch-internal targets count), then issues ONE transaction:
/// per-subject DELETE WHERE + a single INSERT DATA carrying the original
/// triples verbatim plus the audit envelope (created preserved via the write()
/// pattern) and the provenance stamp. Any failure before that point leaves the
/// store untouched.
/// #3392 — the seed door's realm policy (Silas ruling 2026-07-25): a
/// KNOWN-REALMS ALLOWLIST, not chorus-only and not open.
///   - chorus realm: strict — KINDS-convention IRI check, rdf:type-match,
///     SHACL shape checks, referential integrity (unchanged from #3692).
///   - gathering realm: NS-membership + well-formed + no-injection ONLY.
///     ICD is GATHERING-owned (convergence boundary): it self-types via
///     icd:Domain and follows its own IRI scheme — a chorus kind/type check
///     would wrongly reject valid ICD. NS-set verified against the live
///     store 2026-07-25: instance subjects resolved against base
///     https://jeffbridwell.com/, class defs subject in urn:gathering:icd#.
///   - anything else: off-realm refusal (the #3573 door stays closed).
#[derive(Debug, Clone, Copy, PartialEq)]
enum RealmPolicy {
    ChorusStrict,
    ForeignNs(&'static [&'static str]),
}

const GATHERING_NS_SET: &[&str] = &["https://jeffbridwell.com/icd/", "urn:gathering:icd#"];

fn realm_policy(graph: &str) -> Option<RealmPolicy> {
    if graph.starts_with("urn:chorus:") {
        Some(RealmPolicy::ChorusStrict)
    } else if graph.starts_with("urn:gathering:") {
        Some(RealmPolicy::ForeignNs(GATHERING_NS_SET))
    } else {
        None
    }
}

/// One kind's worth of a seed batch: the class the caller states it is loading,
/// and that file's triples.
pub struct SeedGroup<'a> {
    pub kind: &'a str,
    pub triples: &'a [(String, String, String)],
}

pub fn seed(
    store: &dyn Store,
    kind: &str,
    triples: &[(String, String, String)],
    provenance: &str,
    graph: Option<&str>,
    id: &Identity,
) -> R<SeedReport> {
    seed_multi(store, &[SeedGroup { kind, triples }], provenance, graph, id)
}

/// #3839 — seed several kinds as ONE batch.
///
/// WHY this exists: the referential-integrity check is fail-closed against the
/// store OR the current batch. Two kinds that reference each other — a
/// ValueStream `contains` its steps, each step is `inStream` its stream — can
/// therefore never be loaded by two independent seed runs on an empty store:
/// whichever runs first has targets that exist in neither place, and refuses.
/// That is not a data problem, it is the door being unable to express "these
/// arrive together". Splitting the file would not have fixed it; it would have
/// moved the refusal.
///
/// What does NOT loosen: every subject is still validated against the shape of
/// the kind ITS OWN group declares, rdf:type still must agree with that kind, a
/// shape violation anywhere refuses the WHOLE batch before a single write. Only
/// the set of IRIs that count as "present" widens, to the union of the batch —
/// which is exactly what one transaction means.
pub fn seed_multi(
    store: &dyn Store,
    groups: &[SeedGroup<'_>],
    provenance: &str,
    graph: Option<&str>,
    id: &Identity,
) -> R<SeedReport> {
    // #3839 AC7 — where do these instances go?
    //
    // A caller that STATES a graph is honored: per-domain graphs are legitimate
    // (the ICD set seeds Domain individuals into urn:chorus:domains:icd), and
    // refusing an explicit target against the shape's pin would be the door
    // second-guessing an instruction it was given.
    //
    // A caller that states NOTHING is the failure this guards. It used to fall
    // back to a hardcoded urn:chorus:instances — write succeeds, surface serves
    // nothing, and the write's own result cannot tell you (the #3581 / #3675 /
    // #3838 zero-rows class, three separate hits). Now the SHAPE decides, and a
    // class whose shape declares no pin is refused rather than guessed at.
    let resolved: String;
    let g: &str = match graph {
        Some(explicit) => explicit,
        None => {
            let mut pins: Vec<String> = Vec::new();
            for gr in groups {
                let class = class_iri(gr.kind)?;
                let p = instances_graph_pins(store, &class)?;
                if p.is_empty() {
                    witness("model.seed.refused", &[("class", class.as_str()), ("reason", "no-instances-graph-pin")]);
                    return Err(format!(
                        "seed: no --graph given and the shape for <{}> declares no chorus:instancesGraph — refusing to guess a default (the zero-rows class: written, and read by nothing)",
                        class
                    ));
                }
                for v in p {
                    if !pins.contains(&v) {
                        pins.push(v);
                    }
                }
            }
            if pins.len() != 1 {
                return Err(format!(
                    "seed: no --graph given and the batch's shapes pin different instance graphs {:?} — state the target explicitly",
                    pins
                ));
            }
            resolved = pins.remove(0);
            &resolved
        }
    };
    if g.contains(['<', '>', '{', '}', ' ', ';']) {
        witness("model.seed.refused", &[("graph", g), ("reason", "malformed-graph")]);
        return Err(format!("seed: graph '{}' is malformed (refused)", g));
    }
    let policy = realm_policy(g).ok_or_else(|| {
        witness("model.seed.refused", &[("graph", g), ("reason", "off-realm-graph")]);
        format!("seed: graph '{}' is outside the known realms (urn:chorus:*, urn:gathering:*) — refused", g)
    })?;
    assert_dal_writable(g)?; // #3356 AC4 — ontology/security are DBA-path-only
    if groups.is_empty() || groups.iter().all(|gr| gr.triples.is_empty()) {
        return Err("seed: no triples to load".into());
    }

    // Group triples by subject, per group, preserving first-seen order.
    let rdf_type = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let mut grouped: Vec<(&str, Vec<String>, std::collections::HashMap<String, Vec<(String, String)>>)> =
        Vec::new();
    for gr in groups {
        let mut order: Vec<String> = Vec::new();
        let mut by_subject: std::collections::HashMap<String, Vec<(String, String)>> =
            std::collections::HashMap::new();
        for (s, p, o) in gr.triples {
            // Object check is the NT-contract one (escape-aware) — seed's input is
            // riot-canonicalized N-Triples, where real content legitimately carries
            // { } ; and escaped quotes (mermaid, slug templates). See is_nt_literal.
            if !subj_pred_ok(s) || !subj_pred_ok(p) || !(is_iri_term(o) || is_nt_literal(o)) {
                witness("model.seed.refused", &[("reason", "bad-slot")]);
                return Err("seed: a triple has an invalid/injection-shaped slot".into());
            }
            if !by_subject.contains_key(s) {
                order.push(s.clone());
            }
            by_subject.entry(s.clone()).or_default().push((p.clone(), o.clone()));
        }
        grouped.push((gr.kind, order, by_subject));
    }

    // A subject may be claimed by exactly ONE kind in the batch. Two groups
    // claiming the same IRI would each validate it against a different shape and
    // then race in the write — refuse rather than pick a winner.
    {
        let mut claimed: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
        for (kind, order, _) in &grouped {
            for subject_term in order {
                if let Some(prev) = claimed.insert(subject_term.as_str(), kind) {
                    if prev != *kind {
                        witness("model.seed.refused", &[("iri", subject_term.as_str()), ("reason", "subject-claimed-twice")]);
                        return Err(format!(
                            "seed: subject {} is claimed by two kinds in one batch ('{}' and '{}') — refused",
                            subject_term, prev, kind
                        ));
                    }
                }
            }
        }
    }

    // ── Validate ALL subjects of ALL groups first — nothing written on error ──
    // Chorus realm: the full #3692 battery. Gathering realm: NS-membership +
    // well-formed + no-injection ONLY (Silas ruling — their vocab, not ours).
    // The in-batch IRI set is the UNION across groups (#3839).
    let batch_iris: std::collections::HashSet<String> = grouped
        .iter()
        .flat_map(|(_, order, _)| order.iter())
        .map(|s| s[1..s.len() - 1].to_string())
        .collect();

    let mut shapes: Vec<Option<(String, ShapeReq)>> = Vec::new();
    for (kind, order, by_subject) in &grouped {
        if let RealmPolicy::ForeignNs(ns_set) = policy {
            for subject_term in order {
                let iri = &subject_term[1..subject_term.len() - 1];
                if !ns_set.iter().any(|ns| iri.starts_with(ns)) {
                    witness("model.seed.refused", &[("iri", iri), ("reason", "iri-off-realm-ns")]);
                    return Err(format!(
                        "seed: iri-guard — <{}> is outside the realm's namespace set {:?} (refused)",
                        iri, ns_set
                    ));
                }
            }
        }

        let class_and_shape = if policy == RealmPolicy::ChorusStrict {
            let class = class_iri(kind)?;
            let shape = read_shape(store, &class)?;
            Some((class, shape))
        } else {
            None
        };

        if let Some((class, shape)) = &class_and_shape {
            let class = class.as_str();
            for subject_term in order {
                let iri = seed_iri_ok(kind, subject_term)?;
                let props = &by_subject[subject_term];

                // rdf:type, when carried, must INCLUDE the seed kind's class —
                // a mixed-kind TTL is two seed GROUPS, not one blind one.
                //
                // #3839 — this used to demand every carried type equal the kind's
                // class exactly, which refused legitimate dual typing: #3838's
                // roles are `chorus:Role, chorus:AgentRole` on purpose, because
                // the store does no inference and a role typed only AgentRole
                // would vanish from every Role query. Exact-equality made the
                // model's own convention unloadable through its own door.
                //
                // The rule that keeps the teeth: the kind's class must be present,
                // and any ADDITIONAL type must be a subclass of it (asked of the
                // ontology, fail-closed). ValueStreamStep is not a subclass of
                // ValueStream, so a mixed file is still refused.
                let class_term = format!("<{}>", class);
                let carried: Vec<&String> = props
                    .iter()
                    .filter(|(p, _)| p == rdf_type)
                    .map(|(_, o)| o)
                    .collect();
                if !carried.is_empty() {
                    if !carried.iter().any(|o| **o == class_term) {
                        witness("model.seed.refused", &[("iri", iri.as_str()), ("reason", "type-missing")]);
                        return Err(format!(
                            "shape-violation: <{}> declares type(s) {} but seed kind '{}' requires <{}> among them",
                            iri,
                            carried.iter().map(|o| o.as_str()).collect::<Vec<_>>().join(", "),
                            kind, class
                        ));
                    }
                    for o in &carried {
                        if **o == class_term {
                            continue;
                        }
                        let sub = o.trim_start_matches('<').trim_end_matches('>');
                        let is_sub = store.ask(&format!(
                            "ASK {{ GRAPH ?g {{ <{}> <http://www.w3.org/2000/01/rdf-schema#subClassOf>* <{}> }} }}",
                            sub, class
                        ))?;
                        if !is_sub {
                            witness("model.seed.refused", &[("iri", iri.as_str()), ("reason", "type-mismatch")]);
                            return Err(format!(
                                "shape-violation: <{}> also declares type {}, which is not a subclass of the seed kind '{}' (<{}>)",
                                iri, o, kind, class
                            ));
                        }
                    }
                }

                // Field view of the subject's NS-local properties.
                let field_of = |p: &str| -> Option<String> {
                    p.strip_prefix(&format!("<{}", NS))
                        .and_then(|r| r.strip_suffix('>'))
                        .map(|s| s.to_string())
                };
                let has_prop = |name: &str| props.iter().any(|(p, _)| field_of(p).as_deref() == Some(name));

                for need in &shape.required {
                    if !has_prop(need) && need != "label" {
                        witness("model.refused", &[("kind", kind), ("name", iri.as_str()), ("reason", "shape-violation"), ("field", need)]);
                        return Err(format!(
                            "shape-violation: {} requires '{}' (sh:minCount 1, from {}) — subject <{}>",
                            class, need, ONTOLOGY_GRAPH, iri
                        ));
                    }
                }
                for (p, o) in props {
                    let Some(local) = field_of(p) else { continue };
                    if o.starts_with('"') {
                        let (val, carried_dt) = nt_literal_parts(o);
                        // A literal that names its OWN datatype must agree with
                        // the shape's. Silently revalidating "1"^^xsd:string as
                        // an integer would be the door approving a lie.
                        if let (Some(carried), Some(declared)) = (carried_dt, shape.datatypes.get(&local)) {
                            if carried != declared {
                                witness("model.refused", &[("kind", kind), ("name", iri.as_str()), ("reason", "datatype-mismatch"), ("field", local.as_str())]);
                                return Err(format!(
                                    "shape-violation: '{}' carries xsd:{} but {} declares sh:datatype xsd:{} for '{}'",
                                    val, carried, class, declared, local
                                ));
                            }
                        }
                        if let Some(allowed) = shape.enums.get(&local) {
                            if !allowed.iter().any(|a| a == val) {
                                return Err(format!("shape-violation: '{}' not in sh:in {:?} for {}", val, allowed, local));
                            }
                        }
                        if let Some(dt) = shape.datatypes.get(&local) {
                            if !datatype_ok(val, dt) {
                                witness("model.refused", &[("kind", kind), ("name", iri.as_str()), ("reason", "shape-violation"), ("field", local.as_str())]);
                                return Err(format!("shape-violation: '{}' is not a valid xsd:{} for '{}'", val, dt, local));
                            }
                        }
                    } else if is_iri_term(o) && p != rdf_type {
                        // Edge — referential integrity, fail-closed. A target
                        // anywhere in THIS batch counts (#3839: mutually
                        // referential kinds arrive in one transaction).
                        let target = &o[1..o.len() - 1];
                        if !batch_iris.contains(target) {
                            let exists = store.ask(&format!("ASK {{ GRAPH ?g {{ <{}> ?p ?o }} }}", target))?;
                            if !exists {
                                witness("model.refused", &[("kind", kind), ("name", iri.as_str()), ("reason", "unknown-target"), ("edge", local.as_str())]);
                                return Err(format!(
                                    "unknown-target: {} → <{}> exists neither in the store nor in this batch (referential integrity, fail-closed)",
                                    local, target
                                ));
                            }
                        }
                    }
                }
            }
        }
        shapes.push(class_and_shape);
    }

    // ── Idempotence (AC4): read the stored content hashes in ONE query ──────
    // A subject whose authored content is byte-identical to what is already in
    // the graph is skipped entirely — no DELETE, no INSERT, no modified bump.
    // Both sides of the comparison are plain strings, so nothing can be lost in
    // transit the way a literal's datatype is.
    let mut stored_hash: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for row in store.select_v(&format!(
        "SELECT ?v WHERE {{ GRAPH <{g}> {{ ?s <{ns}contentHash> ?h }} BIND(CONCAT(STR(?s), '|', STR(?h)) AS ?v) }}",
        g = g, ns = NS
    ))? {
        if let Some((subj, h)) = row.split_once('|') {
            stored_hash.insert(subj.to_string(), h.to_string());
        }
    }

    // ── Assemble the single transaction, across every group ─────────────────
    const DCT: &str = "http://purl.org/dc/terms/";
    let now = now_iso();
    let creator = id.role().to_string();
    let mut sparql = String::new();
    let mut body = String::new();
    let mut triple_count = 0usize;
    let mut subject_count = 0usize;
    let mut unchanged = 0usize;

    for ((kind, order, by_subject), class_and_shape) in grouped.iter().zip(shapes.iter()) {
        let mut unchanged_in_group = 0usize;
        for subject_term in order {
            let iri = &subject_term[1..subject_term.len() - 1];
            let props = &by_subject[subject_term];
            let hash = content_hash(props);
            if stored_hash.get(iri).map(|h| h == &hash).unwrap_or(false) {
                unchanged += 1;
                unchanged_in_group += 1;
                continue; // AC4 — identical content is a no-op, not a rewrite
            }
            sparql.push_str(&format!(
                "DELETE WHERE {{ GRAPH <{g}> {{ <{s}> ?p ?o }} }} ;\n",
                g = g, s = iri
            ));
            let mut has_label = false;
            let mut has_type = false;
            for (p, o) in props {
                if p == rdf_type { has_type = true; }
                if p == &format!("<{}label>", NS) { has_label = true; }
                body.push_str(&format!("{} {} {} . ", subject_term, p, o));
                triple_count += 1;
            }
            // Autofill (type + label) is chorus-vocab — chorus realm only. A
            // foreign-realm subject keeps exactly its own triples (its vocab,
            // not ours) plus the audit/provenance envelope below.
            if let Some((class, _)) = class_and_shape {
                if !has_type {
                    body.push_str(&format!("{} a <{}> . ", subject_term, class));
                }
                if !has_label {
                    let local = iri.strip_prefix(NS).unwrap_or(iri);
                    body.push_str(&format!("{} <{}label> \"{}\" . ", subject_term, NS, esc(local)));
                }
            } else {
                let _ = (has_type, has_label);
            }
            // created preserved (write() pattern), modified bumped, creator = the
            // verified identity, provenance = the migration stamp.
            let existing_created = store
                .select_v(&format!(
                    "SELECT ?v WHERE {{ GRAPH <{g}> {{ <{s}> <{d}created> ?v }} }}",
                    g = g, s = iri, d = DCT
                ))?
                .into_iter()
                .next();
            let created = existing_created.unwrap_or_else(|| now.clone());
            body.push_str(&format!(
                "{st} <{d}created> \"{c}\" . {st} <{d}modified> \"{m}\" . {st} <{d}creator> \"{cr}\" . {st} <{ns}provenance> \"{pv}\" . {st} <{ns}contentHash> \"{ch}\" . ",
                st = subject_term, d = DCT, c = esc(&created), m = esc(&now),
                cr = esc(&creator), ns = NS, pv = esc(provenance), ch = hash
            ));
        }
        let (ns_, no_) = (order.len().to_string(), kind.to_string());
        witness("model.seed", &[("kind", no_.as_str()), ("graph", g), ("subjects", ns_.as_str()), ("provenance", provenance)]);
        subject_count += order.len() - unchanged_in_group;
    }
    if body.is_empty() {
        // Everything was already exactly this. Say so — a deploy that reports
        // "wrote 67" every night when it changed nothing is a broken signal.
        witness("model.seed.unchanged", &[("graph", g), ("subjects", unchanged.to_string().as_str())]);
        return Ok(SeedReport { subjects: 0, triples: 0 });
    }
    sparql.push_str(&format!("INSERT DATA {{ GRAPH <{g}> {{ {b} }} }}", g = g, b = body));
    store.update(&sparql)?;

    Ok(SeedReport { subjects: subject_count, triples: triple_count })
}

/// #3392 — governed by-IRI delete (Silas ruling 2026-07-25): the migration-
/// cleanup path for a live-only subject in a foreign-realm graph, which has
/// no chorus (kind,name) for delete_entity to address. Same doors as seed:
/// realm-policy allowlist, dba-graph refusal, verified Identity. Deletes the
/// subject's triples AND inbound references in ONE transaction, witnessed as
/// model.deleted — auditable/reversible from the spine.
pub fn delete_iri(store: &dyn Store, iri: &str, graph: &str, id: &Identity) -> R<String> {
    let policy = realm_policy(graph).ok_or_else(|| {
        witness("model.delete.refused", &[("graph", graph), ("reason", "off-realm-graph")]);
        format!("delete-iri: graph '{}' is outside the known realms — refused", graph)
    })?;
    assert_dal_writable(graph)?; // ontology/security stay DBA-path-only
    let term = format!("<{}>", iri);
    if !is_iri_term(&term) {
        return Err(format!("delete-iri: iri-guard — '{}' is not a well-formed IRI", iri));
    }
    let ns_ok = match policy {
        RealmPolicy::ChorusStrict => iri.starts_with(NS),
        RealmPolicy::ForeignNs(ns_set) => ns_set.iter().any(|ns| iri.starts_with(ns)),
    };
    if !ns_ok {
        witness("model.delete.refused", &[("iri", iri), ("reason", "iri-off-realm-ns")]);
        return Err(format!("delete-iri: iri-guard — <{}> is outside the realm's namespace set", iri));
    }
    store.update(&format!(
        "DELETE WHERE {{ GRAPH <{g}> {{ <{s}> ?p ?o }} }} ;\nDELETE WHERE {{ GRAPH <{g}> {{ ?s ?p2 <{s}> }} }}",
        g = graph, s = iri
    ))?;
    witness("model.deleted", &[("iri", iri), ("graph", graph), ("creator", id.role())]);
    Ok(iri.to_string())
}

#[cfg(test)]
mod tests {
    // ── #3680 — Test as a bare-grain reference kind ──
    #[test]
    fn normal_bare_mints_unaffected_by_guard_scoping() {
        // Silas's belt-and-suspenders (bless note, 2026-07-24): the guard
        // re-scoping must not shift NORMAL bare-grain behavior.
        assert_eq!(super::mint("product", "chorus").unwrap(), format!("{}chorus", super::NS));
        assert_eq!(super::mint("domain", "photos").unwrap(), format!("{}photos", super::NS));
        // and the newly-legitimate kind-word-prefixed bare name mints bare, no doubling
        assert_eq!(super::mint("product", "product-x").unwrap(), format!("{}product-x", super::NS));
    }

    #[test]
    fn mint_test_kind_reproduces_crawler_bare_iri() {
        // the crawler minted NS#test-platform-api-... (bare, pre-slugged names);
        // edge resolution must produce the IDENTICAL IRI or referential
        // integrity can never pass for ofTest.
        let iri = super::mint("test", "test-platform-api-tests-access-log-test-ts-x").unwrap();
        assert_eq!(iri, format!("{}test-platform-api-tests-access-log-test-ts-x", super::NS));
    }

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
        // #4089 — commitment rows seed through the manifest, so the kind must mint
        assert_eq!(
            mint("commitment", "ledger-cross-foots").unwrap(),
            format!("{}commitment-ledger-cross-foots", NS)
        );
        assert_eq!(mint("principle", "be direct").unwrap(), format!("{}principle-be-direct", NS));
        // #4040 — the pipelines domain's kinds (the fifth generate-vs-write drift)
        assert_eq!(mint("pipeline", "cicd").unwrap(), format!("{}pipeline-cicd", NS));
        assert_eq!(
            mint("pipeline-step", "cicd-demo").unwrap(),
            format!("{}pipeline-step-cicd-demo", NS)
        );
        // #4047 — run rows mint too; without this the nightly emit 502'd nightly
        assert_eq!(
            mint("pipeline-run", "nightly-2026-09-01").unwrap(),
            format!("{}pipeline-run-nightly-2026-09-01", NS)
        );
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

    // ── #3773 — the pen accepts every class athena-make serves ──────────────────
    //
    // Red first: before the rows were added, each of these returned
    // "unknown-kind" and the whole security domain was unwritable while being
    // fully readable. The failure was silent in the worst way — GET /principals
    // answered 200 with an empty list, which reads as "there are none" rather
    // than "you cannot write these."
    #[test]
    fn security_domain_kinds_are_writable() {
        for kind in [
            "principal",
            "credential",
            "permission",
            "api-surface",
            "auth-boundary",
            "key-registry-entry",
            "security-probe",
        ] {
            let iri = mint(kind, "fixture").unwrap_or_else(|e| {
                panic!("the governed writer refuses '{kind}': {e} — a class athena-make serves must be writable, or the domain can never conform to its own model")
            });
            assert!(iri.ends_with(&format!("{kind}-fixture")), "{kind} mints type-prefixed: {iri}");
        }
    }

    #[test]
    fn model_infrastructure_kinds_are_writable() {
        for kind in ["emit-contract", "metric", "property", "property-key"] {
            mint(kind, "fixture")
                .unwrap_or_else(|e| panic!("the governed writer refuses '{kind}': {e}"));
        }
    }

    // NEGATIVE PROOF that the two tests above can actually fail: a kind that is
    // NOT in the table must still be refused. Without this, a change that made
    // mint() accept anything would turn both tests green while destroying the
    // guarantee they exist to hold — the pen's whole job is refusing what the
    // model does not declare.
    #[test]
    fn a_kind_absent_from_the_table_is_still_refused() {
        let e = mint("principal-impostor", "fixture").unwrap_err();
        assert!(
            e.starts_with("unknown-kind"),
            "adding the security kinds must not have opened the pen to everything: {e}"
        );
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
    fn create_only_proof_stamps_are_unique_and_datetime_compatible() {
        let stamps = (0..128).map(|_| create_only_stamp()).collect::<BTreeSet<_>>();
        assert_eq!(stamps.len(), 128, "the process-local sequence prevents same-tick collisions");
        let encoded_pid = format!("{:010}", std::process::id());
        for stamp in stamps {
            if stamp.starts_with("epoch:") {
                continue; // fail-soft branch when the host lacks `date`
            }
            let fraction = stamp
                .strip_suffix('Z')
                .and_then(|value| value.rsplit_once('.').map(|(_, fraction)| fraction))
                .expect("create-only stamp is an RFC3339/xsd:dateTime value");
            assert_eq!(fraction.len(), 39, "nanos + PID + u64 sequence are fixed width");
            assert!(fraction.chars().all(|c| c.is_ascii_digit()));
            assert_eq!(&fraction[9..19], encoded_pid, "the process identity is encoded");
        }
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
        unique_within: Vec<String>,
        update_error: Option<String>,
        proof_missing: bool,
        pub updates: std::cell::RefCell<Vec<String>>,
    }
    impl Store for StubStore {
        fn ask(&self, sparql: &str) -> R<bool> {
            Ok(self.existing.iter().any(|e| sparql.contains(e.as_str())))
        }
        fn select_v(&self, sparql: &str) -> R<Vec<String>> {
            if sparql.contains("# athena-model create-only outcome proof") {
                if self.proof_missing {
                    return Ok(vec![]);
                }
                let block = sparql
                    .split_once("VALUES (?candidateGraph ?candidate) {")
                    .and_then(|(_, tail)| tail.split_once('}'))
                    .map(|(values, _)| values)
                    .unwrap_or("");
                Ok(block
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .chunks(2)
                    .filter_map(|pair| pair.get(1))
                    .map(|term| {
                        term.trim_matches(|c| matches!(c, '<' | '>' | '(' | ')'))
                            .to_string()
                    })
                    .collect())
            } else if sparql.contains("VALUES ?target") {
                Ok(self
                    .existing
                    .iter()
                    .filter(|iri| sparql.contains(iri.as_str()))
                    .map(|iri| format!("{}|", iri))
                    .collect())
            } else if sparql.contains("sh:minCount") {
                Ok(self.required.clone())
            } else if sparql.contains("uniqueWithin") {
                Ok(self.unique_within.clone())
            } else {
                Ok(vec![])
            }
        }
        fn update(&self, sparql: &str) -> R<()> {
            self.updates.borrow_mut().push(sparql.to_string());
            match &self.update_error {
                Some(error) => Err(error.clone()),
                None => Ok(()),
            }
        }
    }

    /// #3651 — an in-crate test identity (the private constructor is crate-visible;
    /// out-of-crate callers can only get an Identity through verify_identity).
    fn tid() -> Identity { Identity("kade".into()) }

    fn stub(existing: &[&str], required: &[&str]) -> StubStore {
        StubStore {
            existing: existing.iter().map(|s| s.to_string()).collect(),
            required: required.iter().map(|s| s.to_string()).collect(),
            unique_within: Vec::new(),
            update_error: None,
            proof_missing: false,
            updates: Default::default(),
        }
    }

    fn take_test_witnesses() -> Vec<String> {
        TEST_WITNESSES.with(|events| std::mem::take(&mut *events.borrow_mut()))
    }

    #[test]
    fn large_add_batch_emits_one_aggregate_witness_after_commit() {
        let _ = take_test_witnesses();
        let store = stub(&[], &[]);
        let reqs = (0..2_000)
            .map(|index| WriteReq {
                kind: "role".into(),
                name: format!("batch-member-{index}"),
                ..Default::default()
            })
            .collect::<Vec<_>>();

        let report = add_batch(&store, &reqs, &tid()).expect("large create-only batch commits");

        assert_eq!(report.subjects.len(), 2_000);
        assert_eq!(store.updates.borrow().len(), 1);
        assert_eq!(
            take_test_witnesses(),
            vec!["model.add-batch"],
            "one witness invocation equals one chorus-log subprocess in production",
        );
    }

    #[test]
    fn single_write_refuses_missing_uniqueness_partition_without_update_or_success_witness() {
        let _ = take_test_witnesses();
        let mut store = stub(&[], &[]);
        store.unique_within = vec!["rank|inGroup".into()];
        let mut req = WriteReq { kind: "domain".into(), name: "single".into(), ..Default::default() };
        req.fields.insert("rank".into(), "1".into());

        let err = write(&store, &req, &tid())
            .expect_err("uniqueWithin without its partition edge must fail closed");

        assert!(err.contains("uniqueWithin") && err.contains("inGroup") && err.contains("no 'inGroup' edge"), "{err}");
        assert_eq!(take_test_witnesses(), vec!["model.refused"]);
        assert!(store.updates.borrow().is_empty(), "refusal happens before any store update");
    }

    #[test]
    fn add_batch_emits_no_success_witness_when_commit_fails() {
        let _ = take_test_witnesses();
        let mut store = stub(&[], &[]);
        store.update_error = Some("fuseki-update-failed".into());

        let err = add_batch(&store, &[WriteReq {
            kind: "role".into(),
            name: "never-committed".into(),
            ..Default::default()
        }], &tid()).expect_err("failed update cannot produce a success witness");

        assert!(err.contains("fuseki-update-failed"), "store cause survives: {err}");
        assert!(take_test_witnesses().is_empty(), "success witness is strictly post-commit");
    }

    #[test]
    fn add_batch_emits_no_success_witness_without_commit_outcome_proof() {
        let _ = take_test_witnesses();
        let mut store = stub(&[], &[]);
        store.proof_missing = true;

        let err = add_batch(&store, &[WriteReq {
            kind: "role".into(),
            name: "raced".into(),
            ..Default::default()
        }], &tid()).expect_err("conditional no-op cannot produce a success witness");

        assert!(err.contains("create-only-outcome-unknown") && err.contains("role:raced"), "{err}");
        assert!(take_test_witnesses().is_empty(), "success witness requires outcome proof");
    }

    // ── #3573 batch-op security guards (the door's reason to exist) ──
    fn t(s: &str, p: &str, o: &str) -> (String, String, String) { (s.into(), p.into(), o.into()) }

    #[test]
    fn batch_refuses_empty_graph_no_default_ever() {
        let store = stub(&[], &[]);
        let ins = vec![t("<urn:chorus:x>", "<urn:chorus:p>", "<urn:chorus:o>")];
        assert!(batch(&store, "", &[], &ins, &tid()).is_err(), "empty graph must refuse");
        assert!(store.updates.borrow().is_empty(), "nothing written on empty-graph refusal");
    }

    #[test]
    fn batch_refuses_off_realm_graph() {
        let store = stub(&[], &[]);
        let ins = vec![t("<urn:chorus:x>", "<urn:chorus:p>", "<urn:chorus:o>")];
        assert!(batch(&store, "urn:gathering:photos", &[], &ins, &tid()).is_err(), "off-realm graph must refuse");
        assert!(store.updates.borrow().is_empty());
    }

    #[test]
    fn batch_refuses_injection_shaped_slots_and_writes_nothing() {
        let store = stub(&[], &[]);
        // object tries to break out into another GRAPH via ; INSERT ... GRAPH <other>
        let evil_o = vec![t("<urn:chorus:x>", "<urn:chorus:p>",
            "<urn:chorus:o> } } ; INSERT DATA { GRAPH <urn:gathering:x> { <a> <b> <c> } } #")];
        assert!(batch(&store, "urn:chorus:instances", &[], &evil_o, &tid()).is_err());
        // predicate is the bare GRAPH keyword (not an IRI)
        let evil_p = vec![t("<urn:chorus:y>", "GRAPH", "?o")];
        assert!(batch(&store, "urn:chorus:instances", &evil_p, &[], &tid()).is_err());
        // a raw variable object (not the single allowed ?o wildcard)
        let evil_v = vec![t("<urn:chorus:z>", "<urn:chorus:p>", "?anything")];
        assert!(batch(&store, "urn:chorus:instances", &[], &evil_v, &tid()).is_err());
        assert!(store.updates.borrow().is_empty(), "no injection-shaped batch may write");
    }

    #[test]
    fn batch_accepts_valid_and_is_single_graph_scoped() {
        let store = stub(&[], &[]);
        let dels = vec![t("<urn:chorus:file/a>", "<https://jeffbridwell.com/chorus#fileInDomain>", "?o")];
        let ins = vec![t("<urn:chorus:file/a>", "<https://jeffbridwell.com/chorus#fileInDomain>", "<urn:chorus:domain/x>")];
        let n = batch(&store, "urn:chorus:instances", &dels, &ins, &tid()).unwrap();
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
        let n = batch(&store, "urn:chorus:instances", &[], &ins, &tid()).unwrap();
        assert_eq!(n, 1, "typed integer literal must pass (riot's SHACL form)");
        assert!(store.updates.borrow()[0].contains("^^<http://www.w3.org/2001/XMLSchema#integer>"));
    }

    #[test]
    fn batch_refuses_typed_literal_with_bad_datatype_or_injected_value() {
        let store = stub(&[], &[]);
        // datatype not an IRI
        let bad_dt = vec![t("<urn:chorus:s>", "<urn:chorus:p>", "\"1\"^^not-an-iri")];
        assert!(batch(&store, "urn:chorus:instances", &[], &bad_dt, &tid()).is_err());
        // datatype IRI with injection chars
        let evil_dt = vec![t("<urn:chorus:s>", "<urn:chorus:p>", "\"1\"^^<urn:x> } ; INSERT")];
        assert!(batch(&store, "urn:chorus:instances", &[], &evil_dt, &tid()).is_err());
        // injection inside the value of a typed literal
        let evil_val = vec![t("<urn:chorus:s>", "<urn:chorus:p>", "\"1} ; DROP\"^^<urn:x>")];
        assert!(batch(&store, "urn:chorus:instances", &[], &evil_val, &tid()).is_err());
        assert!(store.updates.borrow().is_empty(), "no typed-literal injection may write");
    }

    #[test]
    fn batch_still_accepts_plain_literal_including_carets_inside() {
        let store = stub(&[], &[]);
        let ins = vec![
            t("<urn:chorus:s>", "<urn:chorus:p>", "\"plain value\""),
            t("<urn:chorus:s>", "<urn:chorus:p>", "\"a^^b inside quotes is content\""),
        ];
        assert_eq!(batch(&store, "urn:chorus:instances", &[], &ins, &tid()).unwrap(), 2);
    }

    #[test]
    fn batch_language_tag_decision_rejected_until_needed() {
        // #3622 AC note: "x"@en is NOT accepted yet — no writer emits it; widen
        // deliberately when one does, don't pre-open the parser surface.
        let store = stub(&[], &[]);
        let ins = vec![t("<urn:chorus:s>", "<urn:chorus:p>", "\"x\"@en")];
        assert!(batch(&store, "urn:chorus:instances", &[], &ins, &tid()).is_err());
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
        let n = batch(&store, "urn:chorus:instances", &[], &ins, &tid()).unwrap();
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
        let e = write(&store, &req, &tid()).unwrap_err();
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
        let subj = write(&store, &req, &tid()).unwrap();
        assert_eq!(subj, format!("{}tests", NS));
        let ups = store.updates.borrow();
        assert_eq!(ups.len(), 1);
        assert!(ups[0].contains("DELETE WHERE"), "idempotent replace-subject");
        assert!(ups[0].contains(INSTANCES_GRAPH), "casing-routed to instances graph");
    }

    #[test]
    fn write_routes_to_declared_home_when_provided() {
        // #3647 — with a model-declared home graph, the write lands THERE, not the
        // legacy urn:chorus:instances bucket. This is the orphan fix: athena-make authz
        // reads ownedBy from the declared home, so create must write the same graph.
        let target = format!("{}value-stream-step-proving", NS);
        let store = stub(&[target.as_str()], &[]);
        let home = "urn:chorus:domains:tests"; // a real domain home (NOT the reserved security graph — #3356 AC4)
        let req = WriteReq {
            kind: "domain".into(),
            name: "tests".into(),
            edges: vec![("atStep".into(), "value-stream-step".into(), "proving".into())],
            graph: Some(home.into()),
            ..Default::default()
        };
        write(&store, &req, &tid()).unwrap();
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
        delete_entity(&store, "gate", "x", Some("urn:chorus:domains:tests"), &tid()).unwrap();
        let ups = store.updates.borrow();
        assert!(ups[0].contains("urn:chorus:domains:tests"), "delete targets the declared home");
        assert!(!ups[0].contains(INSTANCES_GRAPH), "delete must not target the legacy bucket when a home is given");
    }

    #[test]
    fn write_stamps_audit_envelope_and_preserves_created() {
        // Jeff's ruling 2026-06-11: dcterms created/modified/creator on every
        // write; created survives a rewrite (read before replace).
        let store = stub(&[], &[]);
        let req = WriteReq { kind: "role".into(), name: "audit-x".into(), ..Default::default() };
        write(&store, &req, &tid()).unwrap();
        let up = store.updates.borrow()[0].clone();
        assert!(up.contains("dc/terms/created"), "created stamped");
        assert!(up.contains("dc/terms/modified"), "modified stamped");
        assert!(up.contains("dc/terms/creator"), "creator stamped");
    }

    #[test]
    fn write_enforces_shape_required_fields_from_store() {
        let store = stub(&[], &["vision"]);
        let req = WriteReq { kind: "product".into(), name: "testprod".into(), ..Default::default() };
        let e = write(&store, &req, &tid()).unwrap_err();
        assert!(e.starts_with("shape-violation"), "{}", e);
        assert!(e.contains("vision"));
    }

    // ── #3468 — delete / link / unlink (the governed verbs athena-make delegates to) ──

    #[test]
    fn delete_entity_refuses_unknown_subject_fail_closed() {
        let store = stub(&[], &[]);
        let e = delete_entity(&store, "domain", "ghost", None, &tid()).unwrap_err();
        assert!(e.starts_with("not-found"), "{}", e);
        assert!(store.updates.borrow().is_empty(), "nothing deleted on a missing subject");
    }

    #[test]
    fn delete_entity_deletes_existing_subject() {
        let subj = format!("{}tests", NS);
        let store = stub(&[subj.as_str()], &[]);
        let got = delete_entity(&store, "domain", "tests", None, &tid()).unwrap();
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
        let got = add_edge(&store, "domain", "tests", "partOf", "product", "athena", None, &tid()).unwrap();
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
        let e = add_edge(&store, "domain", "tests", "partOf", "product", "ghost", None, &tid()).unwrap_err();
        assert!(e.starts_with("unknown-endpoint"), "{}", e);
        assert!(store.updates.borrow().is_empty(), "no edge written when an endpoint is missing");
    }

    #[test]
    fn add_edge_refuses_non_camelcase_property() {
        let store = stub(&[], &[]);
        let e = add_edge(&store, "domain", "tests", "Part-Of", "product", "athena", None, &tid()).unwrap_err();
        assert!(e.starts_with("bad-property"), "{}", e);
    }

    #[test]
    fn remove_edge_is_idempotent_delete_data() {
        let store = stub(&[], &[]); // no existence requirement — removal toward absence
        let got = remove_edge(&store, "domain", "tests", "partOf", "product", "athena", None, &tid()).unwrap();
        assert_eq!(got, format!("{}tests", NS));
        let ups = store.updates.borrow();
        assert_eq!(ups.len(), 1);
        assert!(ups[0].contains("DELETE DATA"), "single-triple removal");
        assert!(ups[0].contains("partOf"));
    }

    // ── Phase A1 add-batch stdin contract ─────────────────────────────────

    #[test]
    fn add_batch_ndjson_decodes_write_reqs_and_tuple_edges() {
        let input = concat!(
            "{\"kind\":\"product\",\"name\":\"Athena\",\"fields\":{\"vision\":\"Legible \\\"truth\\\"\"}}\n",
            "\n",
            "{\"kind\":\"domain\",\"name\":\"Tests\",\"edges\":[[\"partOf\",\"product\",\"Athena\"]],\"graph\":\"urn:chorus:domains:tests\"}\n",
        );
        let reqs = parse_add_batch_ndjson(input).expect("valid NDJSON parses");
        assert_eq!(reqs.len(), 2);
        assert_eq!(reqs[0].fields["vision"], "Legible \"truth\"");
        assert_eq!(reqs[1].edges, vec![("partOf".into(), "product".into(), "Athena".into())]);
        assert_eq!(reqs[1].graph.as_deref(), Some("urn:chorus:domains:tests"));
    }

    #[test]
    fn add_batch_ndjson_refuses_unknown_fields_with_record_line() {
        let input = concat!(
            "{\"kind\":\"role\",\"name\":\"one\"}\n",
            "{\"kind\":\"role\",\"name\":\"two\",\"fileds\":{}}\n",
        );
        let err = parse_add_batch_ndjson(input).unwrap_err();
        assert!(err.contains("line 2"), "record line is named: {err}");
        assert!(err.contains("unknown field `fileds`"), "typo is refused, not dropped: {err}");
    }

    #[test]
    fn add_batch_ndjson_refuses_wrong_edge_tuple_cardinality() {
        let input = "{\"kind\":\"domain\",\"name\":\"tests\",\"edges\":[[\"partOf\",\"product\"]]}\n";
        let err = parse_add_batch_ndjson(input).unwrap_err();
        assert!(err.contains("line 1"), "{err}");
        assert!(err.contains("tuple of size 3"), "tuple shape is typed: {err}");
    }
}

#[cfg(test)]
mod webid_uniqueness_3838 {
    use super::*;

    /// A store that answers the shape-load queries with a uniqueGlobal on webId,
    /// and reports whether a DIFFERENT principal already holds a given value.
    struct UniqStore {
        /// webId values already held by some other principal.
        taken: Vec<String>,
        updates: std::cell::RefCell<Vec<String>>,
    }
    impl Store for UniqStore {
        fn ask(&self, sparql: &str) -> R<bool> {
            // Mirrors lib.rs:925 — the crate asks whether ?other (self-excluded)
            // holds this value. We answer from `taken`.
            Ok(self.taken.iter().any(|v| sparql.contains(v.as_str())))
        }
        fn select_v(&self, sparql: &str) -> R<Vec<String>> {
            // The shape reader asks three separate questions; answer each one
            // the way security-model-3618.ttl now does.
            if sparql.contains("# athena-model uniqueness candidates") {
                return Ok(if self.taken.iter().any(|value| sparql.contains(value)) {
                    vec!["0".into()]
                } else {
                    vec![]
                });
            }
            if sparql.contains("chorus:uniqueGlobal true") {
                return Ok(vec!["webId".into()]);
            }
            if sparql.contains("sh:minCount") {
                return Ok(vec!["label".into()]);
            }
            Ok(vec![])
        }
        fn update(&self, sparql: &str) -> R<()> {
            self.updates.borrow_mut().push(sparql.to_string());
            Ok(())
        }
    }
    fn tid() -> Identity { Identity("wren".into()) }

    fn req(name: &str, webid: &str) -> WriteReq {
        let mut fields = BTreeMap::new();
        fields.insert("label".to_string(), format!("{name} (principal)"));
        fields.insert("webId".to_string(), webid.to_string());
        WriteReq {
            placement_override_reason: None,
            kind: "principal".into(),
            name: name.into(),
            fields,
            edges: vec![],
            // urn:chorus:instances, NOT the security graph. See the module note:
            // the security graph is DBA-path only and the DAL refuses it outright,
            // so the uniqueness primitive is proven here on a graph the DAL owns.
            graph: Some("urn:chorus:instances".into()),
        }
    }

    fn store(taken: &[&str]) -> UniqStore {
        UniqStore {
            taken: taken.iter().map(|s| s.to_string()).collect(),
            updates: Default::default(),
        }
    }

    /// #3838 NEGATIVE PROOF — the one Silas asked for, against the mechanism
    /// that actually runs.
    ///
    /// The WebID is the correlation key between CSS and this graph. It was a
    /// bare string with no uniqueness rule: two principals could claim the same
    /// identity and nothing would object.
    ///
    /// I first wrote this rule as a sh:sparql NodeShape. The validator does not
    /// read sh:sparql — zero occurrences in this crate — so it would have looked
    /// enforced and never fired. chorus:uniqueGlobal is the primitive that is
    /// hand-implemented here (lib.rs:399 loads it, :923 enforces it), so that is
    /// where the rule now lives, and this proves it refuses.
    ///
    /// THE LIMIT, stated because finding it is the point of writing the test:
    /// this proves the PRIMITIVE refuses a duplicate. It does not prove that
    /// PRINCIPALS get that protection, and today they do not. Principals live in
    /// urn:chorus:domains:security, which assert_dal_writable (lib.rs:787)
    /// refuses outright as a DBA-path graph — they are deployed by
    /// athena-deploy-model's SECURITY_SET, a staged GSP merge that never calls
    /// this validator. So the uniqueness rule is real, the enforcement path is
    /// real, and the two do not meet for Principal. That gap is #3838's finding,
    /// not something this test can close, and saying so here is the difference
    /// between a proof and a comfort.
    #[test]
    fn a_second_principal_claiming_the_same_webid_is_refused() {
        let s = store(&["https://id.lightlifeurbangardens.com/jeff/profile/card#me"]);
        let err = write(&s, &req("impostor", "https://id.lightlifeurbangardens.com/jeff/profile/card#me"), &tid())
            .expect_err("a duplicate webId must be refused");
        assert!(
            err.contains("duplicate 'webId'") && err.contains("uniqueGlobal"),
            "the refusal must name the field and the rule, not just fail: {err}"
        );
    }

    /// NEGATIVE PROOF of the negative proof: a principal with its OWN webId
    /// passes. Without this, the test above would pass against an implementation
    /// that refuses everything — which is a different kind of broken and looks
    /// identical from the failing side.
    #[test]
    fn a_principal_with_its_own_webid_passes() {
        let s = store(&["https://id.lightlifeurbangardens.com/jeff/profile/card#me"]);
        write(&s, &req("marknakib", "https://id.lightlifeurbangardens.com/marknakib/profile/card#me"), &tid())
            .expect("a distinct webId must be accepted");
    }
}

/// #3839 — the deploy now goes through this door, so the door has to be proven
/// at the shape the deploy actually uses: several kinds, one transaction.
///
/// Every test here except the first is a NEGATIVE PROOF (#3734) — a fixture
/// where the guarded condition is violated, showing the check FAILS. The
/// batching widened exactly one thing (which IRIs count as present); these
/// exist to show it widened only that, and that nothing is written when any
/// group refuses.
#[cfg(test)]
mod seed_multi_3839 {
    use super::*;

    const VS: &str = "https://jeffbridwell.com/chorus#ValueStream";
    const VSS: &str = "https://jeffbridwell.com/chorus#ValueStreamStep";

    /// An EMPTY store: nothing exists yet. That is the condition the old
    /// two-independent-runs path could never survive, and the one a deploy has
    /// to survive to be reproducible.
    struct EmptyStore {
        updates: std::cell::RefCell<Vec<String>>,
        /// (sub, super) pairs the ontology would answer subClassOf* true for.
        subclasses: Vec<(String, String)>,
        /// What the shape pins as its instances graph. None = the shape declares
        /// no pin, which is the #3581/#3675/#3838 zero-rows condition.
        pin: Option<String>,
        /// subject IRI → content hash already stored (idempotence fixture).
        stored: Vec<(String, String)>,
    }
    impl Store for EmptyStore {
        fn ask(&self, sparql: &str) -> R<bool> {
            if sparql.contains("subClassOf") {
                return Ok(self
                    .subclasses
                    .iter()
                    .any(|(sub, sup)| sparql.contains(sub.as_str()) && sparql.contains(sup.as_str())));
            }
            Ok(false) // nothing pre-exists — every edge target must come from the batch
        }
        fn select_v(&self, sparql: &str) -> R<Vec<String>> {
            if sparql.contains("chorus:instancesGraph ?v") {
                return Ok(self.pin.iter().cloned().collect());
            }
            if sparql.contains("contentHash") {
                return Ok(self
                    .stored
                    .iter()
                    .map(|(s, h)| format!("{}|{}", s, h))
                    .collect());
            }
            if sparql.contains("sh:datatype ?dt") && sparql.contains(VSS) {
                // The step's stageOrder is declared xsd:integer — without this the
                // datatype checks have nothing to compare against and the test
                // would pass for the wrong reason (the first version of this
                // fixture did exactly that).
                return Ok(vec!["stageOrder|integer".into()]);
            }
            if sparql.contains("sh:minCount") {
                // Each class requires one property, so a missing one is provable.
                // VSS first: "ValueStream" is a PREFIX of "ValueStreamStep", so
                // testing the shorter one first answers the step's query with the
                // stream's shape. The first version of this fixture did exactly
                // that and reported a violation the code had not made.
                if sparql.contains(VSS) {
                    return Ok(vec!["stageOrder".into()]);
                }
                if sparql.contains(VS) {
                    return Ok(vec!["outcome".into()]);
                }
            }
            Ok(vec![])
        }
        fn update(&self, sparql: &str) -> R<()> {
            self.updates.borrow_mut().push(sparql.to_string());
            Ok(())
        }
    }
    fn store() -> EmptyStore {
        EmptyStore {
            updates: Default::default(),
            subclasses: vec![],
            pin: Some("urn:chorus:instances".into()),
            stored: vec![],
        }
    }
    /// A store whose ontology knows AgentRole is a subclass of Role.
    fn store_with_subclass() -> EmptyStore {
        EmptyStore {
            updates: Default::default(),
            subclasses: vec![(
                "https://jeffbridwell.com/chorus#AgentRole".into(),
                "https://jeffbridwell.com/chorus#Role".into(),
            )],
            pin: Some("urn:chorus:instances".into()),
            stored: vec![],
        }
    }
    fn tid() -> Identity { Identity("wren".into()) }

    fn t(s: &str, p: &str, o: &str) -> (String, String, String) {
        (s.to_string(), p.to_string(), o.to_string())
    }
    fn iri(local: &str) -> String { format!("<https://jeffbridwell.com/chorus#{}>", local) }
    fn prop(local: &str) -> String { format!("<https://jeffbridwell.com/chorus#{}>", local) }

    /// The stream references its step; the step references the stream back.
    fn streams() -> Vec<(String, String, String)> {
        vec![
            t(&iri("value-stream-werk"), &prop("outcome"), "\"A card landed\""),
            t(&iri("value-stream-werk"), &prop("contains"), &iri("value-stream-step-pull")),
        ]
    }
    fn steps() -> Vec<(String, String, String)> {
        vec![
            t(&iri("value-stream-step-pull"), &prop("stageOrder"), "\"1\""),
            t(&iri("value-stream-step-pull"), &prop("inStream"), &iri("value-stream-werk")),
        ]
    }

    /// The thing that was impossible. Two kinds that point at each other load
    /// against an empty store — because they are one batch, so each is present
    /// for the other's referential check.
    #[test]
    fn mutually_referential_kinds_load_on_an_empty_store() {
        let st = store();
        let (a, b) = (streams(), steps());
        let r = seed_multi(
            &st,
            &[
                SeedGroup { kind: "value-stream", triples: &a },
                SeedGroup { kind: "value-stream-step", triples: &b },
            ],
            "deploy",
            Some("urn:chorus:instances"),
            &tid(),
        )
        .expect("a mutually-referential batch must load");
        assert_eq!(r.subjects, 2, "both subjects written");
        assert_eq!(st.updates.borrow().len(), 1, "ONE transaction, not one per kind");
    }

    /// NEGATIVE PROOF — the widening did not turn the referential check off. A
    /// target that is in neither the batch nor the store still refuses.
    #[test]
    fn a_target_in_neither_batch_nor_store_still_refuses() {
        let st = store();
        let mut a = streams();
        a.push(t(&iri("value-stream-werk"), &prop("contains"), &iri("value-stream-step-ghost")));
        let b = steps();
        let e = seed_multi(
            &st,
            &[
                SeedGroup { kind: "value-stream", triples: &a },
                SeedGroup { kind: "value-stream-step", triples: &b },
            ],
            "deploy",
            Some("urn:chorus:instances"),
            &tid(),
        )
        .expect_err("a dangling target must refuse");
        assert!(e.contains("unknown-target"), "refusal names the class of problem: {e}");
        assert!(e.contains("value-stream-step-ghost"), "refusal names WHICH target: {e}");
        assert!(st.updates.borrow().is_empty(), "nothing written");
    }

    /// NEGATIVE PROOF — the whole point of routing the deploy through here. A
    /// shape violation in the SECOND group refuses the batch, and the FIRST
    /// group's subjects are not written. The old staged-POST path would have
    /// written both without asking.
    #[test]
    fn a_violation_in_one_group_writes_nothing_from_any_group() {
        let st = store();
        let a = streams();
        // Drop the required stageOrder from the step.
        let b = vec![t(&iri("value-stream-step-pull"), &prop("inStream"), &iri("value-stream-werk"))];
        let e = seed_multi(
            &st,
            &[
                SeedGroup { kind: "value-stream", triples: &a },
                SeedGroup { kind: "value-stream-step", triples: &b },
            ],
            "deploy",
            Some("urn:chorus:instances"),
            &tid(),
        )
        .expect_err("a missing required property must refuse");
        assert!(e.contains("shape-violation"), "typed refusal: {e}");
        assert!(e.contains("stageOrder"), "refusal names WHICH constraint: {e}");
        assert!(e.contains("value-stream-step-pull"), "refusal names WHICH subject: {e}");
        assert!(
            st.updates.borrow().is_empty(),
            "group 1 validated clean but must NOT be written — the batch is one transaction"
        );
    }

    /// NEGATIVE PROOF — per-group typing survived batching. A step's subject IRI
    /// passes the value-stream iri-guard by prefix accident, so rdf:type is what
    /// catches it; put the step in the stream group and the door must object.
    #[test]
    fn a_subject_typed_against_the_wrong_group_refuses() {
        let st = store();
        let mut a = streams();
        a.push(t(
            &iri("value-stream-step-pull"),
            "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>",
            &iri("ValueStreamStep"),
        ));
        let e = seed_multi(
            &st,
            &[SeedGroup { kind: "value-stream", triples: &a }],
            "deploy",
            Some("urn:chorus:instances"),
            &tid(),
        )
        .expect_err("a step declared inside the stream group must refuse");
        assert!(e.contains("shape-violation"), "typed refusal: {e}");
        assert!(e.contains("value-stream-step-pull"), "refusal names the subject: {e}");
        assert!(st.updates.borrow().is_empty(), "nothing written");
    }

    /// NEGATIVE PROOF — a subject claimed by two kinds would be validated
    /// against two different shapes and then race in the write. Refuse instead
    /// of picking a winner.
    #[test]
    fn the_same_subject_in_two_groups_refuses() {
        let st = store();
        let a = streams();
        let mut b = steps();
        b.push(t(&iri("value-stream-werk"), &prop("stageOrder"), "\"1\""));
        let e = seed_multi(
            &st,
            &[
                SeedGroup { kind: "value-stream", triples: &a },
                SeedGroup { kind: "value-stream-step", triples: &b },
            ],
            "deploy",
            Some("urn:chorus:instances"),
            &tid(),
        )
        .expect_err("a doubly-claimed subject must refuse");
        assert!(e.contains("claimed by two kinds"), "refusal says what happened: {e}");
        assert!(st.updates.borrow().is_empty(), "nothing written");
    }

    const RDF_TYPE: &str = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";

    fn role_triples(extra_type: Option<&str>) -> Vec<(String, String, String)> {
        let mut v = vec![
            t(&iri("role-wren"), RDF_TYPE, &iri("Role")),
            t(&iri("role-wren"), &prop("roleKind"), "\"agent\""),
        ];
        if let Some(x) = extra_type {
            v.push(t(&iri("role-wren"), RDF_TYPE, &iri(x)));
        }
        v
    }

    /// #3838's roles are `chorus:Role, chorus:AgentRole` on purpose — the store
    /// does no inference, so a role typed only AgentRole vanishes from every
    /// Role query. The door must be able to load the model's own convention.
    #[test]
    fn dual_typing_with_a_real_subclass_loads() {
        let st = store_with_subclass();
        let tr = role_triples(Some("AgentRole"));
        seed_multi(
            &st,
            &[SeedGroup { kind: "role", triples: &tr }],
            "deploy",
            Some("urn:chorus:instances"),
            &tid(),
        )
        .expect("Role + AgentRole must load");
        assert_eq!(st.updates.borrow().len(), 1);
    }

    /// NEGATIVE PROOF — accepting subclasses did not become accepting anything.
    /// An extra type the ontology does NOT relate to the kind still refuses.
    #[test]
    fn an_unrelated_extra_type_still_refuses() {
        let st = store_with_subclass();
        let tr = role_triples(Some("ValueStream"));
        let e = seed_multi(
            &st,
            &[SeedGroup { kind: "role", triples: &tr }],
            "deploy",
            Some("urn:chorus:instances"),
            &tid(),
        )
        .expect_err("an unrelated second type must refuse");
        assert!(e.contains("not a subclass"), "refusal says why: {e}");
        assert!(e.contains("ValueStream"), "refusal names the offending type: {e}");
        assert!(st.updates.borrow().is_empty(), "nothing written");
    }

    /// NEGATIVE PROOF — the typed-literal fix. `"1"^^xsd:integer` must validate
    /// as the integer 1, and a literal that names a datatype the shape did not
    /// declare must refuse rather than be quietly accepted.
    #[test]
    fn a_typed_literal_is_read_as_its_value_not_its_tail() {
        // BEFORE the fix, trim_matches('"') left `1"^^<...#integer>` as the
        // "value", so this well-formed literal failed its own datatype check —
        // the door refusing correct data, which is how the bug surfaced.
        let good = vec![
            t(&iri("value-stream-step-pull"), &prop("stageOrder"),
              "\"1\"^^<http://www.w3.org/2001/XMLSchema#integer>"),
        ];
        let st = store();
        seed_multi(&st, &[SeedGroup { kind: "value-stream-step", triples: &good }],
                   "deploy", Some("urn:chorus:instances"), &tid())
            .expect("a well-formed typed literal must load");

        // And the teeth: a literal carrying a datatype the shape did not declare.
        let bad = vec![
            t(&iri("value-stream-step-pull"), &prop("stageOrder"),
              "\"first\"^^<http://www.w3.org/2001/XMLSchema#string>"),
        ];
        let st2 = store();
        let e = seed_multi(&st2, &[SeedGroup { kind: "value-stream-step", triples: &bad }],
                           "deploy", Some("urn:chorus:instances"), &tid())
            .expect_err("a datatype the shape did not declare must refuse");
        assert!(e.contains("shape-violation"), "typed refusal: {e}");
        assert!(st2.updates.borrow().is_empty(), "nothing written");
    }

    /// NEGATIVE PROOF (AC7) — a shape with no chorus:instancesGraph pin. The
    /// write would succeed and the surface would serve nothing: exactly the
    /// zero-rows failure #3581, #3675 and #3838 each hit separately, and the one
    /// thing a write's own result can never tell you.
    #[test]
    fn a_shape_with_no_instances_graph_pin_refuses() {
        let mut st = store();
        st.pin = None;
        let a = streams();
        // No --graph: the shape is the only thing that could say where these go.
        let e = seed_multi(&st, &[SeedGroup { kind: "value-stream", triples: &a }],
                           "deploy", None, &tid())
            .expect_err("an unpinned shape with no stated graph must refuse");
        assert!(e.contains("instancesGraph"), "refusal names the missing pin: {e}");
        assert!(st.updates.borrow().is_empty(), "nothing written");
    }

    /// The other direction — a pinned shape resolves its own target, so the
    /// caller does not have to repeat it and cannot get it wrong. Without this
    /// the refusal above would be indistinguishable from "seed always needs
    /// --graph".
    #[test]
    fn a_pinned_shape_resolves_its_own_graph() {
        let st = store(); // pin = urn:chorus:instances
        let (a, b) = (streams(), steps());
        seed_multi(&st, &[SeedGroup { kind: "value-stream", triples: &a },
                          SeedGroup { kind: "value-stream-step", triples: &b }],
                   "deploy", None, &tid())
            .expect("a pinned shape needs no explicit graph");
        assert!(
            st.updates.borrow()[0].contains("GRAPH <urn:chorus:instances>"),
            "it wrote to the graph the SHAPE named"
        );
    }

    /// An EXPLICIT graph is honored even when it is not the shape's pin —
    /// per-domain graphs are legitimate (the ICD set seeds Domain individuals
    /// into urn:chorus:domains:icd). The door refuses guesses, not instructions.
    #[test]
    fn an_explicit_graph_is_honored_over_the_pin() {
        let mut st = store();
        st.pin = Some("urn:chorus:instances".into());
        let (a, b) = (streams(), steps());
        seed_multi(&st, &[SeedGroup { kind: "value-stream", triples: &a },
                          SeedGroup { kind: "value-stream-step", triples: &b }],
                   "deploy", Some("urn:chorus:domains:werk"), &tid())
            .expect("an explicitly stated graph is an instruction, not a guess");
        assert!(st.updates.borrow()[0].contains("GRAPH <urn:chorus:domains:werk>"));
    }

    /// AC4 — re-deploying unchanged content is a no-op. The nightly deploy runs
    /// over these instances every night; without this it rewrote all of them and
    /// bumped dcterms:modified each time, destroying the only signal that says
    /// when a thing actually changed.
    #[test]
    fn identical_content_is_a_no_op() {
        let a = streams();
        // First run writes, and the hash it stores is derived from the same
        // authored triples the second run will hash.
        let b = steps(); // the stream references its step — one batch, as deployed
        let st = store();
        seed_multi(&st, &[SeedGroup { kind: "value-stream", triples: &a },
                          SeedGroup { kind: "value-stream-step", triples: &b }],
                   "deploy", Some("urn:chorus:instances"), &tid()).unwrap();
        let written = st.updates.borrow()[0].clone();
        let hash = written
            .split("contentHash> \"")
            .nth(1)
            .and_then(|r| r.split('"').next())
            .expect("the write records a content hash")
            .to_string();

        let mut st2 = store();
        st2.stored = vec![("https://jeffbridwell.com/chorus#value-stream-werk".into(), hash)];
        let r = seed_multi(&st2, &[SeedGroup { kind: "value-stream", triples: &a },
                                   SeedGroup { kind: "value-stream-step", triples: &b }],
                           "deploy", Some("urn:chorus:instances"), &tid()).unwrap();
        // Only the stream's hash is on file, so the step is still written — the
        // assertion is that the STREAM was skipped, not that nothing happened.
        assert_eq!(r.subjects, 1, "the stream is skipped, the step is not");
        assert!(
            !st2.updates.borrow()[0].contains("value-stream-werk> ?p ?o"),
            "the unchanged stream must not be deleted-and-rewritten"
        );
    }

    /// NEGATIVE PROOF (AC4) — the skip is not a blanket skip. A subject whose
    /// content CHANGED must still be written, even though a hash is on file.
    /// Without this the idempotence check would be indistinguishable from
    /// "never write anything twice".
    #[test]
    fn changed_content_is_still_written() {
        let mut st = store();
        st.stored = vec![(
            "https://jeffbridwell.com/chorus#value-stream-werk".into(),
            "0000000000000000".into(), // a hash from some earlier, different content
        )];
        let a = streams();
        let b = steps();
        let r = seed_multi(&st, &[SeedGroup { kind: "value-stream", triples: &a },
                                  SeedGroup { kind: "value-stream-step", triples: &b }],
                           "deploy", Some("urn:chorus:instances"), &tid()).unwrap();
        assert_eq!(r.subjects, 2, "a stale hash means the subject is written, not skipped");
        assert!(
            st.updates.borrow()[0].contains("value-stream-werk> ?p ?o"),
            "the changed stream IS deleted-and-rewritten"
        );
    }

    /// NEGATIVE PROOF — the DBA-path graphs stayed refused. Batching gave the
    /// caller a new way in; it must not be a new way around assert_dal_writable.
    #[test]
    fn a_batch_cannot_reach_a_dba_path_graph() {
        let st = store();
        let a = streams();
        for g in ["urn:chorus:ontology", "urn:chorus:domains:security"] {
            let e = seed_multi(
                &st,
                &[SeedGroup { kind: "value-stream", triples: &a }],
                "deploy",
                Some(g),
                &tid(),
            )
            .expect_err("the DAL must refuse a DBA-path graph");
            assert!(st.updates.borrow().is_empty(), "nothing written to {g}: {e}");
        }
    }
}

/// #4022 — the Fuseki write credential, resolved by the DAL itself: the env
/// (`FUSEKI_ADMIN_PASSWORD`) when a launcher exported it, else the 0600 cred
/// file (`FUSEKI_WRITE_ENV`, default `~/.gathering/data/fuseki-write.env`).
/// Until now only `athena-make-launch.sh` sourced that file, so every OTHER
/// caller — the demo-env athena variant, a launchd-domain setenv that a reboot
/// wiped (2026-08-27 21:xx, Silas), a role shell — wrote unauthenticated and
/// 401'd. The value never leaves this process except on curl's argv.
pub fn fuseki_admin_password() -> Option<String> {
    if let Ok(pw) = std::env::var("FUSEKI_ADMIN_PASSWORD") {
        if !pw.is_empty() {
            return Some(pw);
        }
    }
    fuseki_admin_password_from_file(&fuseki_write_env_path())
}

pub fn fuseki_write_env_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("FUSEKI_WRITE_ENV") {
        if !p.trim().is_empty() {
            return std::path::PathBuf::from(p);
        }
    }
    let home = std::env::var("HOME").unwrap_or_default();
    std::path::PathBuf::from(format!("{}/.gathering/data/fuseki-write.env", home))
}

/// The cred file is `KEY=value` lines; only `FUSEKI_ADMIN_PASSWORD` is read.
/// Absent file, unreadable file, or no such line → None (the write goes out
/// unauthenticated and Fuseki's 401 says so — never a fabricated credential).
pub fn fuseki_admin_password_from_file(path: &std::path::Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    text.lines()
        .map(str::trim)
        .find_map(|l| l.strip_prefix("FUSEKI_ADMIN_PASSWORD="))
        .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|v| !v.is_empty())
}

#[cfg(test)]
mod fuseki_cred_4022 {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("athena-model-4022-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d.join(name)
    }

    #[test]
    fn negative_proof_no_file_no_line_means_no_credential() {
        assert_eq!(fuseki_admin_password_from_file(std::path::Path::new("/nonexistent/fuseki-write.env")), None);
        let p = scratch("empty.env");
        std::fs::write(&p, "FUSEKI_ADMIN_USER=admin
# no password line
").unwrap();
        assert_eq!(fuseki_admin_password_from_file(&p), None);
        let q = scratch("blank.env");
        std::fs::write(&q, "FUSEKI_ADMIN_PASSWORD=
").unwrap();
        assert_eq!(fuseki_admin_password_from_file(&q), None, "an empty value is not a credential");
    }

    #[test]
    fn control_the_cred_file_line_is_read_quoted_or_bare() {
        let p = scratch("ok.env");
        std::fs::write(&p, "FUSEKI_ADMIN_USER=admin
FUSEKI_ADMIN_PASSWORD=\"FIXTURE-NOT-A-PASSWORD-1\"
").unwrap();
        assert_eq!(fuseki_admin_password_from_file(&p).as_deref(), Some("FIXTURE-NOT-A-PASSWORD-1"));
        let q = scratch("bare.env");
        std::fs::write(&q, "FUSEKI_ADMIN_PASSWORD=FIXTURE-NOT-A-PASSWORD-2").unwrap();
        assert_eq!(fuseki_admin_password_from_file(&q).as_deref(), Some("FIXTURE-NOT-A-PASSWORD-2"));
    }
}

#[cfg(test)]
mod deploy_partitions_4089 {
    use super::deploy_partitions;

    #[test]
    fn groups_by_home_in_first_seen_order_and_keeps_manifest_order_inside() {
        let homes: Vec<String> = ["i", "i", "s", "i", "s", "t"].iter().map(|s| s.to_string()).collect();
        let parts = deploy_partitions(&homes);
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], ("i".to_string(), vec![0, 1, 3]));
        assert_eq!(parts[1], ("s".to_string(), vec![2, 4]));
        assert_eq!(parts[2], ("t".to_string(), vec![5]));
    }

    /// NEGATIVE PROOF for the ordering claim: a manifest that names the
    /// services graph before the instances graph must seed services FIRST —
    /// the partitioner never re-sorts to a canonical order.
    #[test]
    fn never_reorders_to_a_canonical_graph_order() {
        let homes: Vec<String> = ["urn:chorus:domains:services", "urn:chorus:instances"].iter().map(|s| s.to_string()).collect();
        let parts = deploy_partitions(&homes);
        assert_eq!(parts[0].0, "urn:chorus:domains:services");
        assert_eq!(parts[1].0, "urn:chorus:instances");
    }
}
