//! owl-api — the OWL→API generator + server (#3350, the model-5 spike).
//!
//! GENERATE: read the Domain class + DomainShape from `urn:chorus:ontology`
//! and emit a route table (`routes.json`). Nothing about the API is hand-
//! written: the field list IS the shape's direct-path properties, the routes
//! are derived per class. Deterministic — same graph in, same routes out.
//!
//! SERVE: load `routes.json`, answer over HTTP from the live graph. Read-only
//! by construction (no write route exists to generate); writes go through
//! chorus-model, the DAL. Runs as its own process — model queries never touch
//! the chorus-api Node event loop (the ADR-034 lesson).
//!
//! Zero-dep (ADR-032 §1): std-only HTTP on TcpListener; SPARQL via `curl`.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;

/// #3402 — seam auth: local HS256 service-token verification (ADR-042 / #3401).
pub mod auth;

pub const NS: &str = "https://jeffbridwell.com/chorus#";
pub const ONTOLOGY_GRAPH: &str = "urn:chorus:ontology";
pub const INSTANCES_GRAPH: &str = "urn:chorus:instances";
/// #3506 / ADR-047 — the response-contract version. Coarse, infrastructure-wide;
/// path-prefixed (/v1/...) AND echoed in every envelope. Bumps only when the
/// envelope shape changes — orthogonal to a primitive's per-shape `shapeVersion`.
pub const API_VERSION: &str = "v1";

// #3435 — re-export the pure resolver surface (#3437) so the handler and consumers
// import one place: owl_api::{ScopeKind, ScopeNode, decide_effective_value, ...}.
pub use properties_resolver::{
    decide_effective_value, CascadeError, PropertyDatum, Resolution, ScopeKind, ScopeNode,
};

pub type R<T> = Result<T, String>;

fn fuseki() -> String {
    std::env::var("CHORUS_FUSEKI").unwrap_or_else(|_| "http://localhost:3030/pods".to_string())
}

pub fn sparql_json(query: &str) -> R<String> {
    let out = Command::new("curl")
        .args([
            "-sf", "--max-time", "20",
            "-H", "Accept: application/sparql-results+json",
            "--data-urlencode", &format!("query={}", query),
            &format!("{}/query", fuseki()),
        ])
        .output()
        .map_err(|e| format!("curl-spawn: {}", e))?;
    if !out.status.success() {
        return Err(format!("fuseki-query failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// The DAL's proven extractor: all bound values of the single ?v variable.
/// Multi-column queries CONCAT their columns into ?v with a `|` separator —
/// the single-var seam is what makes zero-dep parsing reliable.
pub fn select_v(body: &str) -> Vec<String> {
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
    vals
}


// ─── #3354 observability: the telemetry envelope ────────────────────────────
// Per-request lines go to DATED jsonl files (day-boundary rotation) via
// in-process appends — never a subprocess. Promtail's *.jsonl glob ships them
// to Loki; ONLY aggregates/health transitions touch the spine.

/// Typed request status — refusals are never errors (the 2026-06-11 noise
/// lesson, enforced in the schema so no dashboard can conflate them).
#[derive(Debug, Clone, PartialEq)]
pub enum ReqStatus {
    Ok,
    Refused(String),
    Error(String),
}

impl ReqStatus {
    pub fn as_str(&self) -> String {
        match self {
            ReqStatus::Ok => "ok".to_string(),
            ReqStatus::Refused(c) => format!("refused:{}", c),
            ReqStatus::Error(c) => format!("error:{}", c),
        }
    }
}

impl Default for ReqStatus {
    fn default() -> Self {
        ReqStatus::Ok
    }
}

/// One request's telemetry — the envelope settled with Kade (#3354 design):
/// class/entity/route/fold (the per-fold SLI key), typed status, result_count
/// (count:0 + ok = SPARQL's silent-broken-chain signal), latencies, caller,
/// trace_id (joins the card→werk chain).
#[derive(Debug, Default, Clone)]
pub struct TelemetryLine {
    pub class: String,
    pub entity: String,
    pub route: String,
    pub fold: String,
    pub status: ReqStatus,
    pub result_count: i64,
    pub total_ms: u128,
    pub upstream_ms: u128,
    pub caller: String,
    pub trace_id: String,
    // #3506 / ADR-047 AC3 — the contract emit-dims (computed once per class at boot,
    // not per request). apiVersion is the constant API_VERSION, emitted directly.
    pub product: String,
    pub shape_version: String,
    pub commit: String,
}

impl TelemetryLine {
    pub fn to_jsonl(&self, ts_ms: u128) -> String {
        format!(
            "{{\"ts\":{},\"event\":\"api.request.served\",\"service\":\"owl-api\",\"class\":\"{}\",\"entity\":\"{}\",\"route\":\"{}\",\"fold\":\"{}\",\"status\":\"{}\",\"result_count\":{},\"total_ms\":{},\"upstream_ms\":{},\"caller\":\"{}\",\"trace_id\":\"{}\",\"product\":\"{}\",\"apiVersion\":\"{}\",\"shapeVersion\":\"{}\",\"commit\":\"{}\"}}\n",
            ts_ms,
            json_escape(&self.class),
            json_escape(&self.entity),
            json_escape(&self.route),
            json_escape(&self.fold),
            self.status.as_str(),
            self.result_count,
            self.total_ms,
            self.upstream_ms,
            json_escape(&self.caller),
            json_escape(&self.trace_id),
            json_escape(&self.product),
            API_VERSION,
            json_escape(&self.shape_version),
            json_escape(&self.commit)
        )
    }
}

/// Dated telemetry path: ops/logs/owl-api-YYYYMMDD.jsonl under CHORUS_HOME.
/// Day boundary = free rotation; a retention sweep prunes old files.
pub fn telemetry_path(home: &str, ts_ms: u128) -> String {
    // civil date from epoch days (the standard era-based algorithm) — zero-dep.
    let days = (ts_ms / 86_400_000) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{}/ops/logs/owl-api-{:04}{:02}{:02}.jsonl", home, y, m, d)
}

/// Append a telemetry line. Best-effort: a telemetry failure never affects
/// the response (stderr only). O_APPEND keeps single-line writes atomic.
pub fn emit_telemetry(line: &TelemetryLine) {
    let home = std::env::var("CHORUS_HOME")
        .unwrap_or_else(|_| "/Users/jeffbridwell/CascadeProjects/chorus".to_string());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let path = telemetry_path(&home, now);
    if let Some(dir) = std::path::Path::new(&path).parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(line.to_jsonl(now).as_bytes());
    } else {
        eprintln!("owl-api: telemetry append failed for {}", path);
    }
}

/// The generated route table. Derived, never hand-edited.
#[derive(Debug, Clone)]
pub struct RouteTable {
    pub class: String,           // chorus:Domain
    pub fields: Vec<String>,     // direct-path shape properties (label, comment, ...)
    pub routes: Vec<String>,     // human-readable route list (the artifact)
    pub secured: Vec<String>,    // #3414 — surfaces requiring auth, PROJECTED from the OWL annotation
    pub mandatory: Vec<String>,  // #3468 — the completeness FLOOR: properties at sh:severity sh:Violation, PROJECTED from the shape
    pub repo_target: String,     // #3488 — repo land location for generated artifacts, from chorus:repoTarget (or class-keyed default)
    pub exposure: Vec<(String, String)>, // #3506/ADR-048 §3 — field localname → exposure level (public|internal|secret), PROJECTED from chorus:exposure. Unmarked = hidden (fail-closed).
}

/// #3506 / ADR-048 §3 — the read-side field-exposure gate (fail-closed). A field's
/// projected `chorus:exposure` level decides whether it appears in `data`:
///   public → always · internal → authed callers only · secret → never ·
///   None (unmarked) → hidden. Pure + unit-pinned.
pub fn field_exposed(level: Option<&str>, authed: bool) -> bool {
    match level {
        Some("public") => true,
        Some("internal") => authed,
        Some("secret") => false,
        _ => false, // unmarked/unknown → hidden (default-closed)
    }
}

/// ADR-040 conformance at the source (#3364 AC1): the generator REFUSES to
/// emit routes from non-conformant input — L4 naming law enforced where the
/// API is born, not audited after. Classes are CamelCase, properties are
/// camelCase. A violation is a typed refusal, never a bad route.
pub fn adr040_check(class_local: &str, fields: &[String]) -> Result<(), String> {
    let class_ok = class_local
        .chars()
        .next()
        .map(|c| c.is_ascii_uppercase())
        .unwrap_or(false)
        && class_local.chars().all(|c| c.is_ascii_alphanumeric());
    if !class_ok {
        return Err(format!(
            "adr040-violation: class '{}' is not CamelCase (ADR-040 L4: classes CamelCase, e.g. ValueStreamStep)",
            class_local
        ));
    }
    for f in fields {
        let name = f.split('|').next().unwrap_or(f);
        let field_ok = name
            .chars()
            .next()
            .map(|c| c.is_ascii_lowercase())
            .unwrap_or(false)
            && name.chars().all(|c| c.is_ascii_alphanumeric());
        if !field_ok {
            return Err(format!(
                "adr040-violation: property '{}' is not camelCase (ADR-040 L4: properties camelCase, e.g. ownedBy)",
                name
            ));
        }
    }
    Ok(())
}

/// #3414 — PURE projection of the secured-set from the model's auth annotation.
/// `annotated` = the class's shape carries `chorus:requiresAuth true`. Annotated →
/// the class's schema surface is guarded; otherwise NOTHING (mixed-state: an
/// undeclared surface stays open, AC3). Pure so the projection is unit-tested without
/// a live graph — the SPARQL read in generate() is integration-proven separately.
pub fn project_secured(class_local: &str, annotated: bool) -> Vec<String> {
    if annotated {
        vec![format!("/schema/{}", class_local.to_lowercase())]
    } else {
        Vec::new()
    }
}

/// #3488 — the repo tree IS the OWL containment graph projected onto a
/// filesystem, RECURSIVELY (Jeff, 2026-06-18: "its like our repo becomes
/// recursive in exactly the same way as our owl"). A `RepoKind` is a containment
/// level; every level except the value-stream root carries a collection prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoKind {
    ValueStream, // the step: a bare root segment (designing/, building/, …)
    Product,     // products/<name>
    Domain,      // domains/<name>
    Service,     // services/<name>
}

impl RepoKind {
    /// The collection directory this kind lives under, or None for the bare root.
    pub fn collection(self) -> Option<&'static str> {
        match self {
            RepoKind::ValueStream => None,
            RepoKind::Product => Some("products"),
            RepoKind::Domain => Some("domains"),
            RepoKind::Service => Some("services"),
        }
    }
}

/// Project an ordered ROOT→LEAF containment chain into a repo path (ADR-041's
/// Value Stream → Products → Domains, generalized). The vs-step is the bare root;
/// every other ancestor emits `<collection>/<name>`. RECURSIVE by construction —
/// sub-products, sub-domains, and a service that parents off a PRODUCT (a
/// cross-domain service like the clearing/chorus service) vs off a DOMAIN are all
/// just different (kind, name) links in the chain; the projector follows whatever
/// the model says the parent is. Empty/whitespace names are skipped. Pure.
pub fn project_repo_path(chain: &[(RepoKind, &str)]) -> String {
    let mut segs: Vec<String> = Vec::new();
    for (kind, name) in chain {
        let n = name.trim().trim_end_matches('/').to_lowercase();
        if n.is_empty() {
            continue;
        }
        match kind.collection() {
            Some(coll) => {
                segs.push(coll.to_string());
                segs.push(n);
            }
            None => segs.push(n),
        }
    }
    segs.join("/")
}

/// Resolve the repo land location for a generated entity, HONORING the recursive
/// containment structure. `declared` (`chorus:repoTarget`, non-empty) is the
/// explicit override for a bespoke case; otherwise the path is PROJECTED from the
/// walked containment `chain` (root→leaf). The LOCATION half of "generated APIs
/// land in the repo where they belong". Pure so it's unit-pinned; the SPARQL walk
/// that assembles the chain in generate() is integration-proven separately.
pub fn resolve_repo_target(declared: Option<&str>, chain: &[(RepoKind, &str)]) -> String {
    if let Some(p) = declared.map(str::trim).filter(|s| !s.is_empty()) {
        return p.trim_end_matches('/').to_string();
    }
    project_repo_path(chain)
}

/// #3488 — read a single containment-edge target's localname for `class` (None
/// if absent). Covers both modeling styles: the edge on the shape
/// (`?shape sh:targetClass <class> ; <pred> ?t`) or on the class directly
/// (`<class> <pred> ?t`). `strip` removes a kind-tag prefix from the localname
/// so `chorus:value-stream-step-designing` → `designing`. Best-effort: a missing
/// edge yields None (that level is simply skipped in the projected path).
fn read_containment_local(class: &str, pred: &str, strip: &str) -> R<Option<String>> {
    let q = format!(
        "PREFIX sh: <http://www.w3.org/ns/shacl#> PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <{g}> {{ {{ ?s sh:targetClass <{c}> ; {pred} ?t }} UNION {{ <{c}> {pred} ?t }} BIND(REPLACE(STR(?t), '.*[#/]', '') AS ?v) }} }} LIMIT 1",
        ns = NS, g = ONTOLOGY_GRAPH, c = class, pred = pred
    );
    let raw = select_v(&sparql_json(&q)?).into_iter().next();
    Ok(raw.map(|v| v.strip_prefix(strip).unwrap_or(&v).to_string()))
}

/// #3488 — project a PRODUCT API index from its bound domains. A product's API
/// is the aggregate of the domains it `hasDomain`: generating a domain API binds
/// it here BY CONSTRUCTION (Jeff, 2026-06-18: "automation to bind the domain api
/// to the product api"). Add/remove a hasDomain edge → regenerate → the binding
/// follows; no manual register step, so the product API can't drift from the set
/// of domains that exist (registration-is-derived). Pure; the SPARQL read is
/// integration-proven separately. Names lowercased, sorted, de-duped; each
/// domain carries its API mount (the pluralized route root the domain serves).
pub fn project_product_index(product: &str, domains: &[&str]) -> String {
    let mut ds: Vec<String> = domains
        .iter()
        .map(|d| d.trim().to_lowercase())
        .filter(|d| !d.is_empty())
        .collect();
    ds.sort();
    ds.dedup();
    let items: Vec<String> = ds
        .iter()
        .map(|d| format!("{{ \"name\": \"{}\", \"api\": \"/{}\" }}", d, d))
        .collect();
    format!(
        "{{ \"product\": \"{}\", \"domains\": [{}] }}",
        product.trim().to_lowercase(),
        items.join(", ")
    )
}

/// #3488 — read the domains a product `hasDomain` (localnames) for the product
/// API index. Instance edge: `<product> chorus:hasDomain ?d`.
fn read_product_domains(product_local: &str) -> R<Vec<String>> {
    let product = format!("{}{}", NS, product_local);
    let q = format!(
        "PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <{g}> {{ <{p}> chorus:hasDomain ?d BIND(REPLACE(STR(?d), '.*[#/]', '') AS ?v) }} }} ORDER BY ?v",
        ns = NS, g = ONTOLOGY_GRAPH, p = product
    );
    let mut ds = select_v(&sparql_json(&q)?);
    ds.sort();
    ds.dedup();
    Ok(ds)
}

/// #3488 — generate the product API index from the model (the bind, derived from
/// the product's hasDomain edges). The product surface auto-mounts its domains.
pub fn generate_product_index(product_local: &str) -> R<String> {
    let domains = read_product_domains(product_local)?;
    let refs: Vec<&str> = domains.iter().map(String::as_str).collect();
    Ok(project_product_index(product_local, &refs))
}

/// #3494 — read the OWL classes a DOMAIN governs via `chorus:definesVocabulary`
/// (the VOCABULARY edge — what classes this domain's API serves — distinct from
/// partOf/contains CONTAINMENT). Multi-valued: a domain may define several classes
/// (properties → Property, PropertyKey). The per-class generator (#3454) fans out
/// over these. Localnames, sorted, de-duped. Same graph + shape as
/// `read_product_domains` (the hasDomain bind), so the vocab bind reads the model
/// the same way the containment bind does.
fn read_defines_vocabulary(domain_local: &str) -> R<Vec<String>> {
    let domain = format!("{}{}", NS, domain_local);
    let q = format!(
        "PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <{g}> {{ <{d}> chorus:definesVocabulary ?c BIND(REPLACE(STR(?c), '.*[#/]', '') AS ?v) }} }} ORDER BY ?v",
        ns = NS, g = ONTOLOGY_GRAPH, d = domain
    );
    let mut cs = select_v(&sparql_json(&q)?);
    cs.sort();
    cs.dedup();
    Ok(cs)
}

/// #3494 — pure: project a DOMAIN's vocabulary surface index from the classes it
/// `definesVocabulary`. Mirrors `project_product_index` (the product→domain bind):
/// the domain's API is the aggregate of the per-class surfaces its vocabulary
/// classes generate. Each class carries its API mount (the pluralized route root).
/// Names lowercased, sorted, de-duped. Zero classes → an empty `vocab` array (no
/// phantom surface, AC4). Pure; the SPARQL read is integration-proven separately.
pub fn project_domain_vocab_index(domain: &str, classes: &[&str]) -> String {
    let mut cs: Vec<String> = classes
        .iter()
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .collect();
    cs.sort();
    cs.dedup();
    let items: Vec<String> = cs
        .iter()
        .map(|c| format!("{{ \"class\": \"{}\", \"api\": \"/{}\" }}", c, pluralize(c)))
        .collect();
    format!(
        "{{ \"domain\": \"{}\", \"vocab\": [{}] }}",
        domain.trim().to_lowercase(),
        items.join(", ")
    )
}

/// #3494 — enumerate EVERY class any domain `definesVocabulary` across the model
/// (distinct, sorted). The serve fan-out uses this to mount every vocabulary
/// surface on one origin: each class generates its #3454 CRUD table and dispatches
/// via the existing `select_table`. Zero edges → empty Vec (serve adds nothing).
pub fn all_vocab_classes() -> R<Vec<String>> {
    let q = format!(
        "PREFIX chorus: <{ns}> SELECT DISTINCT ?v WHERE {{ GRAPH <{g}> {{ ?d chorus:definesVocabulary ?c BIND(REPLACE(STR(?c), '.*[#/]', '') AS ?v) }} }} ORDER BY ?v",
        ns = NS, g = ONTOLOGY_GRAPH
    );
    let mut cs = select_v(&sparql_json(&q)?);
    cs.sort();
    cs.dedup();
    Ok(cs)
}

/// #3494 — FAN-OUT: enumerate a domain's `definesVocabulary` classes and run the
/// EXISTING per-class generator (#3454) on each, composing the domain's vocabulary
/// surface from one edge set — no new per-class machinery. A domain with zero
/// `definesVocabulary` edges yields an EMPTY Vec (no surface, no crash, no phantom
/// route — AC4). This is the API-surface case of "the whole model renders as a
/// projection": every domain that declares vocabulary gets its CRUD surface
/// projected from that single edge, never hand-written.
pub fn generate_domain_vocab(domain_local: &str) -> R<Vec<RouteTable>> {
    let classes = read_defines_vocabulary(domain_local)?;
    let mut tables = Vec::with_capacity(classes.len());
    for class in classes {
        tables.push(generate(&class)?);
    }
    Ok(tables)
}

/// GENERATE — read the shape's direct-path properties for `class` from the
/// ontology graph and derive the route table.
pub fn generate(class_local: &str) -> R<RouteTable> {
    adr040_check(class_local, &[])?; // refuse before touching the store
    let class = format!("{}{}", NS, class_local);
    // fields WITH their kind: name|datatype:<xsd> or name|edge:<Class> or name|plain
    let q = format!(
        "PREFIX sh: <http://www.w3.org/ns/shacl#> SELECT ?v WHERE {{ GRAPH <{g}> {{ ?s sh:targetClass <{c}> ; sh:property ?p . ?p sh:path ?path . FILTER(isIRI(?path)) OPTIONAL {{ ?p sh:datatype ?dt }} OPTIONAL {{ ?p sh:class ?cl }} BIND(CONCAT(REPLACE(STR(?path), '.*#', ''), '|', COALESCE(CONCAT('datatype:', REPLACE(STR(?dt), '.*#', '')), CONCAT('edge:', REPLACE(STR(?cl), '.*#', '')), 'plain')) AS ?v) }} }} ORDER BY ?v",
        g = ONTOLOGY_GRAPH, c = class
    );
    let body = sparql_json(&q)?;
    let mut fields: Vec<String> = select_v(&body);
    fields.sort();
    fields.dedup();
    if fields.is_empty() {
        return Err(format!("no shape found for {} in {} — land the schema first", class, ONTOLOGY_GRAPH));
    }
    adr040_check(class_local, &fields)?; // shape-sourced fields obey the law too
    let plural = pluralize(class_local);
    let mut routes = vec![
        format!("GET /{}", plural),
        format!("GET /{}/:name", plural),
        format!("GET /{}/:name/contains", plural),
        format!("GET /{}/:name/partof", plural),
        format!("GET /{}/:name/has-child", plural),
        format!("GET /{}/:name/completeness", plural), // #3468 — model-driven completeness gauge (unsecured read)
        format!("GET /schema/{}", class_local.to_lowercase()),
    ];
    // #3454 AC1 — the generated WRITE routes (POST/PUT/DELETE per edge), folded
    // into the served contract: routes.json lists them and openapi_json (now
    // method-aware) advertises them. serve() dispatches non-GET to handle_write
    // (authN → authZ-from-ownedBy → shape-rejection → SPARQL-UPDATE → spine →
    // typed status). A new edge type yields its write routes automatically.
    routes.extend(write_routes(&plural));
    // #3414 — MODEL-DRIVEN secured-set: query whether THIS class's shape carries the
    // auth annotation (`chorus:requiresAuth true`) and PROJECT the guard from it —
    // replacing #3402's hardcoded `is_secured` constant. No annotation = open (AC3:
    // undeclared surfaces stay open; mixed-state by construction). Term PROVISIONAL
    // pending Silas's OWL-DBA blessing (a one-line constant + the shape annotation).
    let aq = format!(
        "PREFIX sh: <http://www.w3.org/ns/shacl#> PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <{g}> {{ ?s sh:targetClass <{c}> ; chorus:requiresAuth ?ra . FILTER(?ra) BIND('secured' AS ?v) }} }}",
        ns = NS, g = ONTOLOGY_GRAPH, c = class
    );
    let annotated = !select_v(&sparql_json(&aq)?).is_empty();
    let secured = project_secured(class_local, annotated);
    // #3468 — the required DATATYPE sections, read from the SAME source the DAL
    // (chorus-model::read_shape) enforces: sh:minCount >= 1. This is owl-api's
    // READ-ONLY completeness GAUGE (the migration thermometer) — it MEASURES how
    // far an instance sits below the floor; the floor itself is ENFORCED at write
    // by the DAL, not here (owl-api is read-only; writes delegate to the DAL).
    // Edge properties (sh:class: ownedBy/atStep/membership) are excluded so the
    // gauge measures the prose sections, not edges that have their own write paths.
    let mq = format!(
        "PREFIX sh: <http://www.w3.org/ns/shacl#> SELECT ?v WHERE {{ GRAPH <{g}> {{ ?s sh:targetClass <{c}> ; sh:property ?p . ?p sh:path ?path ; sh:minCount ?mc . FILTER(?mc >= 1) FILTER(isIRI(?path)) FILTER NOT EXISTS {{ ?p sh:class ?cl }} BIND(REPLACE(STR(?path), '.*#', '') AS ?v) }} }} ORDER BY ?v",
        g = ONTOLOGY_GRAPH, c = class
    );
    let mut mandatory: Vec<String> = select_v(&sparql_json(&mq)?);
    mandatory.sort();
    mandatory.dedup();
    // #3488 — resolve the repo land location as a PROJECTION of the class's
    // containment chain (ADR-041 recursive tree: <vs-step>/products/<product>/
    // domains/<domain>). chorus:repoTarget is the explicit override; otherwise
    // we walk the class's containment edges (best-effort: vs-step via atStep,
    // product via partOf) and project. Absent levels are skipped, so a partly
    // modeled class still lands deterministically. Localnames strip the IRI
    // prefix and any kind-tag (value-stream-step-designing → designing).
    let rq = format!(
        "PREFIX sh: <http://www.w3.org/ns/shacl#> PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <{g}> {{ ?s sh:targetClass <{c}> ; chorus:repoTarget ?v }} }}",
        ns = NS, g = ONTOLOGY_GRAPH, c = class
    );
    let declared = select_v(&sparql_json(&rq)?).into_iter().next();
    let step = read_containment_local(&class, "chorus:atStep", "value-stream-step-")?;
    let product = read_containment_local(&class, "chorus:partOf", "")?;
    let mut chain: Vec<(RepoKind, &str)> = Vec::new();
    if let Some(s) = step.as_deref() {
        chain.push((RepoKind::ValueStream, s));
    }
    if let Some(p) = product.as_deref() {
        chain.push((RepoKind::Product, p));
    }
    chain.push((RepoKind::Domain, class_local));
    let repo_target = resolve_repo_target(declared.as_deref(), &chain);
    // #3506 / ADR-048 §3 — PROJECT field-exposure: each shape property's
    // chorus:exposure level (public|internal|secret). Generated, not hand-authored;
    // a property with no chorus:exposure simply doesn't appear here → hidden by the
    // fail-closed default in field_exposed(). One row per (field, level).
    let eq = format!(
        "PREFIX sh: <http://www.w3.org/ns/shacl#> PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <{g}> {{ ?s sh:targetClass <{c}> ; sh:property ?p . ?p sh:path ?path ; chorus:exposure ?ex . FILTER(isIRI(?path)) BIND(CONCAT(REPLACE(STR(?path), '.*#', ''), '|', REPLACE(STR(?ex), '.*#', '')) AS ?v) }} }} ORDER BY ?v",
        ns = NS, g = ONTOLOGY_GRAPH, c = class
    );
    let exposure: Vec<(String, String)> = select_v(&sparql_json(&eq)?)
        .into_iter()
        .filter_map(|row| row.split_once('|').map(|(f, l)| (f.to_string(), l.to_string())))
        .collect();
    Ok(RouteTable { class, fields, routes, secured, mandatory, repo_target, exposure })
}

/// #3454 AC1 — the WRITE routes generated per edge, mirroring the read routes.
/// POST creates an entity or adds an edge, PUT replaces an entity, DELETE removes
/// an entity or edge. Generated from the same plural/edge vocabulary as the read
/// routes, so a new edge type yields its write routes automatically. Pure +
/// unit-pinned. (The live execution + authZ + shape-rejection are the handler
/// increment; this is the contract.)
pub fn write_routes(plural: &str) -> Vec<String> {
    vec![
        format!("POST /{}", plural),                  // create entity
        format!("PUT /{}/:name", plural),             // replace entity
        format!("DELETE /{}/:name", plural),          // delete entity
        format!("POST /{}/:name/partof", plural),     // add partOf edge
        format!("DELETE /{}/:name/partof", plural),   // remove partOf edge
        format!("POST /{}/:name/contains", plural),   // add contains edge
        format!("DELETE /{}/:name/contains", plural), // remove contains edge
        format!("POST /{}/:name/has-child", plural),  // add has-child edge
        format!("DELETE /{}/:name/has-child", plural),// remove has-child edge
    ]
}

/// #3454 AC5 — the typed write-error taxonomy. ONE place maps a write outcome to
/// an HTTP status, so no route can "return 200 from the read handler" for a
/// malformed/unauthorized write. Pure so the contract is unit-pinned; the live
/// handler maps its outcomes through this. 501 = generated-not-yet-executing
/// (the honest interim — a write is gated + typed, never silently a read).
pub fn write_status(outcome: &str) -> (u16, &'static str) {
    match outcome {
        "ok" => (200, "ok"),
        "created" => (201, "created"),
        "authn-missing" => (401, "authn-missing"),   // no/invalid credential
        "authz" => (403, "authz"),                   // not the owning role (ownedBy)
        "conflict" => (409, "conflict"),             // e.g. 2nd parent on single-valued partOf
        "validation" => (422, "validation"),         // malformed / shape violation
        "not-found" => (404, "not-found"),           // entity/edge target absent
        _ => (501, "not-implemented"),               // generated, execution not yet wired (no fail-open)
    }
}

// === #3454 — the generated WRITE layer (POST/PUT/DELETE per edge) ===========
//
// authN (verify_token) + authZ (ownedBy == caller role, FAIL-CLOSED) + shape
// rejection (single-parent partOf → 409) + typed errors (write_status) + a spine
// event per write — all in ONE generated path, so a write can't forget to auth,
// validate, or log. Pure decision/builders are unit-tested; the I/O wraps them.

/// The caller's ROLE from the verified token's webId. The static webid format is
/// `…/_agents/<role>/profile/card.ttl#me` (auth::chorus_agent_webids). Returns the
/// `<role>` segment, or None if the shape doesn't match (→ authZ fail-closed).
pub fn role_from_webid(web_id: &str) -> Option<String> {
    let after = web_id.split("/_agents/").nth(1)?;
    let role = after.split('/').next()?;
    if role.is_empty() { None } else { Some(role.to_string()) }
}

#[derive(Debug, PartialEq, Eq)]
pub enum WriteOp {
    CreateEntity,
    ReplaceEntity { name: String },
    DeleteEntity { name: String },
    AddEdge { name: String, edge: String },
    RemoveEdge { name: String, edge: String },
}

/// Parse a generated write route into the operation it denotes. Mirrors the
/// read-route shapes; returns None for anything not a known write route (→ 404).
pub fn parse_write(method: &str, path: &str, plural: &str) -> Option<WriteOp> {
    let p = path.split(['?', '#']).next().unwrap_or(path);
    let parts: Vec<&str> = p.trim_matches('/').split('/').filter(|s| !s.is_empty()).collect();
    if parts.first().map(|s| *s) != Some(plural) {
        return None;
    }
    match (method, parts.len()) {
        ("POST", 1) => Some(WriteOp::CreateEntity),
        ("PUT", 2) => Some(WriteOp::ReplaceEntity { name: parts[1].to_string() }),
        ("DELETE", 2) => Some(WriteOp::DeleteEntity { name: parts[1].to_string() }),
        ("POST", 3) => Some(WriteOp::AddEdge { name: parts[1].to_string(), edge: parts[2].to_string() }),
        ("DELETE", 3) => Some(WriteOp::RemoveEdge { name: parts[1].to_string(), edge: parts[2].to_string() }),
        _ => None,
    }
}

/// The OWL predicate local-name for a write edge segment. None = unknown edge
/// (→ validation 422). The single-valued one (partOf, a FunctionalProperty per
/// #3450) is flagged so the handler enforces single-parent.
pub fn edge_predicate(edge: &str) -> Option<&'static str> {
    match edge {
        "partof" => Some("partOf"),
        "contains" => Some("contains"),
        "has-child" => Some("hasChild"),
        _ => None,
    }
}

