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
}

impl TelemetryLine {
    pub fn to_jsonl(&self, ts_ms: u128) -> String {
        format!(
            "{{\"ts\":{},\"event\":\"api.request.served\",\"service\":\"owl-api\",\"class\":\"{}\",\"entity\":\"{}\",\"route\":\"{}\",\"fold\":\"{}\",\"status\":\"{}\",\"result_count\":{},\"total_ms\":{},\"upstream_ms\":{},\"caller\":\"{}\",\"trace_id\":\"{}\"}}\n",
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
            json_escape(&self.trace_id)
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
    Ok(RouteTable { class, fields, routes, secured, mandatory })
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

/// OpenAPI 3.0.3 contract (#3364 AC2) — generated from the same shapes as the
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
    format!(
        "{{\n  \"openapi\": \"3.0.3\",\n  \"info\": {{ \"title\": \"OWL API — generated {class_short} API\", \"version\": \"0\", \"description\": \"Generated from {class} shapes in {graph}. Regenerate, never hand-edit (#3354).\" }},\n  \"paths\": {{\n{paths}\n  }},\n  \"components\": {{ \"schemas\": {{\n    \"EdgeRef\": {{ \"type\": \"object\", \"properties\": {{ \"name\": {{ \"type\": \"string\" }}, \"label\": {{ \"type\": \"string\" }} }} }},\n    \"{class_short}\": {{ \"type\": \"object\", \"properties\": {{ {props} }} }},\n    \"List\": {{ \"type\": \"object\", \"properties\": {{ \"count\": {{ \"type\": \"integer\" }}, \"items\": {{ \"type\": \"array\", \"items\": {{ \"type\": \"object\", \"properties\": {{ \"name\": {{ \"type\": \"string\" }}, \"label\": {{ \"type\": \"string\" }}, \"status\": {{ \"type\": \"string\" }} }} }} }} }} }},\n    \"Fold\": {{ \"type\": \"object\", \"properties\": {{ \"{class_l}\": {{ \"type\": \"string\" }}, \"count\": {{ \"type\": \"integer\" }}, \"contains\": {{ \"type\": \"array\", \"items\": {{ \"type\": \"string\" }} }} }} }},\n    \"Schema\": {{ \"type\": \"object\" }}\n  }} }}\n}}\n",
        class_short = class_short,
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
    format!(
        "{{\n  \"class\": \"{class}\",\n  \"plural\": \"{plural}\",\n  \"unit\": {{ \"routes\": [{routes}], \"mandatory\": [{mandatory}], \"secured\": [{secured}] }},\n  \"conformance\": [\n    {conf}\n  ],\n  \"security\": [\n    {sec}\n  ]\n}}\n",
        class = json_escape(&class), plural = json_escape(&plural),
        routes = arr(&t.routes), mandatory = arr(&t.mandatory), secured = arr(&t.secured),
        conf = conformance.join(",\n    "), sec = security.join(",\n    ")
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
fn entity_json(name: &str) -> R<String> {
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
    let mut parts = vec![format!("\"iri\": \"{}\"", json_escape(&subject))];
    for rowv in prs {
        let (p, o) = match rowv.split_once('|') { Some((a, b)) => (a.to_string(), b.to_string()), None => continue };
        let key = p.rsplit(['#', '/']).next().unwrap_or(&p).to_string();
        if o.starts_with(NS) {
            // EDGE RESOLUTION (#3354 AC2): linked entities return name + label,
            // not a bare fragment — one extra lookup per edge, detail-route only.
            let target_name = o.rsplit('#').next().unwrap_or(&o).to_string();
            let label = sparql_json(&format!(
                "SELECT ?v WHERE {{ GRAPH <{g}> {{ <{o}> <{ns}label> ?l }} BIND(STR(?l) AS ?v) }}",
                g = INSTANCES_GRAPH, o = o, ns = NS
            )).ok().map(|b| select_v(&b).into_iter().next().unwrap_or_default()).unwrap_or_default();
            parts.push(format!(
                "\"{}\": {{ \"name\": \"{}\", \"label\": \"{}\" }}",
                json_escape(&key), json_escape(&target_name), json_escape(&label)
            ));
        } else if o.starts_with("http") && o.contains('#') {
            parts.push(format!("\"{}\": \"{}\"", json_escape(&key), json_escape(o.rsplit('#').next().unwrap_or(&o))));
        } else {
            parts.push(format!("\"{}\": \"{}\"", json_escape(&key), json_escape(&o)));
        }
    }
    Ok(format!("{{ {} }}", parts.join(", ")))
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
    handle_meta(path, table).0
}

/// handle + envelope metadata (the seam's data source).
pub fn handle_meta(path: &str, table: &RouteTable) -> ((u16, String), ReqMeta) {
    let mut meta = ReqMeta::default();
    // /health — the probe target (blackbox-exporter, launchagent checks).
    if path == "/health" {
        meta.route = "health".into();
        return ((200, "{ \"ok\": true, \"service\": \"owl-api\" }".to_string()), meta);
    }
    let resp = handle_inner(path, table, &mut meta);
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

fn handle_inner(path: &str, table: &RouteTable, meta: &mut ReqMeta) -> (u16, String) {
    let plural = format!("/{}", pluralize(table.class.rsplit('#').next().unwrap_or("domain")));
    let parts: Vec<&str> = path.trim_end_matches('/').split('/').filter(|s| !s.is_empty()).collect();

    // GET /schema/domain
    if path.starts_with("/schema/") {
        meta.route = "schema".into();
        let t = RouteTable { class: table.class.clone(), fields: table.fields.clone(), routes: table.routes.clone(), secured: table.secured.clone(), mandatory: table.mandatory.clone() };
        return (200, routes_json(&t));
    }
    // GET /openapi.json — the generated OpenAPI 3.0.3 spec (#3453). Another
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
                    meta.result_count = items.len() as i64;
                    (200, format!("{{ \"count\": {}, \"items\": [\n  {}\n] }}", items.len(), items.join(",\n  ")))
                }
            }
            Err(e) => (502, format!("{{ \"error\": \"{}\" }}", json_escape(&e))),
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
            let q = format!(
                "SELECT ?v WHERE {{ GRAPH <{g}> {{ <{ns}{n}> <{ns}contains> ?o }} BIND(STR(?o) AS ?v) }}",
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
        return match entity_json(name) {
            Ok(j) => {
                meta.result_count = 1;
                (200, j)
            }
            Err(e) if e == "not-found" => (404, format!("{{ \"error\": \"no such domain: {}\" }}", json_escape(name))),
            Err(e) => (502, format!("{{ \"error\": \"{}\" }}", json_escape(&e))),
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

pub fn serve(port: u16, table: &RouteTable) -> R<()> {
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("bind {}: {}", port, e))?;
    eprintln!("owl-api: serving generated {} API on :{} (read-only; writes go through chorus-model)", table.class, port);
    let mut req_counter: u64 = 0;
    // #3402 — seam auth config, loaded ONCE (no per-request env read, no graph call).
    let secret = std::env::var("CHORUS_SERVICE_TOKEN_SECRET").unwrap_or_default().into_bytes();
    let allowed_webids = auth::chorus_agent_webids();
    if secret.is_empty() {
        // fail-closed (verify rejects), but say so loudly — secured surfaces will 401.
        eprintln!("owl-api: WARNING — CHORUS_SERVICE_TOKEN_SECRET unset; secured surfaces will reject ALL requests (fail-closed).");
    }
    for stream in listener.incoming() {
        let mut stream = match stream { Ok(s) => s, Err(_) => continue };
        let started = std::time::Instant::now();
        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf).unwrap_or(0);
        let req = String::from_utf8_lossy(&buf[..n]);
        let path = req.lines().next().and_then(|l| l.split_whitespace().nth(1)).unwrap_or("/").to_string();
        let method = req.lines().next().and_then(|l| l.split_whitespace().next()).unwrap_or("GET").to_string();
        let header = |name: &str| -> String {
            req.lines()
                .find(|l| l.to_ascii_lowercase().starts_with(&format!("{}:", name)))
                .map(|l| l.splitn(2, ':').nth(1).unwrap_or("").trim().to_string())
                .unwrap_or_default()
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
                None => handle_meta(&path, table),
            }
        };
        let upstream_ms = upstream_started.elapsed().as_millis();
        let status = match code {
            200 => "200 OK",
            201 => "201 Created",
            401 => "401 Unauthorized",
            403 => "403 Forbidden",
            404 => "404 Not Found",
            409 => "409 Conflict",
            422 => "422 Unprocessable Entity",
            501 => "501 Not Implemented",
            _ => "502 Bad Gateway",
        };
        let resp = http_response_ct(status, &body, content_type_for(&path));
        let _ = stream.write_all(resp.as_bytes());
        // THE SEAM: every request passes here once — telemetry now; auth,
        // validation, rate limits inject at this same point (the IoC payoff).
        if path != "/health" { // probes are noise, not signal
            emit_telemetry(&TelemetryLine {
                class: table.class.rsplit('#').next().unwrap_or("").to_string(),
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
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn page_html_is_a_generated_projection_on_system_css() {
        // #3420: page_html emits the SHELL of the real Athena domain page anatomy on the
        // #3415 design system; the shared /js/domain-renderer.js fills the mount points.
        let t = RouteTable {
            class: format!("{}Domain", NS),
            fields: vec!["label|plain".into(), "status|datatype:string".into()],
            routes: vec!["GET /domains".into()],
            secured: vec![],
            mandatory: vec![],
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
            mandatory: vec![],
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
        assert!(body.contains("\"openapi\": \"3.0.3\""), "must be an OpenAPI 3.0.3 doc");
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
        let t = RouteTable { class: format!("{}Domain", NS), fields: vec![], routes: vec!["GET /domains".into()], secured: vec![], mandatory: vec![] };
        let (code, body) = handle("/nope", &t);
        assert_eq!(code, 404);
        assert!(body.contains("GET /domains"));
    }
}