/// partOf is the single-valued (FunctionalProperty) edge — a 2nd parent is a 409.
pub fn edge_is_single_valued(edge: &str) -> bool {
    edge == "partof"
}

/// AuthZ: the caller may write a node's edges ONLY if they OWN it. FAIL-CLOSED —
/// an absent ownedBy (None) denies (the #3414 fail-closed lesson; Silas backfills
/// coverage). Pure + unit-tested.
pub fn authz_allows(caller_role: &str, owned_by: Option<&str>) -> bool {
    matches!(owned_by, Some(o) if !o.is_empty() && o == caller_role)
}

/// Extract a JSON string field by key: { "<key>": "<value>" }. Minimal zero-dep;
/// values are validated (is_safe_local for names) or SPARQL-literal-escaped
/// (sparql_lit for property values) downstream, so escape-handling isn't here.
pub fn json_field(body: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\"", key);
    let i = body.find(&needle)? + needle.len();
    let after_colon = &body[i..][body[i..].find(':')? + 1..];
    let q = after_colon.find('"')? + 1;
    let val = &after_colon[q..];
    let end = val.find('"')?;
    Some(val[..end].to_string())
}

/// The edge target name from a write body: { "target": "<name>" }.
pub fn parse_body_target(body: &str) -> Option<String> {
    json_field(body, "target")
}


/// The shape's DATATYPE/plain fields present in the body, as (field, value) pairs.
/// Edge fields (edge:*) are skipped — edges are written through the edge endpoints,
/// not the entity body. Pure + unit-tested.
pub fn collect_entity_props(body: &str, fields: &[String]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for f in fields {
        let (name, kind) = f.split_once('|').unwrap_or((f.as_str(), "plain"));
        if kind.starts_with("edge:") {
            continue;
        }
        if let Some(v) = json_field(body, name) {
            out.push((name.to_string(), v));
        }
    }
    out
}

/// #3468 — the completeness FLOOR decision: which mandatory sections are ABSENT
/// from the provided props. An EMPTY value counts as absent (a blank section is
/// not a present section). Order follows `mandatory` so the 422 message is stable.
/// Pure + unit-pinned — the gate's verdict is tested without a graph.
pub fn missing_mandatory(present: &[(String, String)], mandatory: &[String]) -> Vec<String> {
    mandatory
        .iter()
        .filter(|m| !present.iter().any(|(n, v)| n == *m && !v.trim().is_empty()))
        .cloned()
        .collect()
}

/// #3468 — completeness as the MIGRATION GAUGE (AC4): (met, pct 0..=100, present,
/// missing). MEASURES how far an instance sits below the 100% floor — never blocks
/// a read, never a fill-target (thermometer). A shape with no mandatory set is
/// vacuously 100% complete. Pure + unit-pinned.
pub fn completeness(present: &[(String, String)], mandatory: &[String]) -> (bool, u8, Vec<String>, Vec<String>) {
    let missing = missing_mandatory(present, mandatory);
    let total = mandatory.len();
    let have = total.saturating_sub(missing.len());
    let pct = if total == 0 { 100 } else { ((have * 100) / total) as u8 };
    let present_names: Vec<String> = mandatory.iter().filter(|m| !missing.contains(m)).cloned().collect();
    (missing.is_empty(), pct, present_names, missing)
}

// #3468 — owl-api's raw-SPARQL write builders (build_create_entity /
// build_replace_entity / build_edge_update) and sparql_update were RETIRED: every
// write now delegates to the DAL (chorus-model), the one governed write path.
// owl-api is read-only over Fuseki again, per its Cargo.toml contract.

/// #3468 — DELEGATE a create to the DAL (chorus-model) — the ONE governed write
/// path. Shells to the DAL CLI (the same subprocess pattern owl-api uses for curl);
/// the DAL enforces the completeness floor (sh:minCount, fail-closed), mints the
/// IRI, validates sh:in enums + referential integrity, and stamps the audit/spine
/// witness. `ownedBy` is passed as a field literal so owl-api's authZ reads it back
/// consistently; DEPLOY_ROLE=caller stamps the creator. Returns the DAL's typed
/// refusal text on failure (mapped onto the write taxonomy by the caller).
fn dal_run(args: &[String], caller: &str) -> R<()> {
    let bin = std::env::var("CHORUS_MODEL_BIN").unwrap_or_else(|_| "chorus-model".to_string());
    let out = Command::new(&bin)
        .args(args)
        .env("DEPLOY_ROLE", caller)
        .output()
        .map_err(|e| format!("dal-spawn: {}", e))?;
    if out.status.success() {
        return Ok(());
    }
    // The DAL prints its typed refusal (shape-violation / unknown-endpoint /
    // not-found / …) to stderr+stdout; surface whichever carries it so the caller
    // can map it onto the write taxonomy.
    let err = String::from_utf8_lossy(&out.stderr);
    let msg = if err.trim().is_empty() { String::from_utf8_lossy(&out.stdout).trim().to_string() } else { err.trim().to_string() };
    Err(msg)
}

/// Create/replace an entity via the DAL `add` (full governed upsert: floor + mint
/// + audit). ownedBy is a field literal owl-api's authZ reads back; DEPLOY_ROLE
/// stamps the creator.
fn dal_add(kind: &str, name: &str, caller: &str, props: &[(String, String)]) -> R<()> {
    let mut args: Vec<String> = vec![
        "add".into(), "--kind".into(), kind.to_string(), "--name".into(), name.to_string(),
        "--field".into(), format!("ownedBy={}", caller),
    ];
    for (f, v) in props {
        args.push("--field".into());
        args.push(format!("{}={}", f, v));
    }
    dal_run(&args, caller)
}

/// Delete an entity via the DAL `delete` (governed, fail-closed, witnessed).
fn dal_delete(kind: &str, name: &str, caller: &str) -> R<()> {
    dal_run(&["delete".into(), "--kind".into(), kind.to_string(), "--name".into(), name.to_string()], caller)
}

/// Add/remove one edge via the DAL `link`/`unlink` (incremental + referential
/// integrity + witness). The structural edges (partOf/contains/hasChild) connect
/// bare-kind entities (Domain/Product), so the subject kind mints the target IRI
/// identically (mint is kind-independent for bare kinds).
fn dal_edge(insert: bool, kind: &str, name: &str, prop: &str, tname: &str, caller: &str) -> R<()> {
    let verb = if insert { "link" } else { "unlink" };
    dal_run(&[verb.into(), "--kind".into(), kind.to_string(), "--name".into(), name.to_string(),
              "--edge".into(), format!("{}={}:{}", prop, kind, tname)], caller)
}

/// Map a DAL refusal string onto owl-api's typed write response.
fn dal_err_resp(e: &str) -> (u16, String) {
    if e.contains("shape-violation") || e.contains("unknown-endpoint") || e.contains("unknown-target") || e.contains("bad-property") {
        write_resp("validation", e)
    } else if e.contains("not-found") {
        write_resp("not-found", e)
    } else {
        (502, format!("{{ \"error\": \"dal\", \"message\": \"{}\" }}", json_escape(e)))
    }
}

/// Query the ownedBy role of an entity (for authZ). None = no ownedBy on record →
/// authz_allows fails closed.
fn query_owned_by(entity: &str) -> Option<String> {
    let q = format!(
        "PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <{g}> {{ <{ns}{e}> chorus:ownedBy ?o . BIND(REPLACE(STR(?o), '.*[#/]', '') AS ?v) }} }}",
        ns = NS, g = INSTANCES_GRAPH, e = entity
    );
    sparql_json(&q).ok().and_then(|b| select_v(&b).into_iter().next())
}

/// Does the entity already have a partOf parent? (single-parent → 2nd add is 409).
fn partof_exists(entity: &str) -> bool {
    let q = format!(
        "PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <{g}> {{ <{ns}{e}> chorus:partOf ?p . BIND('y' AS ?v) }} }}",
        ns = NS, g = INSTANCES_GRAPH, e = entity
    );
    sparql_json(&q).map(|b| !select_v(&b).is_empty()).unwrap_or(false)
}

/// Does the entity exist at all? (create → 409 if it does; replace → 404 if it doesn't).
fn entity_exists(entity: &str) -> bool {
    let q = format!(
        "SELECT ?v WHERE {{ GRAPH <{g}> {{ <{ns}{e}> ?p ?o . BIND('y' AS ?v) }} }} LIMIT 1",
        ns = NS, g = INSTANCES_GRAPH, e = entity
    );
    sparql_json(&q).map(|b| !select_v(&b).is_empty()).unwrap_or(false)
}

// (created/modified stamping now lives in the DAL's audit envelope — owl-api's
// own now_stamp was retired with the raw create path, #3468.)

/// Emit the per-write spine event (AC4): who / what / which-edge / when. Uniform,
/// best-effort like the read telemetry — a write is never silent. Resolves the
/// chorus-log path from CHORUS_HOME (the daemon-PATH lesson, #3151).
fn emit_write_spine(caller: &str, op: &str, entity: &str, edge: &str, result: &str) {
    let home = std::env::var("CHORUS_HOME")
        .unwrap_or_else(|_| format!("{}/CascadeProjects/chorus", std::env::var("HOME").unwrap_or_default()));
    let log = format!("{}/platform/scripts/chorus-log", home);
    let _ = Command::new("bash").args([
        log.as_str(), "owl.write", caller,
        &format!("op={}", op), &format!("entity={}", entity),
        &format!("edge={}", edge), &format!("result={}", result),
    ]).output();
}

/// Map a write outcome tag to a typed JSON error/ok response (AC5 — one place,
/// no silent 200). write_status owns the code; this owns the body shape.
fn write_resp(tag: &str, message: &str) -> (u16, String) {
    let (code, t) = write_status(tag);
    let key = if code < 400 { "status" } else { "error" };
    (code, format!("{{ \"{}\": \"{}\", \"message\": \"{}\" }}", key, t, json_escape(message)))
}

/// #3454 — the generated write handler. authZ (ownedBy, fail-closed) → shape
/// rejection (single-parent partOf → 409) → SPARQL-UPDATE execution → spine event,
/// every outcome typed via write_status. authN is done by serve() before this is
/// called (caller_role = the verified token's role). Entity create/replace return
/// a typed 501 (the next slice: full-property-body shape validation); edge add/
/// remove + entity delete are live.
pub fn handle_write(method: &str, path: &str, body: &str, table: &RouteTable, caller_role: &str) -> (u16, String) {
    let class_local = table.class.rsplit('#').next().unwrap_or("");
    let plural = pluralize(class_local);
    let op = match parse_write(method, path, &plural) {
        Some(o) => o,
        None => return write_resp("not-found", "no such write route"),
    };
    // entity name (None for CreateEntity) + injection-safety
    let entity: Option<String> = match &op {
        WriteOp::CreateEntity => None,
        WriteOp::ReplaceEntity { name }
        | WriteOp::DeleteEntity { name }
        | WriteOp::AddEdge { name, .. }
        | WriteOp::RemoveEdge { name, .. } => Some(name.clone()),
    };
    if let Some(e) = &entity {
        if !is_safe_local(e) {
            return write_resp("validation", "invalid entity name");
        }
        // AC3 authZ — only the owning role writes this node's edges (fail-closed).
        let owned = query_owned_by(e);
        if !authz_allows(caller_role, owned.as_deref()) {
            emit_write_spine(caller_role, method, e, "", "authz");
            return write_resp("authz", "only the owning role may write this node (ownedBy)");
        }
    }
    match &op {
        WriteOp::AddEdge { name, edge } | WriteOp::RemoveEdge { name, edge } => {
            let pred = match edge_predicate(edge) {
                Some(p) => p,
                None => return write_resp("validation", "unknown edge type"),
            };
            let target = match parse_body_target(body) {
                Some(t) => t,
                None => return write_resp("validation", "missing 'target' in request body"),
            };
            if !is_safe_local(&target) {
                return write_resp("validation", "invalid target name");
            }
            let insert = matches!(op, WriteOp::AddEdge { .. });
            // AC2 — single-parent partOf: a 2nd parent is a 409, never silently accepted.
            if insert && edge_is_single_valued(edge) && partof_exists(name) {
                emit_write_spine(caller_role, "add-edge", name, edge, "conflict");
                return write_resp("conflict", "partOf is single-valued: node already has a parent");
            }
            // #3468 — DELEGATE to the DAL (link/unlink): incremental edge write with
            // referential integrity + witness. Replaces the raw build_edge_update +
            // sparql_update path so edges ride the ONE governed write path too.
            let kind = class_local.to_lowercase();
            match dal_edge(insert, &kind, name, pred, &target, caller_role) {
                Ok(_) => {
                    let verb = if insert { "add-edge" } else { "remove-edge" };
                    emit_write_spine(caller_role, verb, name, edge, "ok");
                    write_resp("ok", &format!("{} {} {} -> {} (via DAL)", verb, name, edge, target))
                }
                Err(e) => {
                    emit_write_spine(caller_role, "edge", name, edge, "error");
                    dal_err_resp(&e)
                }
            }
        }
        WriteOp::DeleteEntity { name } => {
            // #3468 — DELEGATE to the DAL `delete` (governed, fail-closed, witnessed).
            let kind = class_local.to_lowercase();
            match dal_delete(&kind, name, caller_role) {
                Ok(_) => {
                    emit_write_spine(caller_role, "delete-entity", name, "", "ok");
                    write_resp("ok", &format!("deleted {} (via DAL)", name))
                }
                Err(e) => {
                    emit_write_spine(caller_role, "delete-entity", name, "", "error");
                    dal_err_resp(&e)
                }
            }
        }
        WriteOp::CreateEntity => {
            // CREATE: name from the body; the authenticated caller becomes the owner
            // (you own what you create) — no prior ownedBy to check, so the top-level
            // authZ block (entity=None here) is correctly skipped; authN was enforced
            // by serve(). 409 if the entity already exists.
            let name = match json_field(body, "name") {
                Some(n) => n,
                None => return write_resp("validation", "create requires a 'name' in the body"),
            };
            if !is_safe_local(&name) {
                return write_resp("validation", "invalid entity name");
            }
            if entity_exists(&name) {
                emit_write_spine(caller_role, "create", &name, "", "conflict");
                return write_resp("conflict", "entity already exists");
            }
            let props = collect_entity_props(body, &table.fields);
            // #3468 — DELEGATE THE WRITE TO THE DAL (chorus-model). owl-api is
            // read-only by contract; the DAL is the ONE governed write path. It
            // enforces the completeness FLOOR (sh:minCount, fail-closed), mints the
            // IRI (ADR-040), checks sh:in enums + referential integrity, and stamps
            // the audit/spine witness — none of which a raw SPARQL write does. The
            // old build_create_entity + sparql_update path was a competing impl that
            // bypassed all of it (and contradicted owl-api's own read-only contract).
            // ownedBy is passed as a field literal (matches owl-api's authZ read) and
            // DEPLOY_ROLE=caller stamps the creator.
            let kind = class_local.to_lowercase();
            match dal_add(&kind, &name, caller_role, &props) {
                Ok(_) => {
                    emit_write_spine(caller_role, "create", &name, "", "ok");
                    write_resp("created", &format!("created {} via DAL (ownedBy {})", name, caller_role))
                }
                Err(e) => {
                    let outcome = if e.contains("shape-violation") { "incomplete" } else { "error" };
                    emit_write_spine(caller_role, "create", &name, "", outcome);
                    dal_err_resp(&e)
                }
            }
        }
        WriteOp::ReplaceEntity { name } => {
            // REPLACE: authZ (ownedBy == caller) already enforced in the entity block
            // above. Must exist (404 otherwise).
            if !entity_exists(name) {
                return write_resp("not-found", "entity does not exist");
            }
            let props = collect_entity_props(body, &table.fields);
            if props.is_empty() {
                return write_resp("validation", "replace requires at least one shape property in the body");
            }
            // #3468 — DELEGATE to the DAL `add` (idempotent full upsert). NOTE: the
            // DAL is single-writer full-replace by design (#3345) — a replace must
            // restate the COMPLETE entity (the floor re-applies; omitted edges/fields
            // are not preserved). This unifies replace onto the DAL's one write
            // semantic rather than owl-api's prior partial-update (a competing impl).
            let kind = class_local.to_lowercase();
            match dal_add(&kind, name, caller_role, &props) {
                Ok(_) => {
                    emit_write_spine(caller_role, "replace", name, "", "ok");
                    write_resp("ok", &format!("replaced {} via DAL ({} props)", name, props.len()))
                }
                Err(e) => {
                    emit_write_spine(caller_role, "replace", name, "", "error");
                    dal_err_resp(&e)
                }
            }
        }
    }
}

/// dashboards.json — the observability config as a GENERATED artifact

/// dashboards.json — the observability config as a GENERATED artifact
/// (#3354: regenerate-not-reload applies to observability too). Emitted
/// beside routes.json; file-drops into shared-observability/dashboards/
/// where Grafana's provisioning picks it up within 30s. Panels derive from
/// the class + the telemetry envelope: rate, latency, typed-status split,
/// the silent-broken-chain watch (count:0 + ok).
pub fn dashboards_json(t: &RouteTable) -> String {
    let class_l = t.class.rsplit('#').next().unwrap_or("domain").to_lowercase();
    let class_short = t.class.rsplit('#').next().unwrap_or("").to_string();
    // LogQL line filters use BACKTICK literals — no quote escaping inside JSON.
    let q_all = format!("{{{{job=\"werk-verbs\"}}}} |= `api.request.served` |= `\"class\":\"{}\"`", class_short);
    let q_err = format!("{} |= `\"status\":\"error`", q_all);
    let q_chain = format!("{} |= `\"result_count\":0` |= `\"status\":\"ok\"`", q_all);
    format!(
        r#"{{
  "annotations": {{ "list": [] }},
  "editable": true,
  "id": null,
  "panels": [
    {{ "type": "row", "gridPos": {{ "h": 1, "w": 24, "x": 0, "y": 0 }}, "id": 1,
      "title": "owl-api — generated {class_l} API (generated dashboard — do not hand-edit)" }},
    {{ "type": "logs", "datasource": {{ "type": "loki", "uid": "loki" }},
      "gridPos": {{ "h": 8, "w": 24, "x": 0, "y": 1 }}, "id": 2,
      "title": "requests (telemetry envelope)",
      "targets": [ {{ "expr": "{q_all}", "refId": "A" }} ] }},
    {{ "type": "logs", "datasource": {{ "type": "loki", "uid": "loki" }},
      "gridPos": {{ "h": 6, "w": 12, "x": 0, "y": 9 }}, "id": 3,
      "title": "errors (typed — refusals excluded)",
      "targets": [ {{ "expr": "{q_err}", "refId": "A" }} ] }},
    {{ "type": "logs", "datasource": {{ "type": "loki", "uid": "loki" }},
      "gridPos": {{ "h": 6, "w": 12, "x": 12, "y": 9 }}, "id": 4,
      "title": "silent-broken-chain watch (ok + result_count:0)",
      "targets": [ {{ "expr": "{q_chain}", "refId": "A" }} ] }}
  ],
  "refresh": "30s",
  "schemaVersion": 38,
  "time": {{ "from": "now-6h", "to": "now" }},
  "title": "OWL API — {class_l}",
  "uid": "owl-api-{class_l}",
  "version": 1
}}
"#,
        class_l = class_l,
        q_all = q_all.replace('"', "\\\""),
        q_err = q_err.replace('"', "\\\""),
        q_chain = q_chain.replace('"', "\\\"")
    )
}

/// OpenAPI 3.1 contract (#3364 AC2, #3520) — generated from the same shapes as the
/// routes, in the same pass. The spec IS the api docs AND the validation
/// contract: the conformance walker validates live responses against it, and
/// it's committed as a drift baseline beside routes.json. Deterministic.
pub fn openapi_json(t: &RouteTable) -> String {
    let class_short = t.class.rsplit('#').next().unwrap_or("").to_string();
    let class_l = class_short.to_lowercase();
    let mut props: Vec<String> = vec![
        "\"iri\": { \"type\": \"string\" }".into(),
        "\"created\": { \"type\": \"string\" }".into(),
        "\"creator\": { \"type\": \"string\" }".into(),
        "\"modified\": { \"type\": \"string\" }".into(),
        "\"type\": { \"$ref\": \"#/components/schemas/EdgeRef\" }".into(),
    ];
    for f in &t.fields {
        let (name, kind) = f.split_once('|').unwrap_or((f.as_str(), "plain"));
        let schema = if kind.starts_with("edge:") {
            "{ \"$ref\": \"#/components/schemas/EdgeRef\" }".to_string()
        } else {
            // datatype:* and plain both serialize as JSON strings today
            "{ \"type\": \"string\" }".to_string()
        };
        props.push(format!("\"{}\": {}", name, schema));
    }
    props.sort();
    // #3454 — method-aware: group operations by path so a path with both a GET
    // (read) and POST/PUT/DELETE (generated write) emits one path object with
    // multiple operation keys (valid OpenAPI). Writes document the typed-error
    // taxonomy (write_status) + the {target} requestBody.
    let mut by_path: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    for r in &t.routes {
        let (method, raw) = r.split_once(' ').unwrap_or(("GET", r.as_str()));
        let p = raw.replace(":name", "{name}");
        let m = method.to_ascii_lowercase();
        let op = if m == "get" {
            let (resp, params) = if p.ends_with("/{name}") {
                (format!("#/components/schemas/{}", class_short), NAME_PARAM)
            } else if p.contains("{name}") {
                ("#/components/schemas/Fold".to_string(), NAME_PARAM)
            } else if p.starts_with("/schema") {
                ("#/components/schemas/Schema".to_string(), "")
            } else {
                ("#/components/schemas/List".to_string(), "")
            };
            format!(
                "\"get\": {{ {}\"responses\": {{ \"200\": {{ \"description\": \"ok\", \"content\": {{ \"application/json\": {{ \"schema\": {{ \"$ref\": \"{}\" }} }} }} }}, \"404\": {{ \"description\": \"typed refusal\" }} }} }}",
                params, resp
            )
        } else {
            let params = if p.contains("{name}") { NAME_PARAM } else { "" };
            format!(
                "\"{}\": {{ {}\"requestBody\": {{ \"content\": {{ \"application/json\": {{ \"schema\": {{ \"type\": \"object\", \"properties\": {{ \"target\": {{ \"type\": \"string\" }} }} }} }} }} }}, \"responses\": {{ \"200\": {{ \"description\": \"ok\" }}, \"201\": {{ \"description\": \"created\" }}, \"401\": {{ \"description\": \"authn-missing\" }}, \"403\": {{ \"description\": \"authz (ownedBy)\" }}, \"409\": {{ \"description\": \"conflict (single-parent partOf)\" }}, \"422\": {{ \"description\": \"validation\" }}, \"404\": {{ \"description\": \"not-found\" }} }} }}",
                m, params
            )
        };
        by_path.entry(p).or_default().push(op);
    }
    let paths: String = by_path
        .iter()
        .map(|(p, ops)| {
            let mut o = ops.clone();
            o.sort();
            format!("    \"{}\": {{ {} }}", p, o.join(", "))
        })
        .collect::<Vec<_>>()
        .join(",\n");
    // #3520 — project the completeness floor (t.mandatory, sh:minCount≥1) as the
    // OpenAPI `required` array; optional fields are expressed by omission, not null,
    // so we emit no `nullable` (3.1-clean by construction).
    let required = if t.mandatory.is_empty() {
        String::new()
    } else {
        format!(
            ", \"required\": [{}]",
            t.mandatory.iter().map(|m| format!("\"{}\"", m)).collect::<Vec<_>>().join(", ")
        )
    };
    format!(
        "{{\n  \"openapi\": \"3.1.0\",\n  \"info\": {{ \"title\": \"OWL API — generated {class_short} API\", \"version\": \"0\", \"description\": \"Generated from {class} shapes in {graph}. Regenerate, never hand-edit (#3354).\" }},\n  \"paths\": {{\n{paths}\n  }},\n  \"components\": {{ \"schemas\": {{\n    \"EdgeRef\": {{ \"type\": \"object\", \"properties\": {{ \"name\": {{ \"type\": \"string\" }}, \"label\": {{ \"type\": \"string\" }} }} }},\n    \"{class_short}\": {{ \"type\": \"object\", \"properties\": {{ {props} }}{required} }},\n    \"List\": {{ \"type\": \"object\", \"properties\": {{ \"count\": {{ \"type\": \"integer\" }}, \"items\": {{ \"type\": \"array\", \"items\": {{ \"type\": \"object\", \"properties\": {{ \"name\": {{ \"type\": \"string\" }}, \"label\": {{ \"type\": \"string\" }}, \"status\": {{ \"type\": \"string\" }} }} }} }} }} }},\n    \"Fold\": {{ \"type\": \"object\", \"properties\": {{ \"{class_l}\": {{ \"type\": \"string\" }}, \"count\": {{ \"type\": \"integer\" }}, \"contains\": {{ \"type\": \"array\", \"items\": {{ \"type\": \"string\" }} }} }} }},\n    \"Schema\": {{ \"type\": \"object\" }}\n  }} }}\n}}\n",
        class_short = class_short,
        required = required,
        class = t.class,
        graph = ONTOLOGY_GRAPH,
        paths = paths,
        props = props.join(", "),
        class_l = class_l
    )
}

const NAME_PARAM: &str = "\"parameters\": [ { \"name\": \"name\", \"in\": \"path\", \"required\": true, \"schema\": { \"type\": \"string\" } } ], ";

/// trace mint-when-absent (#3364 AC6, Kade's #3354 finding): a blank/missing
/// trace header mints a recognizable, joinable id instead of silently logging
/// trace_id:"" — unjoinable-with-no-complaint is the silent-degradation class.
pub fn effective_trace(header_value: &str, ts_ms: u128, counter: u64) -> String {
    if header_value.trim().is_empty() {
        format!("owl-{}-{}", ts_ms, counter)
    } else {
        header_value.to_string()
    }
}

/// Serialize the route table as routes.json (the generated artifact).
pub fn routes_json(t: &RouteTable) -> String {
    let fields = t.fields.iter().map(|f| format!("\"{}\"", f)).collect::<Vec<_>>().join(", ");
    let routes = t.routes.iter().map(|r| format!("\"{}\"", r)).collect::<Vec<_>>().join(", ");
    let secured = t.secured.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(", ");
    // #3468 — the completeness FLOOR is part of the published contract: the page
    // meter computes mandatory-met + % from this list, sourced from the model (not v1).
    let mandatory = t.mandatory.iter().map(|m| format!("\"{}\"", m)).collect::<Vec<_>>().join(", ");
    format!(
        "{{\n  \"generatedFrom\": \"{}\",\n  \"graph\": \"{}\",\n  \"fields\": [{}],\n  \"routes\": [{}],\n  \"secured\": [{}],\n  \"mandatory\": [{}]\n}}\n",
        t.class, ONTOLOGY_GRAPH, fields, routes, secured, mandatory
    )
}

/// #3467 — TEST manifest projection: unit (route/mandatory/secured snapshot) +
/// API conformance (every GET route → 200) + security (unauth write → 401, secured
/// surface → 401, injection name → 400, incomplete create → 422), all DERIVED from
/// the RouteTable. A generic runner executes it against the live API — tests as a
/// model projection, not hand-written per domain. Pure + unit-pinned.
pub fn tests_manifest(t: &RouteTable) -> String {
    let class = t.class.rsplit('#').next().unwrap_or("").to_string();
    let plural = pluralize(&class);
    let p = format!("/{}", plural);
    let arr = |v: &[String]| v.iter().map(|x| format!("\"{}\"", json_escape(x))).collect::<Vec<_>>().join(", ");
    let conformance: Vec<String> = t.routes.iter()
        .filter(|r| r.starts_with("GET "))
        .map(|r| {
            let path = r.trim_start_matches("GET ");
            format!("{{ \"id\": \"conform {r}\", \"method\": \"GET\", \"path\": \"{path}\", \"expectStatus\": 200 }}",
                r = json_escape(r), path = json_escape(path))
        })
        .collect();
    let mut security: Vec<String> = vec![
        format!("{{ \"id\": \"unauth-create-401\", \"method\": \"POST\", \"path\": \"{p}\", \"auth\": \"none\", \"expectStatus\": 401 }}", p = json_escape(&p)),
        format!("{{ \"id\": \"injection-name-400\", \"method\": \"GET\", \"path\": \"{p}/bad%20name\", \"auth\": \"none\", \"expectStatus\": 400 }}", p = json_escape(&p)),
        format!("{{ \"id\": \"incomplete-create-422\", \"method\": \"POST\", \"path\": \"{p}\", \"auth\": \"owner\", \"body\": \"{{}}\", \"expectStatus\": 422 }}", p = json_escape(&p)),
    ];
    for s in &t.secured {
        security.push(format!("{{ \"id\": \"secured-401 {s}\", \"method\": \"GET\", \"path\": \"{s}\", \"auth\": \"none\", \"expectStatus\": 401 }}", s = json_escape(s)));
    }
    // #3467 finish — CONSTRAINT-enforcement cases, derived from the shape's fields:
    // a strict-datatype field gets a bad value → 422 (sh:datatype); an edge points at
    // a wrong-typed target → 422 (sh:class edge-target-type). These ASSERT the DAL
    // enforcement that makes the write surface constraint-safe (not just well-formed).
    let strict_xsd = |x: &str| matches!(x,
        "integer" | "int" | "long" | "short" | "byte" | "nonNegativeInteger" | "positiveInteger"
        | "nonPositiveInteger" | "negativeInteger" | "unsignedInt" | "unsignedLong" | "unsignedShort"
        | "decimal" | "double" | "float" | "boolean");
    let mut constraints: Vec<String> = vec![];
    for f in &t.fields {
        let (name, kind) = f.split_once('|').unwrap_or((f.as_str(), "plain"));
        if let Some(xsd) = kind.strip_prefix("datatype:") {
            if strict_xsd(xsd) {
                constraints.push(format!(
                    "{{ \"id\": \"datatype-reject {name}\", \"check\": \"datatype\", \"field\": \"{name}\", \"xsd\": \"{xsd}\", \"method\": \"POST\", \"path\": \"{p}\", \"badValue\": \"not-a-{xsd}\", \"expectStatus\": 422 }}",
                    name = json_escape(name), xsd = json_escape(xsd), p = json_escape(&p)
                ));
            }
        } else if let Some(cls) = kind.strip_prefix("edge:") {
            let seg = name.to_lowercase();
            constraints.push(format!(
                "{{ \"id\": \"edge-target-type-reject {name}\", \"check\": \"edge-target-type\", \"edge\": \"{name}\", \"targetClass\": \"{cls}\", \"method\": \"POST\", \"path\": \"{p}/:name/{seg}\", \"targetOfWrongType\": true, \"expectStatus\": 422 }}",
                name = json_escape(name), cls = json_escape(cls), p = json_escape(&p), seg = json_escape(&seg)
            ));
        }
    }
    format!(
        "{{\n  \"class\": \"{class}\",\n  \"plural\": \"{plural}\",\n  \"unit\": {{ \"routes\": [{routes}], \"mandatory\": [{mandatory}], \"secured\": [{secured}] }},\n  \"conformance\": [\n    {conf}\n  ],\n  \"security\": [\n    {sec}\n  ],\n  \"constraints\": [\n    {cons}\n  ]\n}}\n",
        class = json_escape(&class), plural = json_escape(&plural),
        routes = arr(&t.routes), mandatory = arr(&t.mandatory), secured = arr(&t.secured),
        conf = conformance.join(",\n    "), sec = security.join(",\n    "), cons = constraints.join(",\n    ")
    )
}

/// #3467 — ADR-031 MCP tool BINDING projection: chorus_<plural-resource>_<verb>
/// (closed verb set get/list/add; add delegates to the DAL — the one write
/// authority). The MCP surface generated from the SAME model as the REST routes, so
/// the two bindings cannot drift. Pure + unit-pinned.
pub fn mcp_binding(t: &RouteTable) -> String {
    let class = t.class.rsplit('#').next().unwrap_or("").to_string();
    let plural = pluralize(&class);
    let tool = |verb: &str, route: String, extra: &str| format!(
        "{{ \"name\": \"chorus_{plural}_{verb}\", \"verb\": \"{verb}\", \"route\": \"{route}\"{extra} }}",
        plural = plural, verb = verb, route = json_escape(&route), extra = extra
    );
    let tools = vec![
        tool("list", format!("GET /{}", plural), ""),
        tool("get", format!("GET /{}/:name", plural), ""),
        tool("add", format!("POST /{}", plural), ", \"delegatesTo\": \"DAL (chorus-model)\""),
    ];
    format!(
        "{{\n  \"class\": \"{class}\",\n  \"binding\": \"mcp\",\n  \"convention\": \"ADR-031 chorus_<plural-resource>_<verb>\",\n  \"tools\": [\n    {tools}\n  ]\n}}\n",
        class = json_escape(&class), tools = tools.join(",\n    ")
    )
}

/// English plural for a lowercased class name. Naive `+s` produced `propertys`;
/// this handles consonant+y→ies and sibilants→es. Used by BOTH generate() and
/// the serve router so routes and dispatch agree.
pub fn pluralize(s: &str) -> String {
    let s = s.to_lowercase();
    let ends_with_any = |suffixes: &[&str]| suffixes.iter().any(|suf| s.ends_with(suf));
    if let Some(stem) = s.strip_suffix('y') {
        let last = stem.chars().last();
        let vowel = matches!(last, Some('a' | 'e' | 'i' | 'o' | 'u'));
        if !vowel && last.is_some() {
            return format!("{}ies", stem); // property → properties
        }
    }
    if ends_with_any(&["s", "x", "z", "ch", "sh"]) {
        return format!("{}es", s); // class → classes, box → boxes
    }
    format!("{}s", s)
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\r', "\\r")
}

/// #3435 — coerce a Property's string-encoded value to its typed JSON fragment,
/// ONCE per propertyValueType (string | int | bool | json | list). The effective-config
/// read calls this so a consumer gets a typed value, not a raw literal. Fails LOUD on a
/// type mismatch — never silently defaults to string (a wrong type must surface, not hide).
/// list/json are stored already-encoded: coercion is a structural shape-check + passthrough
/// (zero-dep — owl-api takes no JSON parser; a full parse is a follow-on if depth matters).
pub fn coerce_effective(value: &str, value_type: &str) -> R<String> {
    match value_type {
        "int" => value
            .parse::<i64>()
            .map(|n| n.to_string())
            .map_err(|_| format!("propertyValue {:?} is not an int", value)),
        "bool" => match value {
            "true" | "false" => Ok(value.to_string()),
            _ => Err(format!("propertyValue {:?} is not a bool (want true|false)", value)),
        },
        "string" => Ok(format!("\"{}\"", json_escape(value))),
        "list" => {
            let t = value.trim();
            if t.len() >= 2 && t.starts_with('[') && t.ends_with(']') {
                Ok(t.to_string())
            } else {
                Err(format!("propertyValue {:?} is not a JSON array", value))
            }
        }
        "json" => {
            let t = value.trim();
            let shaped = t.len() >= 2
                && ((t.starts_with('{') && t.ends_with('}'))
                    || (t.starts_with('[') && t.ends_with(']')));
            if shaped {
                Ok(t.to_string())
            } else {
                Err(format!("propertyValue {:?} is not a JSON object/array", value))
            }
        }
        other => Err(format!("unknown propertyValueType {:?}", other)),
    }
}

/// #3435 — parse one effective-config fetch row into a PropertyDatum. The fetch CONCATs
/// each Property as "iri|key|valueType|value" into ?v (owl-api's single-var seam). `value`
/// is LAST and `splitn(4)` gives it the remainder, so an arbitrary config value may itself
/// contain '|'. An empty value is an explicit override (meaningful to the resolver), not
/// malformed; a missing iri/key/value field is.
pub fn parse_property_row(row: &str) -> R<properties_resolver::PropertyDatum> {
    let mut it = row.splitn(4, '|');
    let iri = it.next().unwrap_or("");
    match (it.next(), it.next(), it.next()) {
        (Some(key), Some(value_type), Some(value)) if !iri.is_empty() && !key.is_empty() => {
            Ok(properties_resolver::PropertyDatum {
                iri: iri.to_string(),
                key: key.to_string(),
                value: value.to_string(),
                value_type: value_type.to_string(),
            })
        }
        _ => Err(format!("malformed property row: {:?}", row)),
    }
}

/// #3435 — assemble a node's fetched property rows into a ScopeNode. `kind` is
/// chosen by the caller (the handler): for the single-node proof the leaf is the
/// most specific, so the handler passes ScopeKind::Service; the leaf-kind taxonomy
/// for non-structural nodes (e.g. a TestCoverage carrying testType) is deferred to
/// the ownership-walk follow-on. A malformed row fails LOUD — never a dropped property.
pub fn build_scope_node(node_iri: &str, kind: ScopeKind, rows: &[String]) -> R<ScopeNode> {
    let mut properties = Vec::with_capacity(rows.len());
    for row in rows {
        properties.push(parse_property_row(row)?);
    }
    Ok(ScopeNode { kind, iri: node_iri.to_string(), properties })
}

/// #3435 — the node-scoped effective-config fetch. Reads `urn:chorus:instances` LIVE
/// via SPARQL — NO projection/mirror/sqlite store (the AC invariant; the query-builder
/// test asserts the instances graph + hasProperty traversal so a projection swap goes
/// red). Traverses hasProperty→Property and returns ALL the node's declared properties
/// as "iri|key|valueType|value" rows (value LAST so it may contain '|') in ONE round-trip.
/// The key is selected in pure code (decide_effective_value), never filtered in SPARQL —
/// so the round-trip stays one and key-selection stays unit-tested.
pub fn effective_fetch_query(node_iri: &str) -> String {
    format!(
        "SELECT ?v WHERE {{ \
           GRAPH <{g}> {{ \
             <{node}> <{ns}hasProperty> ?prop . \
             ?prop <{ns}propertyKey> ?key . \
             ?prop <{ns}propertyValue> ?value . \
             ?prop <{ns}propertyValueType> ?vtype . \
           }} \
           BIND(CONCAT(STR(?prop), \"|\", STR(?key), \"|\", STR(?vtype), \"|\", STR(?value)) AS ?v) \
         }}",
        g = INSTANCES_GRAPH,
        ns = NS,
        node = node_iri
    )
}

/// #3435 — shape the effective-config response from a node's already-fetched rows.
/// The handler's pure core (it adds only the live `sparql_json` fetch): build the
/// 1-element scope chain, resolve `key`, coerce. 200 with the typed value + provenance,
/// 404 if the key is unset on the node, 500 on a malformed row / coercion mismatch.
/// `value` is the coerced JSON fragment (bare `3000`, `true`, or a quoted string).
pub fn effective_response(node_name: &str, key: &str, rows: &[String]) -> (u16, String) {
    let node_iri = format!("{}{}", NS, node_name);
    let node = match build_scope_node(&node_iri, ScopeKind::Service, rows) {
        Ok(n) => n,
        Err(e) => return (500, format!("{{\"error\":\"{}\"}}", json_escape(&e))),
    };
    match decide_effective_value(&[node], key) {
        Ok(Some(res)) => match coerce_effective(&res.value, &res.value_type) {
            Ok(coerced) => (
                200,
                format!(
                    "{{\"node\":\"{}\",\"key\":\"{}\",\"value\":{},\"valueType\":\"{}\",\"winningScope\":\"{}\"}}",
                    json_escape(node_name),
                    json_escape(key),
                    coerced,
                    json_escape(&res.value_type),
                    json_escape(&res.winning_scope_iri)
                ),
            ),
            Err(e) => (500, format!("{{\"error\":\"{}\"}}", json_escape(&e))),
        },
        Ok(None) => (
            404,
            format!(
                "{{\"error\":\"no property sets key\",\"node\":\"{}\",\"key\":\"{}\"}}",
                json_escape(node_name),
                json_escape(key)
            ),
        ),
        Err(e) => (500, format!("{{\"error\":\"malformed scope chain: {:?}\"}}", e)),
    }
}

/// #3435 — a config key is compared in pure code, never interpolated into SPARQL (the
/// read is node-scoped + fetches the full property set), so it needs hygiene, not the
/// strict injection guard. Dotted keys (`alert.threshold`) are valid; `is_safe_local`
/// would wrongly reject them. Allow alphanumeric + `-` `_` `.`, bounded, non-empty.
pub fn is_safe_key(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
}

/// #3435 — the HTTP status line for a response code. Extracted from serve()'s inline
/// match so it's unit-testable; a live /effective request exposed that 400 (bad input)
/// and 500 (server error) were absent and silently serialized as "502 Bad Gateway".
pub fn status_line(code: u16) -> &'static str {
    match code {
        200 => "200 OK",
        201 => "201 Created",
        400 => "400 Bad Request",
        401 => "401 Unauthorized",
        403 => "403 Forbidden",
        404 => "404 Not Found",
        409 => "409 Conflict",
        422 => "422 Unprocessable Entity",
        500 => "500 Internal Server Error",
        501 => "501 Not Implemented",
        _ => "502 Bad Gateway",
    }
}

/// #3420 — GENERATE the Athena domain page as a PROJECTION on the #3415 design system,
/// replacing the hand-built domain-detail page. page_html emits the STATIC SHELL — the
/// real anatomy (breadcrumb → identity → stats → promise → completeness → facet sections)
/// with system.css classes + the generated marker — and the shared /js/domain-renderer.js
/// fills it: client-fetching the EXISTING Athena/chorus-domain facet endpoints (same-origin)
/// + the owl-api model identity overlay (owner/step/comment). One page renders any domain
/// via ?name=. PROJECTION — regenerate, never hand-edit. Built in stages (#3420 design pass):
/// shell + identity/stats/completeness first; the 17 facet sections in the renderer.
pub fn page_html(t: &RouteTable) -> String {
    let class_short = t.class.rsplit('#').next().unwrap_or("Domain").to_string();
    let tmpl = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{{CLASS}} — Athena</title>
<!-- GENERATED by owl-api page_html (#3420). PROJECTION — regenerate from the model, never hand-edit. system.css = #3415 vocabulary; facet data from the existing Athena endpoints (same-origin); identity + decomposition (contains/partof) from owl-api. -->
<!-- DATA-ROUTE SECURITY DECISION (#3420 AC4, recorded): GET /domains, /domains/:name, /domains/:name/contains, /domains/:name/partof are OPEN — read-only navigation of the model, nothing to protect; gate-the-token cost only where there's something to protect. Writes + /schema/:class are secured model-driven via #3414 (chorus:requiresAuth annotation projects is_secured). No token on this page today (AC5) because no rendered route is secured; if a facet route is later annotated secured, the browser-session→token lane (#3402, Silas's) is the designed-in path. -->
<link rel="stylesheet" href="/css/system.css">
</head>
<body class="theme-light">
<nav class="navbar no-print">
  <a href="/athena/value-stream.html">Athena</a>
  <span class="muted">&rsaquo;</span> <a id="bc-step" href="#">Step</a>
  <span class="muted">&rsaquo;</span> <span id="bc-domain">{{CLASS}}</span>
</nav>
<div class="wrap">
  <h1 id="domain-title">{{CLASS}}</h1>
  <p class="muted" id="domain-subtitle"></p>
  <div class="content-actions" id="content-actions" data-title="Athena {{CLASS}}" data-url="">
    <button class="action-btn" data-btn="print" title="Save as PDF">&#x2913; PDF</button>
    <button class="action-btn" data-btn="share" title="Share this page">&#x2197; Share</button>
    <button class="action-btn" data-btn="reflect" title="Send to Reflect">&#x2726; Reflect</button>
  </div>
  <div id="stats-bar"></div>
  <div id="partof-block"></div>
  <div id="haschild-block"></div>
  <div id="promise-block"></div>
  <div id="completeness-block"></div>
  <div id="content-sections"></div>
  <p class="muted" style="margin-top:var(--space-5)">Athena &middot; Chorus &middot; GENERATED page (owl-api) — live from the model</p>
</div>
<script>window.OWL_PORT = 3360;</script>
<script src="https://cdn.jsdelivr.net/npm/mermaid@10/dist/mermaid.min.js"></script>
<script src="/js/domain-renderer.js" defer></script>
<script src="/js/content-actions.js" defer></script>
</body>
</html>
"##;
    tmpl.replace("{{CLASS}}", &class_short)
}

/// Build the JSON for one entity: every direct property in the instances graph.
/// #3506 / ADR-047 §1+§3 — project an entity into (data, links). DATATYPE props are
/// scalars in `data`; EDGE props (NS-IRI objects) project into `links` as target id
/// refs — single-valued → string, multi-valued → ARRAY. The array is the fix for a
/// real bug the contract exposed: the old shape emitted a multi-valued edge (e.g.
/// `contains`, ~80 files on the cards domain) as duplicate JSON keys — malformed
/// JSON. links also drops the per-edge label lookup (#3354): a link is a traversal
/// ref, the label lives on the target — fewer queries, ADR-conformant.
fn entity_json(name: &str, exposure: &[(String, String)], authed: bool) -> R<(String, String)> {
    let subject = format!("{}{}", NS, name);
    let q = format!(
        "SELECT ?v WHERE {{ GRAPH <{g}> {{ <{s}> ?p ?o }} BIND(CONCAT(STR(?p), \"|\", STR(?o)) AS ?v) }} ORDER BY ?v",
        g = INSTANCES_GRAPH, s = subject
    );
    let body = sparql_json(&q)?;
    let prs = select_v(&body);
    if prs.is_empty() {
        return Err("not-found".to_string());
    }
    // #3506 / ADR-048 §3 — field-exposure enforcement is PER-SHADE OPT-IN (migration-
    // safe, the #3414 mixed-state pattern): a shape that DECLARES any chorus:exposure
    // enforces the whitelist fail-closed (a data field shows only if its level passes
    // field_exposed); a shape with NO exposure annotations stays fully open until it's
    // migrated. So annotating ServiceShape tightens it without breaking un-annotated
    // Domain/Product reads.
    let enforced = !exposure.is_empty();
    let level_of = |k: &str| exposure.iter().find(|(f, _)| f == k).map(|(_, l)| l.as_str());
    let mut data_parts = vec![format!("\"iri\": \"{}\"", json_escape(&subject))];
    let mut links: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    for rowv in prs {
        let (p, o) = match rowv.split_once('|') { Some((a, b)) => (a.to_string(), b.to_string()), None => continue };
        let key = p.rsplit(['#', '/']).next().unwrap_or(&p).to_string();
        if o.starts_with(NS) {
            // EDGE → links (target id ref). Multi-valued accumulates into one array.
            let target_name = o.rsplit('#').next().unwrap_or(&o).to_string();
            links.entry(key).or_default().push(format!("chorus:{}", target_name));
        } else {
            // SCALAR data field — fail-closed exposure gate when the shape opts in.
            if enforced && !field_exposed(level_of(&key), authed) {
                continue;
            }
            if o.starts_with("http") && o.contains('#') {
                data_parts.push(format!("\"{}\": \"{}\"", json_escape(&key), json_escape(o.rsplit('#').next().unwrap_or(&o))));
            } else {
                data_parts.push(format!("\"{}\": \"{}\"", json_escape(&key), json_escape(&o)));
            }
        }
    }
    let data = format!("{{ {} }}", data_parts.join(", "));
    let mut link_parts: Vec<String> = Vec::new();
    for (k, vals) in &links {
        if vals.len() == 1 {
            link_parts.push(format!("\"{}\": \"{}\"", json_escape(k), json_escape(&vals[0])));
        } else {
            let arr = vals.iter().map(|v| format!("\"{}\"", json_escape(v))).collect::<Vec<_>>().join(", ");
            link_parts.push(format!("\"{}\": [{}]", json_escape(k), arr));
        }
    }
    let links_json = format!("{{ {} }}", link_parts.join(", "));
    Ok((data, links_json))
}

/// #3506 / ADR-047 — the uniform response envelope. Every owl-api read (and, as the
/// slice fans out, write + error) is wrapped in this ONE shape, generated from the
/// model — no per-endpoint shaping. PURE: every field is an input derived by the
/// caller from the request + the shape, so it is unit-testable without the store.
/// `data` is the only payload slot; collections omit `id` and carry `count`.
#[allow(clippy::too_many_arguments)]
pub fn envelope(
    kind: &str,
    id: Option<&str>,
    self_url: &str,
    shape: &str,
    shape_version: &str,
    commit: &str,
    requires_auth: bool,
    data_json: &str,
    links_json: &str,
    count: Option<i64>,
) -> String {
    let mut p: Vec<String> = Vec::new();
    p.push(format!("\"apiVersion\": \"{}\"", API_VERSION));
    p.push(format!("\"kind\": \"{}\"", json_escape(kind)));
    if let Some(i) = id {
        p.push(format!("\"id\": \"{}\"", json_escape(i)));
    }
    p.push(format!("\"self\": \"{}\"", json_escape(self_url)));
    p.push(format!(
        "\"generatedFrom\": {{ \"graph\": \"{}\", \"shape\": \"{}\", \"shapeVersion\": \"{}\", \"commit\": \"{}\" }}",
        json_escape(ONTOLOGY_GRAPH), json_escape(shape), json_escape(shape_version), json_escape(commit)
    ));
    p.push(format!("\"data\": {}", data_json));
    p.push(format!("\"links\": {}", links_json));
    if let Some(c) = count {
        p.push(format!("\"count\": {}", c));
    }
    p.push(format!("\"requiresAuth\": {}", requires_auth));
    p.push("\"deprecation\": null".to_string());
    format!("{{ {} }}", p.join(", "))
}

/// #3520 / ADR-047 §2 — the `generatedFrom` provenance, DERIVED FROM THE MODEL.
/// The version is a content hash of the shape's own declared property paths
/// (the `sh:path` IRIs, sorted) — the version IS the shape's content, so it
/// changes exactly when the shape changes and can never be stale, hand-bumped,
/// or faked. Supersedes BOTH the hand-authored `chorus:shapeVersion` literal and
/// the injected `OWL_API_MODEL_COMMIT` env (both deleted): version = f(model),
/// resolved per request from the graph — nothing hardcoded, nothing injected.
fn shape_meta(class_local: &str) -> (String, String, String) {
    let shape = format!("chorus:{}Shape", class_local);
    let class = format!("{}{}", NS, class_local);
    // The shape's property PATHS are IRIs (no blank nodes), so the sorted set is a
    // stable, canonical fingerprint of the schema — cheap to hash, never ambiguous.
    let pq = format!(
        "PREFIX sh: <http://www.w3.org/ns/shacl#> SELECT ?v WHERE {{ GRAPH <{g}> {{ ?sh sh:targetClass <{c}> ; sh:property ?p . ?p sh:path ?path BIND(STR(?path) AS ?v) }} }} ORDER BY ?v",
        g = ONTOLOGY_GRAPH, c = class
    );
    let version = sparql_json(&pq)
        .ok()
        .map(|b| select_v(&b).join("\n"))
        .filter(|s| !s.is_empty())
        .map(|paths| content_hash(&paths))
        .unwrap_or_else(|| "unversioned".to_string());
    // Both provenance axes (shapeVersion + the former `commit`) are now the same
    // model-derived version — one fact, not two separately-maintained stamps.
    (shape, version.clone(), version)
}

/// #3520 — a content-derived version/ETag: a stable hex digest of the given bytes.
/// The version IS the content (a git-blob-style hash), so it can never drift from
/// what it labels. `DefaultHasher` uses fixed keys, so the digest is deterministic
/// across processes — the same bytes always yield the same tag.
fn content_hash(bytes: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// #3506 / ADR-047 §4 — an error IS the same envelope (`kind:"Error"`) carrying an
/// RFC-9457 Problem-Details `data`. Served as `application/problem+json`. Per-field
/// `errors[]` is the projected SHACL violation report (generated) when present —
/// the model is the input-validation boundary, so the detail is not hand-written.
pub fn error_envelope(
    table: &RouteTable,
    instance_name: &str,
    status: u16,
    type_slug: &str,
    detail: &str,
    field_errors: &[(String, String)],
) -> String {
    let kind_local = table.class.rsplit('#').next().unwrap_or("Resource");
    let (shape, shape_version, commit) = shape_meta(kind_local);
    let plural = pluralize(kind_local);
    let instance = format!("/{}/{}/{}", API_VERSION, plural, instance_name);
    let title = match status {
        400 => "Bad Request", 401 => "Unauthorized", 403 => "Forbidden", 404 => "Not Found",
        409 => "Conflict", 412 => "Precondition Failed", 422 => "Unprocessable Entity",
        428 => "Precondition Required", 429 => "Too Many Requests", 502 => "Bad Gateway",
        _ => "Error",
    };
    let errs = if field_errors.is_empty() {
        String::new()
    } else {
        let items = field_errors
            .iter()
            .map(|(f, d)| format!("{{ \"field\": \"{}\", \"detail\": \"{}\" }}", json_escape(f), json_escape(d)))
            .collect::<Vec<_>>()
            .join(", ");
        format!(", \"errors\": [{}]", items)
    };
    let data = format!(
        "{{ \"type\": \"/errors/{}\", \"title\": \"{}\", \"status\": {}, \"detail\": \"{}\", \"instance\": \"{}\"{} }}",
        json_escape(type_slug), title, status, json_escape(detail), json_escape(&instance), errs
    );
    envelope("Error", None, &instance, &shape, &shape_version, &commit, false, &data, "{}", None)
}

/// #3506 / ADR-047 §7 — read one query param from a `&`-joined query string.
pub fn query_param(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        if k == key { Some(v.to_string()) } else { None }
    })
}

/// #3506 / ADR-047 §7 — opaque-cursor pagination (AIP-158). The cursor is the next
/// offset into the ordered, stable-per-request list; returns the page slice + the
/// next cursor (None at the end). Pure + unit-pinned.
pub fn paginate<'a>(items: &'a [String], cursor: Option<&str>, limit: usize) -> (&'a [String], Option<usize>) {
    let start = cursor.and_then(|c| c.parse::<usize>().ok()).unwrap_or(0).min(items.len());
    let end = start.saturating_add(limit.max(1)).min(items.len());
    let next = if end < items.len() { Some(end) } else { None };
    (&items[start..end], next)
}

/// Request metadata for the telemetry envelope, filled by handle().
#[derive(Debug, Default, Clone)]
pub struct ReqMeta {
    pub route: String,
    pub entity: String,
    pub fold: String,
    pub result_count: i64,
}

/// SERVE — answer the generated routes from the live graph.
pub fn handle(path: &str, table: &RouteTable) -> (u16, String) {
    // #3506 / ADR-048 §3 — default unauthenticated; the serve seam passes the real
    // authed state. An unauth read sees only `public` fields of an exposure-enforced shape.
    handle_meta(path, table, false).0
}

/// handle + envelope metadata (the seam's data source). `authed` = the caller
/// presented a valid token (gates `internal`-exposure fields, ADR-048 §3).
pub fn handle_meta(path: &str, table: &RouteTable, authed: bool) -> ((u16, String), ReqMeta) {
    let mut meta = ReqMeta::default();
    // /health — the probe target (blackbox-exporter, launchagent checks).
    if path == "/health" {
        meta.route = "health".into();
        return ((200, "{ \"ok\": true, \"service\": \"owl-api\" }".to_string()), meta);
    }
    let resp = handle_inner(path, table, &mut meta, authed);
    (resp, meta)
}

/// A safe entity local-name: non-empty, bounded, and only the characters that
/// appear in a minted IRI local part (ADR-040 ids are kebab/alnum). Anything else
/// could break or inject the SPARQL IRI it gets interpolated into. (#3420 code gate)
pub fn is_safe_local(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn handle_inner(path: &str, table: &RouteTable, meta: &mut ReqMeta, authed: bool) -> (u16, String) {
    // #3506 / ADR-047 §7 — split the query string off BEFORE route matching, so
    // `?limit=&cursor=` (cursor pagination, AIP-158) never breaks the path parse.
    let (path, query) = match path.split_once('?') {
        Some((p, q)) => (p, q),
        None => (path, ""),
    };
    let plural = format!("/{}", pluralize(table.class.rsplit('#').next().unwrap_or("domain")));
    let parts: Vec<&str> = path.trim_end_matches('/').split('/').filter(|s| !s.is_empty()).collect();

    // #3435 — GET /effective/:node/:key — the effective-config read. The ONLY impure
    // point in this card: read urn:chorus:instances LIVE via SPARQL (no projection), then
    // resolve + coerce in pure code. is_safe_local guards the node (interpolated into the
    // query); is_safe_key guards the key (compared in code only — dotted keys allowed).
    if parts.len() == 3 && parts[0] == "effective" {
        let (node, key) = (parts[1], parts[2]);
        meta.route = "effective".into();
        meta.entity = node.to_string();
        meta.fold = key.to_string();
        if !is_safe_local(node) {
            return (400, "{ \"error\": \"invalid node name\" }".to_string());
        }
        if !is_safe_key(key) {
            return (400, "{ \"error\": \"invalid key\" }".to_string());
        }
        let q = effective_fetch_query(&format!("{}{}", NS, node));
        return match sparql_json(&q) {
            Ok(body) => {
                let rows = select_v(&body);
                let (code, resp) = effective_response(node, key, &rows);
                meta.result_count = if code == 200 { 1 } else { 0 };
                (code, resp)
            }
            Err(e) => (502, format!("{{ \"error\": \"fuseki: {}\" }}", json_escape(&e))),
        };
    }

    // GET /schema/domain
    if path.starts_with("/schema/") {
        meta.route = "schema".into();
        let t = RouteTable { class: table.class.clone(), fields: table.fields.clone(), routes: table.routes.clone(), secured: table.secured.clone(), mandatory: table.mandatory.clone(), repo_target: table.repo_target.clone(), exposure: table.exposure.clone() };
        return (200, routes_json(&t));
    }
    // GET /openapi.json — the generated OpenAPI 3.1 spec (#3453, #3520). Another
    // projection of the SAME model that generates the routes; openapi_json walks
    // table.routes so a new edge type appears here automatically. The API
    // documents itself — no hand-written stand-in.
    if path == "/openapi.json" {
        meta.route = "openapi".into();
        return (200, openapi_json(table));
    }
    // GET /openapi — the human, browsable view (static shell, client-fetches
    // /openapi.json). Served as text/html via content_type_for in serve().
    if path == "/openapi" {
        meta.route = "openapi-html".into();
        return (200, openapi_html(&table.class));
    }
    // GET /domains
    if format!("/{}", parts.first().unwrap_or(&"")) == plural && parts.len() == 1 {
        meta.route = "list".into();
        let q = format!(
            "SELECT ?v WHERE {{ GRAPH <{g}> {{ ?s a <{c}> . OPTIONAL {{ ?s <{ns}label> ?label }} OPTIONAL {{ ?s <{ns}status> ?status }} BIND(CONCAT(STR(?s), \"|\", COALESCE(?label, \"\"), \"|\", COALESCE(?status, \"\")) AS ?v) }} }} ORDER BY ?v",
            g = INSTANCES_GRAPH, c = table.class, ns = NS
        );
        return match sparql_json(&q) {
            Ok(body) => {
                let items: Vec<String> = select_v(&body)
                    .into_iter()
                    .map(|rowv| {
                        let cols: Vec<&str> = rowv.splitn(3, '|').collect();
                        let name = cols.first().map(|s| s.rsplit('#').next().unwrap_or(s)).unwrap_or("");
                        format!(
                            "{{ \"name\": \"{}\", \"label\": \"{}\", \"status\": \"{}\" }}",
                            json_escape(name),
                            json_escape(cols.get(1).unwrap_or(&"")),
                            json_escape(cols.get(2).unwrap_or(&""))
                        )
                    })
                    .collect();
                {
                    // #3506 / ADR-047 — the collection list, enveloped + paginated:
                    // kind = the item class, no `id`, `data` = the page, `count` = the
                    // TOTAL, `links.next` = the cursor URL when more remain. Uniform
                    // with the entity read; consumers learn ONE shape.
                    let total = items.len() as i64;
                    let limit = query_param(query, "limit")
                        .and_then(|l| l.parse::<usize>().ok())
                        .filter(|n| *n > 0)
                        .unwrap_or(100);
                    let cursor = query_param(query, "cursor");
                    let (page, next) = paginate(&items, cursor.as_deref(), limit);
                    meta.result_count = page.len() as i64;
                    let data = format!("[\n  {}\n]", page.join(",\n  "));
                    let kind = table.class.rsplit('#').next().unwrap_or("Domain");
                    let (shape, shape_version, commit) = shape_meta(kind);
                    let self_url = format!("/{}{}", API_VERSION, plural);
                    let links = match next {
                        Some(n) => format!("{{ \"next\": \"/{}{}?cursor={}&limit={}\" }}", API_VERSION, plural, n, limit),
                        None => "{}".to_string(),
                    };
                    (200, envelope(kind, None, &self_url, &shape, &shape_version, &commit, !table.secured.is_empty(), &data, &links, Some(total)))
                }
            }
            Err(e) => (502, error_envelope(table, "", 502, "upstream", &json_escape(&e), &[])),
        };
    }
    // GET /domains/:name and /domains/:name/contains
    if format!("/{}", parts.first().unwrap_or(&"")) == plural && (parts.len() == 2 || parts.len() == 3) {
        let name = parts[1];
        // The name is interpolated into a SPARQL IRI (<{ns}{name}>) by every entity
        // branch below (detail, /contains, /partof). A name carrying SPARQL/IRI
        // metacharacters would break or inject the query, so refuse anything that is
        // not a bare local name. One guard covers all three folds. (#3420 code gate)
        if !is_safe_local(name) {
            return (400, "{ \"error\": \"invalid entity name\" }".to_string());
        }
        meta.entity = name.to_string();
        meta.route = if parts.len() == 3 { "fold".into() } else { "detail".into() };
        if parts.len() == 3 { meta.fold = parts[2].to_string(); }
        if parts.len() == 3 && parts[2] == "contains" {
            // DOWN containment, symmetric with /partof's UNION below: a node "contains"
            // its children via chorus:contains (domain→sub) OR chorus:hasDomain
            // (product→domain). Querying only `contains` left /products/:p/contains
            // empty though the hasDomain edges exist — the UP bind Kade's product-rooted
            // tree render needs (#3466). One predicate set, both directions mirror.
            let q = format!(
                "SELECT ?v WHERE {{ GRAPH <{g}> {{ {{ <{ns}{n}> <{ns}contains> ?o }} UNION {{ <{ns}{n}> <{ns}hasDomain> ?o }} }} BIND(STR(?o) AS ?v) }}",
                g = INSTANCES_GRAPH, ns = NS, n = name
            );
            return match sparql_json(&q) {
                Ok(body) => {
                    let items: Vec<String> = select_v(&body).into_iter()
                        .map(|v| format!("\"{}\"", json_escape(v.rsplit('#').next().unwrap_or(&v))))
                        .collect();
                    {
                        meta.result_count = items.len() as i64;
                        (200, format!("{{ \"domain\": \"{}\", \"count\": {}, \"contains\": [{}] }}", json_escape(name), items.len(), items.join(", ")))
                    }
                }
                Err(e) => (502, format!("{{ \"error\": \"{}\" }}", json_escape(&e))),
            };
        }
        // #3420 slice 2 — UPWARD edge: who contains / has-domain this entity (the inverse
        // of contains/hasDomain). Mirrors /contains; the page renders up + down deps.
        if parts.len() == 3 && parts[2] == "partof" {
            let q = format!(
                "SELECT ?v WHERE {{ GRAPH <{g}> {{ {{ ?s <{ns}contains> <{ns}{n}> }} UNION {{ ?s <{ns}hasDomain> <{ns}{n}> }} }} BIND(STR(?s) AS ?v) }}",
                g = INSTANCES_GRAPH, ns = NS, n = name
            );
            return match sparql_json(&q) {
                Ok(body) => {
                    let items: Vec<String> = select_v(&body).into_iter()
                        .map(|v| format!("\"{}\"", json_escape(v.rsplit('#').next().unwrap_or(&v))))
                        .collect();
                    {
                        meta.result_count = items.len() as i64;
                        (200, format!("{{ \"domain\": \"{}\", \"count\": {}, \"partof\": [{}] }}", json_escape(name), items.len(), items.join(", ")))
                    }
                }
                Err(e) => (502, format!("{{ \"error\": \"{}\" }}", json_escape(&e))),
            };
        }
        // #3351 slice 1 — STRUCTURAL recursion: this entity's child entities via chorus:hasChild
        // (ADR-041: hasChild = domain→domain structure, NEVER contains=content membership).
        // This is the clickable parent→child edge the page walks (e.g. messages→heralds).
        if parts.len() == 3 && parts[2] == "has-child" {
            let q = format!(
                "SELECT ?v WHERE {{ GRAPH <{g}> {{ <{ns}{n}> <{ns}hasChild> ?o }} BIND(STR(?o) AS ?v) }}",
                g = INSTANCES_GRAPH, ns = NS, n = name
            );
            return match sparql_json(&q) {
                Ok(body) => {
                    let items: Vec<String> = select_v(&body).into_iter()
                        .map(|v| format!("\"{}\"", json_escape(v.rsplit('#').next().unwrap_or(&v))))
                        .collect();
                    {
                        meta.result_count = items.len() as i64;
                        (200, format!("{{ \"domain\": \"{}\", \"count\": {}, \"hasChild\": [{}] }}", json_escape(name), items.len(), items.join(", ")))
                    }
                }
                Err(e) => (502, format!("{{ \"error\": \"{}\" }}", json_escape(&e))),
            };
        }
        // #3468 — MODEL-DRIVEN completeness gauge: present datatype sections vs the
        // mandatory floor (table.mandatory, projected from sh:severity sh:Violation).
        // Unsecured read — it MEASURES, never blocks (thermometer). Replaces the page's
        // Athena-v1 /subdomains/:id/completeness call (severs the old↔new dependency).
        if parts.len() == 3 && parts[2] == "completeness" {
            let q = format!(
                "SELECT ?v WHERE {{ GRAPH <{g}> {{ <{ns}{n}> ?p ?o . FILTER(isLiteral(?o)) BIND(CONCAT(REPLACE(STR(?p), '.*#', ''), '|', STR(?o)) AS ?v) }} }}",
                g = INSTANCES_GRAPH, ns = NS, n = name
            );
            return match sparql_json(&q) {
                Ok(body) => {
                    let present: Vec<(String, String)> = select_v(&body).into_iter()
                        .filter_map(|row| row.split_once('|').map(|(a, b)| (a.to_string(), b.to_string())))
                        .collect();
                    let (met, pct, have, miss) = completeness(&present, &table.mandatory);
                    let arr = |v: &[String]| v.iter().map(|s| format!("\"{}\"", json_escape(s))).collect::<Vec<_>>().join(", ");
                    meta.result_count = table.mandatory.len() as i64;
                    (200, format!(
                        "{{ \"domain\": \"{}\", \"met\": {}, \"percentage\": {}, \"present\": [{}], \"missing\": [{}] }}",
                        json_escape(name), met, pct, arr(&have), arr(&miss)
                    ))
                }
                Err(e) => (502, format!("{{ \"error\": \"{}\" }}", json_escape(&e))),
            };
        }
        return match entity_json(name, &table.exposure, authed) {
            Ok((data, links)) => {
                meta.result_count = 1;
                // #3506 / ADR-047 — wrap the entity read in the uniform envelope
                // (prove-one-first: this GET /:name path is the end-to-end proof).
                let kind = table.class.rsplit('#').next().unwrap_or("Domain");
                let (shape, shape_version, commit) = shape_meta(kind);
                let self_url = format!("/{}{}/{}", API_VERSION, plural, name);
                let id = format!("chorus:{}", name);
                let body = envelope(
                    kind, Some(&id), &self_url, &shape, &shape_version, &commit,
                    !table.secured.is_empty(), &data, &links, None,
                );
                (200, body)
            }
            Err(e) if e == "not-found" => (404, error_envelope(table, name, 404, "not-found", &format!("no such {}: {}", table.class.rsplit('#').next().unwrap_or("entity").to_lowercase(), name), &[])),
            Err(e) => (502, error_envelope(table, name, 502, "upstream", &json_escape(&e), &[])),
        };
    }
    (404, format!("{{ \"error\": \"unknown route\", \"routes\": [{}] }}",
        table.routes.iter().map(|r| format!("\"{}\"", r)).collect::<Vec<_>>().join(", ")))
}

/// The std-only HTTP loop. One thread, GET-only, JSON-only — a spike server.
///
/// THE IoC SEAM (#3350 AC5, Jeff's inversion-of-control design): every request
/// passes through exactly one point — the `handle()` call below — before any
/// route logic runs. v1 wraps that single call with the injected cross-cuts
/// (auth, request logging, validation, rate limits) ONCE, and every generated
/// route inherits them. No per-route wiring, ever — that's the payoff of
/// generating: the seam is structural, not conventional.
/// Build the full HTTP response. Pure seam (the `effective_trace` pattern) so
/// the wire shape is testable. #3373: responses carry CORS — pages ride beside
/// their generated APIs, and a :3340-served page must read this loopback-bound
/// API cross-origin. The permissive origin is loopback-scoped (listener binds
/// 127.0.0.1, tunnel never exposes it); the #3355 expiry tooth (security ADR
/// #3372) supersedes it when generated authn lands.
pub fn http_response(status: &str, body: &str) -> String {
    http_response_ct(status, body, "application/json")
}

/// #3453 — content-type-aware response builder. The OpenAPI human view (/openapi)
/// is HTML, not JSON; everything else stays application/json. http_response keeps
/// its JSON default so existing callers (cors.rs) are untouched.
pub fn http_response_ct(status: &str, body: &str, content_type: &str) -> String {
    format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status, content_type, body.len(), body
    )
}

/// #3506 / ADR-047 §7 — a cacheable read response: adds `Vary: Accept` (content-
/// negotiation) and, when an ETag is supplied (= the model `commit`), `ETag` +
/// `Cache-Control: no-cache` so a client can revalidate with `If-None-Match` and get
/// a 304 when the model hasn't moved. Pure so it's unit-pinned.
pub fn http_response_cacheable(status: &str, body: &str, content_type: &str, etag: Option<&str>) -> String {
    let mut headers = format!(
        "Content-Type: {}\r\nAccess-Control-Allow-Origin: *\r\nVary: Accept\r\n",
        content_type
    );
    if let Some(tag) = etag {
        headers.push_str(&format!("ETag: \"{}\"\r\nCache-Control: no-cache\r\n", tag));
    }
    format!(
        "HTTP/1.1 {}\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
        status, headers, body.len(), body
    )
}

/// #3506 / ADR-047 §7 — the 304 Not Modified for a conditional-GET hit (commit
/// unchanged): ETag echoed, no body.
pub fn http_response_304(etag: &str) -> String {
    format!(
        "HTTP/1.1 304 Not Modified\r\nETag: \"{}\"\r\nVary: Accept\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
        etag
    )
}

/// #3453 — the content type for a served path. The human OpenAPI view is HTML;
/// every machine route is JSON. Pure so it's unit-pinned.
pub fn content_type_for(path: &str) -> &'static str {
    if path == "/openapi" { "text/html; charset=utf-8" } else { "application/json" }
}

/// #3453 — the human, browsable OpenAPI view: a STATIC shell that fetches
/// /openapi.json client-side and renders it. No runtime template engine (the
/// ejs-500 deploy lesson) — the shell is generated once, the data is the live
/// generated spec. So the doc can never drift from the routes: both come from
/// the same model on every request.
pub fn openapi_html(class: &str) -> String {
    let class_short = class.rsplit('#').next().unwrap_or("");
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>OWL API — generated {cs} API</title>\n<!-- GENERATED by owl-api openapi_html (#3453). Static shell + client fetch — the spec is the live /openapi.json projection of the model; never hand-edit. -->\n<style>body{{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;margin:2rem;max-width:64rem}}h1{{font-size:1.2rem}}pre{{background:#f6f8fa;padding:1rem;border-radius:6px;overflow:auto;white-space:pre-wrap}}a{{color:#0969da}}</style>\n</head>\n<body>\n<h1>OWL API — generated {cs} API</h1>\n<p>Self-documenting. This spec is generated from the model on every request — <a href=\"/openapi.json\">/openapi.json</a> (raw).</p>\n<pre id=\"spec\">loading /openapi.json …</pre>\n<script>\nfetch('/openapi.json').then(function(r){{return r.json()}}).then(function(s){{document.getElementById('spec').textContent=JSON.stringify(s,null,2)}}).catch(function(e){{document.getElementById('spec').textContent='failed to load /openapi.json: '+e}});\n</script>\n</body>\n</html>\n",
        cs = class_short
    )
}

/// #3466 — pick the RouteTable whose class owns this request's path resource.
/// `/products/loom` → resource "products" → the Product table; `/schema/product`
/// → "product" → the Product table; `/health` is handled server-level upstream.
/// None = no class owns the resource (typed 404). Multi-class serve dispatches
/// through this so one server fronts every generated API on one origin.
pub fn select_table<'a>(path: &str, tables: &'a [RouteTable]) -> Option<&'a RouteTable> {
    let trimmed = path.trim_start_matches('/');
    let mut segs = trimmed.split('/');
    let first = segs.next().unwrap_or("");
    let resource = if first == "schema" { segs.next().unwrap_or("") } else { first };
    if resource.is_empty() {
        return None;
    }
    tables.iter().find(|t| {
        let cl = t.class.rsplit('#').next().unwrap_or("");
        pluralize(cl).eq_ignore_ascii_case(resource) || cl.eq_ignore_ascii_case(resource)
    })
}

/// #3494 — a COMPOSED domain surface: a domain (`domain`) mounted at its
/// `chorus:repoTarget` path (`mount`, e.g. "borg/properties"), composing the
/// per-class CRUD surfaces of the classes it `definesVocabulary` (`classes`,
/// localnames). Each class is a sub-resource under the mount
/// (`/borg/properties/property`, `/borg/properties/property-key`). The route shape
/// Silas's ADR-045 ratified: definesVocabulary exists to COMPOSE a domain's
/// surface — vocabulary classes belong UNDER their domain, not as root peers.
#[derive(Clone, Debug)]
pub struct DomainSurface {
    pub mount: String,
    pub domain: String,
    pub classes: Vec<String>,
}

/// #3494 — kebab a class localname into its sub-resource segment: PropertyKey →
/// "property-key", Property → "property". Lowercase with a hyphen before each
/// internal uppercase boundary.
pub fn class_subresource(class_local: &str) -> String {
    let mut out = String::new();
    for (i, ch) in class_local.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            out.push('-');
        }
        out.extend(ch.to_lowercase());
    }
    out
}

/// #3494 — what a composed-domain request resolves to. Pure over (path, surfaces).
#[derive(PartialEq, Debug)]
pub enum SurfaceHit {
    /// GET /<mount> → the domain's vocabulary index.
    Index { domain: String, classes: Vec<String> },
    /// /<mount>/<class-kebab>[/...] → dispatch to that vocab class's RouteTable,
    /// with the path rewritten to the class's own plural root for `handle()`.
    Class { class_local: String, rewritten_path: String },
}

/// #3494 — resolve a request path against the composed domain surfaces. Longest
/// mount wins (so a nested mount isn't shadowed). `/<mount>` exactly → Index;
/// `/<mount>/<sub>[/rest]` → the vocab class whose kebab matches `<sub>`, rewritten
/// to `/<plural>[/rest]` for the existing per-class `handle()`. None = not a
/// surface path (falls through to the primitive select_table). Pure + testable.
pub fn resolve_surface(path: &str, surfaces: &[DomainSurface]) -> Option<SurfaceHit> {
    let trimmed = path.trim_start_matches('/');
    // longest mount first so /a/b wins over /a
    let mut ordered: Vec<&DomainSurface> = surfaces.iter().collect();
    ordered.sort_by(|a, b| b.mount.len().cmp(&a.mount.len()));
    for s in ordered {
        let m = s.mount.trim_matches('/');
        if m.is_empty() {
            continue;
        }
        if trimmed == m {
            return Some(SurfaceHit::Index { domain: s.domain.clone(), classes: s.classes.clone() });
        }
        let prefix = format!("{}/", m);
        if let Some(rest) = trimmed.strip_prefix(&prefix) {
            let mut segs = rest.splitn(2, '/');
            let sub = segs.next().unwrap_or("");
            let tail = segs.next().map(|t| format!("/{}", t)).unwrap_or_default();
            if let Some(cl) = s.classes.iter().find(|c| class_subresource(c) == sub) {
                let rewritten = format!("/{}{}", pluralize(cl), tail);
                return Some(SurfaceHit::Class { class_local: cl.clone(), rewritten_path: rewritten });
            }
            // mount matched but no such vocab class → still a surface miss (typed 404 upstream)
            return Some(SurfaceHit::Index { domain: s.domain.clone(), classes: s.classes.clone() });
        }
    }
    None
}

/// #3494 — read the composed domain surfaces from the model: every domain with a
/// `chorus:repoTarget` (the mount) AND `chorus:definesVocabulary` edges (the
/// classes). One SPARQL; grouped per domain. Empty → no composed routes.
pub fn read_domain_surfaces() -> R<Vec<DomainSurface>> {
    // CONCAT dom|mount|cls into ?v (the single-var pattern select_v reads, as
    // generate() does for field|kind) — one row per (domain, class) pair.
    let q = format!(
        "PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <{g}> {{ ?d chorus:repoTarget ?mount ; chorus:definesVocabulary ?c BIND(CONCAT(REPLACE(STR(?d), '.*[#/]', ''), '|', STR(?mount), '|', REPLACE(STR(?c), '.*[#/]', '')) AS ?v) }} }} ORDER BY ?v",
        ns = NS, g = ONTOLOGY_GRAPH
    );
    let body = sparql_json(&q)?;
    let mut by_mount: std::collections::BTreeMap<(String, String), Vec<String>> = std::collections::BTreeMap::new();
    for row in select_v(&body) {
        let parts: Vec<&str> = row.splitn(3, '|').collect();
        if parts.len() == 3 && !parts[1].is_empty() && !parts[2].is_empty() {
            by_mount
                .entry((parts[1].to_string(), parts[0].to_string()))
                .or_default()
                .push(parts[2].to_string());
        }
    }
    Ok(by_mount
        .into_iter()
        .map(|((mount, domain), mut classes)| {
            classes.sort();
            classes.dedup();
            DomainSurface { mount, domain, classes }
        })
        .collect())
}

pub fn serve(port: u16, tables: &[RouteTable]) -> R<()> {
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("bind {}: {}", port, e))?;
    let classes: Vec<&str> = tables.iter().map(|t| t.class.rsplit('#').next().unwrap_or("")).collect();
    eprintln!("owl-api: serving {} generated API(s) on :{} [{}] (read-only; writes go through chorus-model)", tables.len(), port, classes.join(", "));
    let mut req_counter: u64 = 0;
    // #3402 — seam auth config, loaded ONCE (no per-request env read, no graph call).
    let secret = std::env::var("CHORUS_SERVICE_TOKEN_SECRET").unwrap_or_default().into_bytes();
    let allowed_webids = auth::chorus_agent_webids();
    if secret.is_empty() {
        // fail-closed (verify rejects), but say so loudly — secured surfaces will 401.
        eprintln!("owl-api: WARNING — CHORUS_SERVICE_TOKEN_SECRET unset; secured surfaces will reject ALL requests (fail-closed).");
    }
    // #3494 — composed domain surfaces (the definesVocabulary fan-out): every domain
    // with chorus:repoTarget + definesVocabulary mounts at /<repoTarget>, composing
    // its vocab classes (whose RouteTables are already in `tables` via the serve
    // fan-out) as sub-resources. Read once at boot; empty → no composed routes.
    let surfaces = read_domain_surfaces().unwrap_or_default();
    for s in &surfaces {
        eprintln!("owl-api: + /{} domain surface [{}]", s.mount, s.classes.join(", "));
    }
    // #3506 / ADR-047 AC3 — emit-dims computed ONCE per class at boot (the #3066
    // lesson: never a Fuseki query per request). class → (product, shapeVersion,
    // commit); looked up at the telemetry seam below. apiVersion is the constant.
    let dim_cache: std::collections::HashMap<String, (String, String, String)> = tables
        .iter()
        .map(|t| {
            let local = t.class.rsplit('#').next().unwrap_or("").to_string();
            let product = read_containment_local(&t.class, "chorus:partOf", "")
                .ok()
                .flatten()
                .unwrap_or_default();
            let (_, shape_version, commit) = shape_meta(&local);
            (local, (product, shape_version, commit))
        })
        .collect();
    for stream in listener.incoming() {
        let mut stream = match stream { Ok(s) => s, Err(_) => continue };
        let started = std::time::Instant::now();
        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf).unwrap_or(0);
        let req = String::from_utf8_lossy(&buf[..n]);
        let raw_path = req.lines().next().and_then(|l| l.split_whitespace().nth(1)).unwrap_or("/").to_string();
        // #3506 / ADR-047 §7 — strip the query for ROUTING (so `?limit=&cursor=` never
        // breaks select_table at serve level); carry it to handle for pagination.
        let (path, query) = match raw_path.split_once('?') {
            Some((p, q)) => (p.to_string(), q.to_string()),
            None => (raw_path, String::new()),
        };
        let method = req.lines().next().and_then(|l| l.split_whitespace().next()).unwrap_or("GET").to_string();
        let header = |name: &str| -> String {
            req.lines()
                .find(|l| l.to_ascii_lowercase().starts_with(&format!("{}:", name)))
                .map(|l| l.splitn(2, ':').nth(1).unwrap_or("").trim().to_string())
                .unwrap_or_default()
        };
        // #3466 — multi-class dispatch: /health is server-level; otherwise select
        // the table whose class owns this path's resource. Unknown resource → 404.
        if path == "/health" {
            let resp = http_response_ct(status_line(200), "{ \"ok\": true, \"service\": \"owl-api\" }", "application/json");
            let _ = stream.write_all(resp.as_bytes());
            continue;
        }
        // #3506 / ADR-047 §7 — the DISCOVERY ROOT: GET / (and /v1) lists every served
        // primitive with its collection URL + per-shape version, so a consumer learns
        // the whole surface from one entrypoint (no out-of-band knowledge). Plus
        // /livez + /readyz liveness probes (the heartbeat the contract calls for).
        if path == "/livez" || path == "/readyz" {
            let resp = http_response_ct(status_line(200), "{ \"ok\": true }", "application/json");
            let _ = stream.write_all(resp.as_bytes());
            continue;
        }
        if path == "/" || path == format!("/{}", API_VERSION) {
            let prims: Vec<String> = tables
                .iter()
                .map(|t| {
                    let local = t.class.rsplit('#').next().unwrap_or("");
                    let plural = pluralize(local);
                    let sv = dim_cache.get(local).map(|d| d.1.clone()).unwrap_or_default();
                    format!(
                        "{{ \"kind\": \"{}\", \"collection\": \"/{}/{}\", \"openapi\": \"/{}/openapi.json\", \"shapeVersion\": \"{}\" }}",
                        json_escape(local), API_VERSION, plural, plural, json_escape(&sv)
                    )
                })
                .collect();
            let doc = format!(
                "{{ \"apiVersion\": \"{}\", \"service\": \"owl-api\", \"kind\": \"Discovery\", \"count\": {}, \"primitives\": [{}] }}",
                API_VERSION, tables.len(), prims.join(", ")
            );
            let resp = http_response_ct(status_line(200), &doc, "application/json");
            let _ = stream.write_all(resp.as_bytes());
            continue;
        }
        // #3506 / ADR-047 §7 — served OpenAPI for EVERY surface: /<plural>/openapi.json
        // (machine) and /<plural>/openapi (browsable). Was only /borg/properties; now
        // every primitive documents itself, found via the discovery root above.
        if let Some(rest) = path.strip_suffix("/openapi.json").or_else(|| path.strip_suffix("/openapi")) {
            let want = rest.trim_start_matches('/');
            if let Some(t) = tables
                .iter()
                .find(|t| pluralize(t.class.rsplit('#').next().unwrap_or("")) == want)
            {
                let (body, ct) = if path.ends_with(".json") {
                    (openapi_json(t), "application/json")
                } else {
                    (openapi_html(t.class.rsplit('#').next().unwrap_or("")), "text/html; charset=utf-8")
                };
                let resp = http_response_ct(status_line(200), &body, ct);
                let _ = stream.write_all(resp.as_bytes());
                continue;
            }
        }
        // #3494 — composed domain surface dispatch, BEFORE the primitive select_table.
        // /<mount> → the domain's vocabulary index; /<mount>/<class-kebab>[/...] →
        // rewrite to the vocab class's own plural root and fall through to the normal
        // per-class flow (so the composed sub-resource reuses handle()/auth untouched).
        let path = match resolve_surface(&path, &surfaces) {
            Some(SurfaceHit::Index { domain, classes }) => {
                let refs: Vec<&str> = classes.iter().map(String::as_str).collect();
                let idx = project_domain_vocab_index(&domain, &refs);
                let resp = http_response_ct(status_line(200), &idx, "application/json");
                let _ = stream.write_all(resp.as_bytes());
                continue;
            }
            Some(SurfaceHit::Class { rewritten_path, .. }) => rewritten_path,
            None => path,
        };
        let table = match select_table(&path, tables) {
            Some(t) => t,
            None => {
                let served: Vec<String> = tables
                    .iter()
                    .map(|t| format!("\"/{}\"", pluralize(t.class.rsplit('#').next().unwrap_or(""))))
                    .collect();
                let nf = format!("{{ \"error\": \"unknown route\", \"served\": [{}] }}", served.join(", "));
                let resp = http_response_ct(status_line(404), &nf, "application/json");
                let _ = stream.write_all(resp.as_bytes());
                continue;
            }
        };
        let upstream_started = std::time::Instant::now();
        // THE SEAM (#3402): auth injects here, ONCE, before route logic. A secured
        // surface with a missing/invalid credential short-circuits to 401/403; every
        // other surface falls through untouched (mixed-state). Local verify only.
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let ((code, body), meta) = if method != "GET" {
            // #3454 — the WRITE path. authN is ALWAYS required on a write (a write is
            // never open, unlike a read), then handle_write does authZ-from-ownedBy,
            // shape rejection, the SPARQL-UPDATE, the spine event, and a typed status.
            // A write can NEVER reach a read handler (the "POST returns 200" anti-pattern).
            let auth_hdr = header("authorization");
            let token = auth_hdr
                .strip_prefix("Bearer ")
                .or_else(|| auth_hdr.strip_prefix("bearer "))
                .unwrap_or(&auth_hdr);
            match auth::verify_token(token, &secret, &allowed_webids, now_secs) {
                Err(_) => {
                    let (c, t) = write_status("authn-missing");
                    ((c, format!("{{ \"error\": \"{}\", \"message\": \"a valid Bearer service-token is required for writes\" }}", t)),
                     ReqMeta { route: "write-authn".into(), ..Default::default() })
                }
                Ok(claims) => {
                    let role = role_from_webid(&claims.web_id).unwrap_or_default();
                    let body_str = req.splitn(2, "\r\n\r\n").nth(1).unwrap_or("");
                    let (c, b) = handle_write(&method, &path, body_str, table, &role);
                    ((c, b), ReqMeta { route: format!("write:{}", method.to_ascii_lowercase()), ..Default::default() })
                }
            }
        } else {
            match auth::seam_auth(&path, &header("authorization"), &secret, &allowed_webids, now_secs, &table.secured) {
                Some((c, b)) => ((c, b), ReqMeta { route: "auth-refused".into(), ..Default::default() }),
                None => {
                    let hp = if query.is_empty() { path.clone() } else { format!("{}?{}", path, query) };
                    // #3506 / ADR-048 §3 — authed = a valid service token is present; it
                    // gates `internal`-exposure fields on an exposure-enforced shape.
                    let ah = header("authorization");
                    let tok = ah.strip_prefix("Bearer ").or_else(|| ah.strip_prefix("bearer ")).unwrap_or(&ah);
                    let authed = auth::verify_token(tok, &secret, &allowed_webids, now_secs).is_ok();
                    handle_meta(&hp, table, authed)
                }
            }
        };
        let upstream_ms = upstream_started.elapsed().as_millis();
        let status = status_line(code);
        // #3520 / ADR-047 §7 — ETag = a content hash of the served body. The cache
        // key IS the response content, derived per-entity: it changes exactly when
        // THIS entity's bytes change (NOT a global commit that would invalidate every
        // cache on any model write — that coarseness was the bug), and it activates
        // with zero env and zero deploy injection. version = f(content).
        let etag = if method == "GET" && code == 200 {
            Some(content_hash(&body))
        } else {
            None
        };
        let cond_hit = method == "GET"
            && code == 200
            && etag.as_deref().map_or(false, |t| header("if-none-match").trim().trim_matches('"') == t);
        let resp = if cond_hit {
            http_response_304(etag.as_deref().unwrap_or(""))
        } else if method == "GET" && code == 200 {
            http_response_cacheable(status, &body, content_type_for(&path), etag.as_deref())
        } else {
            http_response_ct(status, &body, content_type_for(&path))
        };
        let _ = stream.write_all(resp.as_bytes());
        // THE SEAM: every request passes here once — telemetry now; auth,
        // validation, rate limits inject at this same point (the IoC payoff).
        if path != "/health" { // probes are noise, not signal
            let class_local = table.class.rsplit('#').next().unwrap_or("").to_string();
            let (product, shape_version, commit) = dim_cache.get(&class_local).cloned().unwrap_or_default();
            emit_telemetry(&TelemetryLine {
                class: class_local,
                entity: meta.entity,
                route: meta.route,
                fold: meta.fold,
                status: match code {
                    200 => ReqStatus::Ok,
                    404 => ReqStatus::Refused("not-found".into()),
                    _ => ReqStatus::Error("upstream".into()),
                },
                result_count: meta.result_count,
                total_ms: started.elapsed().as_millis(),
                upstream_ms,
                caller: header("x-chorus-caller"),
                trace_id: {
                    req_counter += 1;
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0);
                    effective_trace(&header("x-chorus-trace-id"), now, req_counter)
                },
                product,
                shape_version,
                commit,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // #3506 / ADR-047 — the envelope wrapper pins the full response contract for a
    // single entity AND a collection, generated from inputs (pure, no store).
    #[test]
    fn envelope_wraps_entity_in_adr047_contract() {
        let e = envelope(
            "Domain",
            Some("chorus:properties"),
            "/v1/domains/properties",
            "chorus:DomainShape",
            "2026-06-19",
            "534805b9",
            true,
            "{ \"purpose\": \"config-as-data\" }",
            "{ \"partOf\": \"/v1/products/borg\" }",
            None,
        );
        for needle in [
            "\"apiVersion\": \"v1\"",
            "\"kind\": \"Domain\"",
            "\"id\": \"chorus:properties\"",
            "\"self\": \"/v1/domains/properties\"",
            "\"generatedFrom\":",
            "\"graph\": \"urn:chorus:ontology\"",
            "\"shape\": \"chorus:DomainShape\"",
            "\"shapeVersion\": \"2026-06-19\"",
            "\"commit\": \"534805b9\"",
            "\"data\": { \"purpose\":",
            "\"links\": { \"partOf\":",
            "\"requiresAuth\": true",
            "\"deprecation\": null",
        ] {
            assert!(e.contains(needle), "entity envelope missing `{}`:\n{}", needle, e);
        }
        // an entity carries no `count`
        assert!(!e.contains("\"count\""), "entity envelope must not carry count: {}", e);
    }

    // #3506 / ADR-048 §3 — the read-side field-exposure gate, fail-closed.
    #[test]
    fn field_exposed_is_fail_closed() {
        assert!(field_exposed(Some("public"), false));
        assert!(field_exposed(Some("public"), true));
        assert!(!field_exposed(Some("internal"), false), "internal hidden from unauth");
        assert!(field_exposed(Some("internal"), true), "internal shown to authed");
        assert!(!field_exposed(Some("secret"), false));
        assert!(!field_exposed(Some("secret"), true), "secret NEVER emitted, even authed");
        assert!(!field_exposed(None, false));
        assert!(!field_exposed(None, true), "unmarked hidden even when authed (default-closed)");
        assert!(!field_exposed(Some("bogus"), true), "unknown level → hidden");
    }

    // #3506 / ADR-047 §7 — cursor pagination: page slicing + next cursor + query parse.
    #[test]
    fn paginate_pages_and_signals_next() {
        let items: Vec<String> = (0..25).map(|i| i.to_string()).collect();
        let (p0, n0) = paginate(&items, None, 10);
        assert_eq!(p0.len(), 10, "first page is `limit`");
        assert_eq!(n0, Some(10), "next cursor = end offset when more remain");
        let (p1, n1) = paginate(&items, Some("10"), 10);
        assert_eq!(p1, &["10", "11", "12", "13", "14", "15", "16", "17", "18", "19"]);
        assert_eq!(n1, Some(20));
        let (p2, n2) = paginate(&items, Some("20"), 10);
        assert_eq!(p2.len(), 5, "last partial page");
        assert_eq!(n2, None, "no next cursor at the end");
        // cursor past the end → empty page, no panic
        let (p3, n3) = paginate(&items, Some("999"), 10);
        assert!(p3.is_empty() && n3.is_none());
    }

    #[test]
    fn query_param_reads_keys() {
        assert_eq!(query_param("limit=20&cursor=10", "limit").as_deref(), Some("20"));
        assert_eq!(query_param("limit=20&cursor=10", "cursor").as_deref(), Some("10"));
        assert_eq!(query_param("limit=20", "cursor"), None);
        assert_eq!(query_param("", "limit"), None);
    }

    // #3506 / ADR-047 §7 — cacheable read carries ETag + Vary; 304 echoes the tag, no body.
    #[test]
    fn cacheable_response_carries_etag_and_vary() {
        let r = http_response_cacheable("200 OK", "{}", "application/json", Some("534805b9"));
        assert!(r.contains("ETag: \"534805b9\""), "ETag from the model commit: {}", r);
        assert!(r.contains("Vary: Accept"), "content-negotiation Vary: {}", r);
        assert!(r.contains("Cache-Control: no-cache"), "revalidate, don't blind-cache: {}", r);
        // no etag → no ETag header (e.g. commit unknown)
        let r2 = http_response_cacheable("200 OK", "{}", "application/json", None);
        assert!(!r2.contains("ETag:"), "no etag header when commit unknown: {}", r2);
        assert!(r2.contains("Vary: Accept"), "Vary still present: {}", r2);

        let nm = http_response_304("534805b9");
        assert!(nm.starts_with("HTTP/1.1 304 Not Modified"), "conditional hit → 304: {}", nm);
        assert!(nm.contains("ETag: \"534805b9\""), "304 echoes the tag: {}", nm);
        assert!(nm.trim_end().ends_with("\r\n") || nm.ends_with("\r\n\r\n"), "304 has no body");
    }

    #[test]
    fn envelope_collection_omits_id_and_carries_count() {
        let c = envelope(
            "Domain", None, "/v1/domains", "chorus:DomainShape",
            "2026-06-19", "534805b9", false, "[]", "{}", Some(35),
        );
        assert!(c.contains("\"count\": 35"), "collection carries count: {}", c);
        assert!(!c.contains("\"id\":"), "collection omits id: {}", c);
        assert!(c.contains("\"requiresAuth\": false"), "open collection: {}", c);
        assert!(c.contains("\"data\": []"), "collection data is the array: {}", c);
    }

    #[test]
    fn select_v_parses_all_rows() {
        let body = r#"{ "results": { "bindings": [
            { "v": { "value": "a|A|operating" } },
            { "v": { "value": "b|B|building" } } ] } }"#;
        let r = select_v(body);
        assert_eq!(r.len(), 2);
        assert_eq!(r[1], "b|B|building");
    }

    #[test]
    fn project_secured_is_model_driven() {
        // #3414: the secured-set comes from the model's annotation, not a hardcoded route.
        assert_eq!(project_secured("Domain", true), vec!["/schema/domain".to_string()],
            "annotated class → its schema surface guarded");
        assert_eq!(project_secured("Domain", false), Vec::<String>::new(),
            "no annotation → open (AC3 mixed-state: undeclared stays open)");
        // follows the CLASS, not hardcoded to domain — proof a DIFFERENT surface is securable (AC4 'beyond /schema/domain')
        assert_eq!(project_secured("Product", true), vec!["/schema/product".to_string()],
            "the secured surface is whatever class the model annotates — beyond /schema/domain");
    }

    #[test]
    fn project_repo_path_is_the_recursive_owl_projection() {
        use RepoKind::*;
        // ADR-041 one-level case: <vs-step>/products/<product>/domains/<domain>
        assert_eq!(
            project_repo_path(&[(ValueStream, "designing"), (Product, "athena"), (Domain, "domains")]),
            "designing/products/athena/domains/domains",
            "vs-step bare root, product + domain carry their collection prefix",
        );
        // RECURSION: sub-product + sub-domain are just more links in the chain
        assert_eq!(
            project_repo_path(&[
                (ValueStream, "directing"), (Product, "clearing"),
                (Product, "pulse"), (Domain, "messages"), (Domain, "streams"),
            ]),
            "directing/products/clearing/products/pulse/domains/messages/domains/streams",
            "sub-product (pulse under clearing) and sub-domain nest by the same rule",
        );
        // a DOMAIN-scoped service nests under its domain
        assert_eq!(
            project_repo_path(&[(ValueStream, "directing"), (Product, "clearing"), (Domain, "cards"), (Service, "card-store")]),
            "directing/products/clearing/domains/cards/services/card-store",
            "domain-scoped service lives under its domain",
        );
        // a PRODUCT-spanning service (the clearing/chorus service) parents off the PRODUCT, peer to domains/
        assert_eq!(
            project_repo_path(&[(ValueStream, "directing"), (Product, "clearing"), (Service, "clearing")]),
            "directing/products/clearing/services/clearing",
            "cross-domain service parents off the product, not forced under a domain",
        );
        // names lowercased + slashes/whitespace normalized; empty links skipped
        assert_eq!(
            project_repo_path(&[(ValueStream, " Building/ "), (Product, "Werk"), (Domain, ""), (Domain, "CICD")]),
            "building/products/werk/domains/cicd",
            "segments normalized, empty links dropped",
        );
    }

    #[test]
    fn resolve_repo_target_override_else_projection() {
        use RepoKind::*;
        let chain = [(ValueStream, "designing"), (Product, "athena"), (Domain, "domains")];
        // declared chorus:repoTarget is the explicit override (bespoke case)
        assert_eq!(
            resolve_repo_target(Some("  custom/home/  "), &chain),
            "custom/home",
            "declared override wins, trimmed + slash-normalized",
        );
        // absent/empty declared → project the walked containment chain
        assert_eq!(
            resolve_repo_target(None, &chain),
            "designing/products/athena/domains/domains",
            "no override → recursive projection of the chain",
        );
        assert_eq!(
            resolve_repo_target(Some("   "), &chain),
            "designing/products/athena/domains/domains",
            "whitespace-only declared falls through to the projection",
        );
        // partial chain (vs-step + domain, product unknown) still lands deterministically
        assert_eq!(
            resolve_repo_target(None, &[(ValueStream, "proving"), (Domain, "logs")]),
            "proving/domains/logs",
            "a partly-modeled entity still projects a deterministic path",
        );
    }

    #[test]
    fn project_product_index_binds_domains_from_the_graph() {
        // #3488: the product API is the aggregate of its hasDomain domains — the
        // binding is DERIVED (no manual register), sorted + deduped, each domain
        // mounted at its own route root.
        let idx = project_product_index("Athena", &["domains", "services", "knowledge"]);
        assert!(idx.contains("\"product\": \"athena\""), "product lowercased");
        assert!(idx.contains("{ \"name\": \"domains\", \"api\": \"/domains\" }"), "domain bound with its mount");
        assert!(idx.contains("services") && idx.contains("knowledge"), "all hasDomain domains bound");
        // adding a domain to the graph → it auto-appears (binding by construction)
        let idx2 = project_product_index("athena", &["domains", "cards"]);
        assert!(idx2.contains("\"name\": \"cards\""), "a new domain auto-registers in the product index");
        // normalization: dedup + skip empty, names lowercased
        let idx3 = project_product_index("athena", &["Domains", "domains", ""]);
        assert_eq!(idx3.matches("\"name\":").count(), 1, "dedup + skip-empty → one domain");
    }

    #[test]
    fn project_domain_vocab_index_composes_definesvocabulary_classes() {
        // #3494: a domain's API is the aggregate of the classes it definesVocabulary
        // — each class mounted at its pluralized route root (the per-class #3454
        // surface). Sorted, deduped; domain lowercased.
        let idx = project_domain_vocab_index("properties", &["Property", "PropertyKey"]);
        assert!(idx.contains("\"domain\": \"properties\""), "domain lowercased");
        assert!(idx.contains("{ \"class\": \"Property\", \"api\": \"/propertys\" }")
                || idx.contains("\"class\": \"Property\""), "vocab class bound with its mount");
        assert!(idx.contains("PropertyKey"), "all definesVocabulary classes composed");
        // dedup + skip-empty
        let idx2 = project_domain_vocab_index("Properties", &["Property", "Property", ""]);
        assert_eq!(idx2.matches("\"class\":").count(), 1, "dedup + skip-empty → one class");
        // AC4 — zero definesVocabulary edges → empty vocab array, no phantom surface
        let idx3 = project_domain_vocab_index("borg", &[]);
        assert!(idx3.contains("\"vocab\": []"), "zero classes → empty vocab, no phantom route");
    }

    #[test]
    fn class_subresource_kebabs_vocab_classes() {
        assert_eq!(class_subresource("Property"), "property");
        assert_eq!(class_subresource("PropertyKey"), "property-key");
        assert_eq!(class_subresource("Service"), "service");
    }

    #[test]
    fn resolve_surface_mounts_composed_domain_route() {
        // #3494 AC3: /<repoTarget> composes the domain's vocab classes as
        // sub-resources, each rewritten to its own plural root for handle().
        let surfaces = vec![DomainSurface {
            mount: "borg/properties".into(),
            domain: "properties".into(),
            classes: vec!["Property".into(), "PropertyKey".into()],
        }];
        // /<mount> → the vocab index
        match resolve_surface("/borg/properties", &surfaces) {
            Some(SurfaceHit::Index { domain, classes }) => {
                assert_eq!(domain, "properties");
                assert_eq!(classes, vec!["Property".to_string(), "PropertyKey".to_string()]);
            }
            other => panic!("expected Index, got {:?}", other),
        }
        // /<mount>/<class-kebab> → that class, rewritten to its plural root
        assert_eq!(
            resolve_surface("/borg/properties/property", &surfaces),
            Some(SurfaceHit::Class { class_local: "Property".into(), rewritten_path: "/properties".into() })
        );
        assert_eq!(
            resolve_surface("/borg/properties/property-key", &surfaces),
            Some(SurfaceHit::Class { class_local: "PropertyKey".into(), rewritten_path: format!("/{}", pluralize("PropertyKey")) })
        );
        // sub-resource tail (e.g. an instance name) is preserved through the rewrite
        match resolve_surface("/borg/properties/property/some-key", &surfaces) {
            Some(SurfaceHit::Class { rewritten_path, .. }) => assert_eq!(rewritten_path, "/properties/some-key"),
            other => panic!("expected Class with tail, got {:?}", other),
        }
        // a non-surface path falls through to the primitive select_table
        assert_eq!(resolve_surface("/products/loom", &surfaces), None);
        assert_eq!(resolve_surface("/domains/properties", &surfaces), None);
    }

    #[test]
    fn page_html_is_a_generated_projection_on_system_css() {
        // #3420: page_html emits the SHELL of the real Athena domain page anatomy on the
        // #3415 design system; the shared /js/domain-renderer.js fills the mount points.
        let t = RouteTable {
            class: format!("{}Domain", NS),
            fields: vec!["label|plain".into(), "status|datatype:string".into()],
            routes: vec!["GET /domains".into()],
            secured: vec![],
            mandatory: vec![],
            repo_target: String::new(),
            exposure: vec![],
        };
        let h = page_html(&t);
        // projection doctrine — the generated marker says regenerate, never hand-edit
        assert!(h.contains("GENERATED by owl-api"), "must carry the generated marker");
        // renders into #3415's system.css vocabulary, not bespoke styling
        assert!(h.contains("/css/system.css"), "links the #3415 design system");
        assert!(h.contains("theme-light") && h.contains("class=\"wrap\""), "uses the #3415 shell + theme");
        assert!(h.contains("content-actions"), "carries the PDF/Share/Reflect chrome");
        // the REAL domain-page anatomy: breadcrumb + the identity/stats/promise/completeness/facets mount points
        assert!(h.contains("id=\"bc-domain\""), "breadcrumb (Athena › Step › Domain)");
        assert!(h.contains("id=\"stats-bar\""), "stats strip mount");
        assert!(h.contains("id=\"partof-block\""), "#3420 AC2 — the UPWARD (part-of) decomposition mount");
        assert!(h.contains("id=\"haschild-block\""), "#3351 — the DOWNWARD structural-recursion (hasChild) mount");
        assert!(h.contains("id=\"completeness-block\""), "completeness thermometer mount");
        assert!(h.contains("id=\"content-sections\""), "facet sections mount");
        // the shell loads the shared renderer (not inline rendering) — like content-actions.js
        assert!(h.contains("/js/domain-renderer.js"), "loads the shared renderer");
        // NO hardcoded host anywhere in the shell (#3415 portability doctrine)
        assert!(!h.contains("localhost:3360"), "no hardcoded host in the shell");
        // #3420 AC4 — the data-route security decision is RECORDED in the artifact, not just in conversation
        assert!(h.contains("DATA-ROUTE SECURITY DECISION"), "the per-route security decision travels with the page");
        // class projected from the table, not hardcoded (the breadcrumb is generic, not a literal "Domain")
        assert!(h.contains("Domain"), "titled by the class from the table");
        // deterministic — same table in, same page out (idempotent projection)
        assert_eq!(page_html(&t), page_html(&t));
        // #3420 AC6 — the breadcrumb/title are CLASS-projected (the generalization path for
        // services/roles), not a hardcoded "Domain". Prove it with a different class.
        let svc = page_html(&RouteTable {
            class: format!("{}Service", NS),
            fields: vec![],
            routes: vec![],
            secured: vec![],
            mandatory: vec![],
            repo_target: String::new(),
            exposure: vec![],
        });
        assert!(svc.contains("id=\"bc-domain\">Service</span>"), "breadcrumb projects the class (Service)");
        assert!(!svc.contains(">Domain</span>"), "a Service page never hardcodes Domain in the breadcrumb");
    }

    #[test]
    fn rejects_unsafe_entity_names_to_block_sparql_injection() {
        // #3420 code gate — name is interpolated into a SPARQL IRI; only bare local names allowed.
        for ok in ["cards-service", "build_domain", "Athena", "x"] {
            assert!(is_safe_local(ok), "{} should be a safe local name", ok);
        }
        // SPARQL/IRI metacharacters, whitespace, dots, slashes, empty → refused
        for bad in ["cards>service", "a b", "x\"y", "}", "a/b", "", "name#frag", "a.b"] {
            assert!(!is_safe_local(bad), "{} must be rejected (SPARQL-injection guard)", bad);
        }
    }

    // === #3453 — serve the generated OpenAPI spec + human view ===

    fn openapi_fixture() -> RouteTable {
        RouteTable {
            class: format!("{}Domain", NS),
            fields: vec!["comment".into(), "label".into()],
            mandatory: vec!["label".into()], // #3520 — exercises the `required` projection
            repo_target: String::new(),
            exposure: vec![],
            routes: vec![
                "GET /domains".into(),
                "GET /domains/:name".into(),
                "GET /domains/:name/contains".into(),
                "GET /schema/domain".into(),
            ],
            secured: vec![],
        }
    }

    #[test]
    fn openapi_json_route_serves_the_generated_spec() {
        let t = openapi_fixture();
        let (code, body) = handle("/openapi.json", &t);
        assert_eq!(code, 200);
        assert!(body.contains("\"openapi\": \"3.1.0\""), "must be an OpenAPI 3.1 doc (ADR-047 §7, #3520)");
        assert!(body.contains("\"required\": ["), "the completeness floor (t.mandatory) projects as `required` (#3520)");
        // AC2 — every generated read route appears (covers each edge type)
        assert!(body.contains("\"/domains\""), "list route present");
        assert!(body.contains("/domains/{name}"), "entity route present");
        assert!(body.contains("/domains/{name}/contains"), "contains edge present");
        assert!(body.contains("/schema/domain"), "schema route present");
    }

    #[test]
    fn openapi_json_is_generated_not_handwritten_regenerates_with_the_model() {
        // AC3 — add an edge type to the model (a new route) → it appears in the spec,
        // no hand-edit. The spec is a pure projection of table.routes.
        let mut t = openapi_fixture();
        assert!(!handle("/openapi.json", &t).1.contains("/domains/{name}/partof"));
        t.routes.push("GET /domains/:name/partof".into());
        assert!(handle("/openapi.json", &t).1.contains("/domains/{name}/partof"),
            "a new edge route must appear in the regenerated spec");
    }

    #[test]
    fn openapi_human_view_is_html_shell_that_fetches_the_spec() {
        let t = openapi_fixture();
        let (code, body) = handle("/openapi", &t);
        assert_eq!(code, 200);
        assert!(body.contains("<!doctype html"), "human view is HTML");
        assert!(body.contains("/openapi.json"), "shell client-fetches the live spec");
        assert!(body.contains("Domain"), "names the class");
    }

    #[test]
    fn content_type_for_html_view_vs_json_routes() {
        assert_eq!(content_type_for("/openapi"), "text/html; charset=utf-8");
        assert_eq!(content_type_for("/openapi.json"), "application/json");
        assert_eq!(content_type_for("/domains"), "application/json");
    }

    // === #3454 — write-route generation + typed-error taxonomy ===

    #[test]
    fn write_routes_generated_post_put_delete_per_edge() {
        let r = write_routes("domains");
        // entity lifecycle
        assert!(r.contains(&"POST /domains".to_string()), "create entity");
        assert!(r.contains(&"PUT /domains/:name".to_string()), "replace entity");
        assert!(r.contains(&"DELETE /domains/:name".to_string()), "delete entity");
        // per-edge add/remove (mirrors the read edges)
        for edge in ["partof", "contains", "has-child"] {
            assert!(r.contains(&format!("POST /domains/:name/{}", edge)), "add {} edge", edge);
            assert!(r.contains(&format!("DELETE /domains/:name/{}", edge)), "remove {} edge", edge);
        }
        // pluralization flows through (mirrors read-route generation)
        assert!(write_routes("properties").contains(&"POST /properties".to_string()));
    }

    #[test]
    fn role_from_webid_extracts_role_or_none() {
        assert_eq!(role_from_webid("http://localhost:3000/pods/chorus/_agents/wren/profile/card.ttl#me").as_deref(), Some("wren"));
        assert_eq!(role_from_webid("http://localhost:3000/pods/chorus/_agents/silas/profile/card.ttl#me").as_deref(), Some("silas"));
        assert_eq!(role_from_webid("https://example.com/nobody"), None);
    }

    #[test]
    fn parse_write_maps_method_and_shape() {
        assert_eq!(parse_write("POST", "/domains", "domains"), Some(WriteOp::CreateEntity));
        assert_eq!(parse_write("PUT", "/domains/x", "domains"), Some(WriteOp::ReplaceEntity { name: "x".into() }));
        assert_eq!(parse_write("DELETE", "/domains/x", "domains"), Some(WriteOp::DeleteEntity { name: "x".into() }));
        assert_eq!(parse_write("POST", "/domains/x/partof", "domains"), Some(WriteOp::AddEdge { name: "x".into(), edge: "partof".into() }));
        assert_eq!(parse_write("DELETE", "/domains/x/partof", "domains"), Some(WriteOp::RemoveEdge { name: "x".into(), edge: "partof".into() }));
        assert_eq!(parse_write("POST", "/widgets", "domains"), None);
        assert_eq!(parse_write("POST", "/domains/x/y/z", "domains"), None);
    }

    #[test]
    fn authz_allows_is_fail_closed() {
        assert!(authz_allows("wren", Some("wren")));
        assert!(!authz_allows("wren", Some("silas")));
        assert!(!authz_allows("wren", None));        // absent ownedBy → FAIL-CLOSED
        assert!(!authz_allows("wren", Some("")));
    }

    #[test]
    fn edge_predicate_and_single_valued() {
        assert_eq!(edge_predicate("partof"), Some("partOf"));
        assert_eq!(edge_predicate("contains"), Some("contains"));
        assert_eq!(edge_predicate("has-child"), Some("hasChild"));
        assert_eq!(edge_predicate("bogus"), None);
        assert!(edge_is_single_valued("partof"));
        assert!(!edge_is_single_valued("contains"));
    }

    #[test]
    fn parse_body_target_pulls_target() {
        assert_eq!(parse_body_target(r#"{"target":"parentnode"}"#).as_deref(), Some("parentnode"));
        assert_eq!(parse_body_target(r#"{ "target" : "p2" , "x": 1 }"#).as_deref(), Some("p2"));
        assert_eq!(parse_body_target(r#"{"other":"x"}"#), None);
    }

    #[test]
    fn collect_entity_props_takes_datatype_fields_skips_edges() {
        let fields = vec!["label|datatype:string".to_string(), "status|plain".to_string(), "partOf|edge:Domain".to_string()];
        let props = collect_entity_props(r#"{"name":"x","label":"My Label","status":"active","partOf":"shouldskip"}"#, &fields);
        assert!(props.contains(&("label".to_string(), "My Label".to_string())));
        assert!(props.contains(&("status".to_string(), "active".to_string())));
        // edge fields are NOT written via the entity body (they go through edge endpoints)
        assert!(!props.iter().any(|(f, _)| f == "partOf"));
    }

    // (build_create_entity / build_replace_entity / sparql_lit tests retired with
    // their fns — writes delegate to the DAL, owl-api builds no raw SPARQL. #3468)

    #[test]
    fn write_status_typed_taxonomy_no_silent_200() {
        assert_eq!(write_status("created"), (201, "created"));
        assert_eq!(write_status("authn-missing"), (401, "authn-missing"));
        assert_eq!(write_status("authz"), (403, "authz"));
        assert_eq!(write_status("conflict"), (409, "conflict"));   // 2nd parent on single-valued partOf
        assert_eq!(write_status("validation"), (422, "validation"));
        assert_eq!(write_status("not-found"), (404, "not-found"));
        // the honest interim: generated-not-yet-executing is a typed 501, never a silent read-200
        assert_eq!(write_status("not-implemented"), (501, "not-implemented"));
        assert_eq!(write_status("anything-unknown"), (501, "not-implemented"));
    }

    #[test]
    fn routes_json_is_deterministic() {
        let t = RouteTable {
            class: format!("{}Domain", NS),
            fields: vec!["comment".into(), "label".into()],
            routes: vec!["GET /domains".into()],
            secured: vec!["/schema/domain".into()],
            mandatory: vec!["label".into(), "comment".into()],
            repo_target: String::new(),
            exposure: vec![],
        };
        assert_eq!(routes_json(&t), routes_json(&t));
        assert!(routes_json(&t).contains("\"generatedFrom\""));
    }

    // === #3468 — the completeness FLOOR (100% at write) + migration gauge ===

    #[test]
    fn missing_mandatory_flags_absent_and_empty_sections() {
        // The floor's verdict: a mandatory section is satisfied ONLY by a present,
        // NON-EMPTY value. Absent and blank both count as missing (no "I'll fill it
        // later" — the graded human-era tier is gone). Order follows the mandatory set.
        let mandatory: Vec<String> = vec!["identity".into(), "promise".into(), "value".into()];
        let present = vec![
            ("identity".to_string(), "Athena".to_string()),
            ("promise".to_string(), "   ".to_string()), // blank → NOT satisfied
            ("unrelated".to_string(), "x".to_string()), // extra props don't help
        ];
        assert_eq!(
            missing_mandatory(&present, &mandatory),
            vec!["promise".to_string(), "value".to_string()],
            "blank 'promise' + absent 'value' are both missing"
        );
        let full = vec![
            ("identity".to_string(), "a".to_string()),
            ("promise".to_string(), "b".to_string()),
            ("value".to_string(), "c".to_string()),
        ];
        assert!(missing_mandatory(&full, &mandatory).is_empty(), "all present → nothing missing");
        assert!(missing_mandatory(&present, &[]).is_empty(), "no floor → vacuously satisfied");
    }

    #[test]
    fn completeness_is_a_migration_gauge_not_a_gate() {
        // AC4 — completeness MEASURES distance to the 100% floor; it never blocks a read.
        let mandatory: Vec<String> = vec!["a".into(), "b".into(), "c".into(), "d".into()];
        let partial = vec![("a".to_string(), "1".to_string()), ("b".to_string(), "2".to_string())];
        let (met, pct, have, miss) = completeness(&partial, &mandatory);
        assert!(!met, "2 of 4 mandatory → not met");
        assert_eq!(pct, 50, "2/4 → 50%");
        assert_eq!(have, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(miss, vec!["c".to_string(), "d".to_string()]);
        let full = vec![
            ("a".to_string(), "1".to_string()), ("b".to_string(), "2".to_string()),
            ("c".to_string(), "3".to_string()), ("d".to_string(), "4".to_string()),
        ];
        let (met2, pct2, _, _) = completeness(&full, &mandatory);
        assert!(met2 && pct2 == 100, "all mandatory → met, 100%");
        assert_eq!(completeness(&[], &[]).1, 100, "a shape with no floor is vacuously complete");
    }

    #[test]
    fn routes_json_publishes_the_mandatory_floor() {
        // AC4/AC5 — the floor is part of the published /schema contract so the page
        // meter sources completeness from the MODEL (severing the Athena-v1 dependency).
        let t = RouteTable {
            class: format!("{}Domain", NS),
            fields: vec!["label".into(), "comment".into()],
            routes: vec!["GET /domains".into()],
            secured: vec![],
            mandatory: vec!["label".into(), "comment".into()],
            repo_target: String::new(),
            exposure: vec![],
        };
        let j = routes_json(&t);
        assert!(j.contains("\"mandatory\": [\"label\", \"comment\"]"),
            "the mandatory floor is published in /schema, got: {}", j);
    }

    #[test]
    fn pluralize_handles_english_irregulars() {
        // the bug Jeff caught: Property → propertys. consonant + y → ies.
        assert_eq!(pluralize("property"), "properties");
        assert_eq!(pluralize("Property"), "properties");
        // regular: just add s (Domain must NOT regress — it serves /domains on prod)
        assert_eq!(pluralize("domain"), "domains");
        assert_eq!(pluralize("service"), "services");
        assert_eq!(pluralize("valuestream"), "valuestreams");
        // sibilants take -es
        assert_eq!(pluralize("class"), "classes");
        assert_eq!(pluralize("box"), "boxes");
        // vowel + y is regular (not ies)
        assert_eq!(pluralize("day"), "days");
    }

    #[test]
    fn unknown_route_404s_and_teaches_routes() {
        let t = RouteTable { class: format!("{}Domain", NS), fields: vec![], routes: vec!["GET /domains".into()], secured: vec![], mandatory: vec![], repo_target: String::new(), exposure: vec![] };
        let (code, body) = handle("/nope", &t);
        assert_eq!(code, 404);
        assert!(body.contains("GET /domains"));
    }
}
