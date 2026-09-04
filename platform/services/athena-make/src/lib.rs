//! athena-make — the OWL→API generator + server (#3350, the model-5 spike).
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

use std::io::Write;
use std::net::TcpListener;
use std::process::Command;

/// #3402 — seam auth: local HS256 service-token verification (ADR-042 / #3401).
pub mod auth;
pub mod reconcile; // #3723 — the model reconciliation readout
pub mod oidc; // #3613 / ADR-052 — ES256/JWKS (Solid-OIDC via CSS) verify at the seam

pub const NS: &str = "https://jeffbridwell.com/chorus#";
pub const ONTOLOGY_GRAPH: &str = "urn:chorus:ontology";
pub const INSTANCES_GRAPH: &str = "urn:chorus:instances";
/// #3506 / ADR-047 — the response-contract version. Coarse, infrastructure-wide;
/// path-prefixed (/v1/...) AND echoed in every envelope. Bumps only when the
/// envelope shape changes — orthogonal to a primitive's per-shape `shapeVersion`.
pub const API_VERSION: &str = "v1";

// #3435 — re-export the pure resolver surface (#3437) so the handler and consumers
// import one place: athena_make::{ScopeKind, ScopeNode, decide_effective_value, ...}.
pub use properties_resolver::{
    decide_effective_value, CascadeError, PropertyDatum, Resolution, ScopeKind, ScopeNode,
};

pub type R<T> = Result<T, String>;

fn fuseki() -> String {
    std::env::var("CHORUS_FUSEKI").unwrap_or_else(|_| "http://localhost:3030/pods".to_string())
}

/// #4022 — the curl argv for a SPARQL query. The query text travels on STDIN as
/// a raw `application/sparql-query` POST body (SPARQL 1.1 protocol §2.1.3),
/// never on argv and never url-encoded: `GET /testresults?limit=100000` builds
/// a VALUES block over 100k subjects (~10 MB). On argv that failed at spawn with
/// `Argument list too long (os error 7)`; url-encoded through curl it failed with
/// `--data-urlencode: out of memory`. Both were 502s whose cause was the
/// transport, not the store. Pure so the proof can assert the query is absent
/// from argv without a Fuseki.
pub fn sparql_curl_args(endpoint: &str) -> Vec<String> {
    vec![
        "-sf".into(), "--max-time".into(), "60".into(),
        "-H".into(), "Accept: application/sparql-results+json".into(),
        "-H".into(), "Content-Type: application/sparql-query".into(),
        "--data-binary".into(), "@-".into(),
        format!("{}/query", endpoint),
    ]
}

pub fn sparql_json(query: &str) -> R<String> {
    sparql_json_at(&fuseki(), query)
}

pub fn sparql_json_at(endpoint: &str, query: &str) -> R<String> {
    use std::io::Write;
    let mut child = Command::new("curl")
        .args(sparql_curl_args(endpoint))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("curl-spawn: {}", e))?;
    if let Some(mut stdin) = child.stdin.take() {
        // A closed pipe (curl already gone) surfaces below as a failed status,
        // with curl's own stderr — not as a spawn error.
        let _ = stdin.write_all(query.as_bytes());
    }
    let out = child.wait_with_output().map_err(|e| format!("curl-spawn: {}", e))?;
    if !out.status.success() {
        return Err(format!("fuseki-query failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// The DAL's proven extractor: all bound values of the single ?v variable.
/// Multi-column queries CONCAT their columns into ?v with a `|` separator —
/// the single-var seam is what makes zero-dep parsing reliable.
/// #3635 — values are JSON strings and MUST be unescaped: Fuseki ASCII-escapes
/// non-ASCII (an em dash arrives as the text `—`); the old substring scan
/// passed the escape through, json_escape then doubled the backslash, and every
/// page rendered the escape sequence literally. The scan also now respects
/// escaped quotes when finding the closing quote.
pub fn select_v(body: &str) -> Vec<String> {
    let mut vals = Vec::new();
    for chunk in body.split("\"v\"").skip(1) {
        if let Some(i) = chunk.find("\"value\"") {
            let rest = &chunk[i + 7..];
            if let Some(start) = rest.find('"') {
                let rest = &rest[start + 1..];
                if let Some(raw) = scan_json_string(rest) {
                    vals.push(json_unescape(raw));
                }
            }
        }
    }
    vals
}

/// Slice up to the closing quote of a JSON string, honoring backslash escapes.
fn scan_json_string(rest: &str) -> Option<&str> {
    let bytes = rest.as_bytes();
    let mut esc = false;
    for (i, &c) in bytes.iter().enumerate() {
        if esc {
            esc = false;
        } else if c == b'\\' {
            esc = true;
        } else if c == b'"' {
            return Some(&rest[..i]);
        }
    }
    None
}

/// Decode JSON string escapes (zero-dep): \" \\ \/ \n \r \t \uXXXX incl.
/// surrogate pairs. Unknown escapes pass through verbatim rather than erroring —
/// a read path must not refuse data it can still show.
fn json_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('b') => out.push('\u{0008}'),
            Some('f') => out.push('\u{000C}'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('/') => out.push('/'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                match u32::from_str_radix(&hex, 16) {
                    Ok(cp) if (0xD800..0xDC00).contains(&cp) => {
                        // high surrogate — pair with the following \uXXXX low half
                        let mut ahead = chars.clone();
                        let paired = (ahead.next() == Some('\\') && ahead.next() == Some('u'))
                            .then(|| ahead.by_ref().take(4).collect::<String>())
                            .and_then(|h2| u32::from_str_radix(&h2, 16).ok())
                            .filter(|lo| (0xDC00..0xE000).contains(lo))
                            .and_then(|lo| char::from_u32(0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00)));
                        if let Some(ch) = paired {
                            out.push(ch);
                            chars = ahead;
                        } else {
                            out.push('\u{FFFD}');
                        }
                    }
                    Ok(cp) => out.push(char::from_u32(cp).unwrap_or('\u{FFFD}')),
                    Err(_) => {
                        out.push('\\');
                        out.push('u');
                        out.push_str(&hex);
                    }
                }
            }
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
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
            "{{\"ts\":{},\"event\":\"api.request.served\",\"service\":\"athena-make\",\"class\":\"{}\",\"entity\":\"{}\",\"route\":\"{}\",\"fold\":\"{}\",\"status\":\"{}\",\"result_count\":{},\"total_ms\":{},\"upstream_ms\":{},\"caller\":\"{}\",\"trace_id\":\"{}\",\"product\":\"{}\",\"apiVersion\":\"{}\",\"shapeVersion\":\"{}\",\"commit\":\"{}\"}}\n",
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

/// Dated telemetry path: ops/logs/athena-make-YYYYMMDD.jsonl under CHORUS_HOME.
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
    format!("{}/ops/logs/athena-make-{:04}{:02}{:02}.jsonl", home, y, m, d)
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
        eprintln!("athena-make: telemetry append failed for {}", path);
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
    pub write_required: Vec<String>, // every sh:minCount>=1 property, including required edges; drives create contracts
    pub repo_target: String,     // #3488 — repo land location for generated artifacts, from chorus:repoTarget (or class-keyed default)
    pub exposure: Vec<(String, String)>, // #3506/ADR-048 §3 — field localname → exposure level (public|internal|secret), PROJECTED from chorus:exposure. Unmarked = hidden (fail-closed).
    pub instances_graph: String, // #3570 — the kind's instance HOME graph (the domains.* spine): chorus:instancesGraph override, else urn:chorus:domains:<domain>, else urn:chorus:instances (back-compat). Threaded into every serve read.
    pub tree_edges: Vec<String>, // #3660 — recursive-descent edge localnames, PROJECTED from chorus:treeEdge on the shape. Empty = no /tree read emitted.
    pub tree_order: Option<String>, // #3660 — sibling rank property localname (chorus:treeOrder). None = unordered (label sort fallback).
    pub model_version: String,   // #3704/#3706 — PROJECTED from chorus:modelVersion on the class. "target"=reviewed-canonical, "legacy"=reviewed-strangled; transitional literals "v1"/"v2" persist until the review pass (v1≈legacy, v2≈claimed-current-unreviewed). ABSENT → "unclassified": nobody has reviewed the class, and it must never render as current (born-v2 removed 2026-07-30, Jeff's ruling).
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

/// #3675 — the collection-side twin of entity_json's exposure gate. Same per-shape
/// opt-in: a shape with NO exposure annotations passes everything (migration-safe);
/// an annotated shape keeps only fields whose level passes field_exposed for this
/// caller. Pure + unit-pinned; applied before the projection is queried.
pub fn exposed_projection(
    fields: Vec<(String, bool)>,
    exposure: &[(String, String)],
    authed: bool,
) -> Vec<(String, bool)> {
    if exposure.is_empty() {
        return fields;
    }
    let level_of = |k: &str| exposure.iter().find(|(f, _)| f == k).map(|(_, l)| l.as_str());
    fields
        .into_iter()
        .filter(|(n, _)| field_exposed(level_of(n), authed))
        .collect()
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

/// #3551 — kebab/snake verb localname → PascalCase trait stem (athena-deploy → AthenaDeploy).
pub fn to_pascal(verb_local: &str) -> String {
    verb_local
        .split(|c| c == '-' || c == '_')
        .filter(|s| !s.is_empty())
        .map(|s| {
            let mut ch = s.chars();
            match ch.next() {
                Some(f) => f.to_ascii_uppercase().to_string() + ch.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// #3551 — map a VerbShape I/O type token (the `datatype:X` / `edge:X` form the shape
/// projects, mirroring RouteTable field encoding) to its Rust type. Unknown → String
/// (fail-soft to an opaque carrier, never a bad type).
pub fn rust_type(tok: &str) -> String {
    let t = tok.strip_prefix("datatype:").or_else(|| tok.strip_prefix("edge:")).unwrap_or(tok);
    match t {
        "integer" | "int" | "long" => "i64",
        "decimal" | "double" | "float" => "f64",
        "boolean" | "bool" => "bool",
        "string" => "String",
        _ => "String", // edge:Class IRIs + unknowns carried as String (the node's IRI)
    }
    .to_string()
}

/// #3551 — the Generation-Gap SEAM, projected. Emits the Rust trait the hand-written
/// verb logic (`<verb>.rs`) MUST implement, with its signature derived from the verb's
/// typed inputs/outputs. The trait signature IS the verb contract (ADR-032) made
/// physical: a model I/O change re-projects this signature, so a stale hand-impl
/// FAILS TO COMPILE — the compiler is the drift gate. athena-make writes this into
/// `<verb>_generated.rs` (regenerated freely); the human owns the impl in `<verb>.rs`
/// (never clobbered). Pure projection, unit-pinned — the SPARQL read of the VerbShape
/// in generate_verb() is integration-proven separately (mirrors project_secured).
/// Inputs/outputs are (name, type-token) pairs as projected from the shape.
pub fn verb_trait_signature(
    verb_local: &str,
    inputs: &[(String, String)],
    outputs: &[(String, String)],
) -> String {
    let stem = to_pascal(verb_local);
    let args = inputs
        .iter()
        .map(|(n, ty)| format!("{}: {}", n, rust_type(ty)))
        .collect::<Vec<_>>()
        .join(", ");
    let ret = match outputs {
        [] => "()".to_string(),
        [(_, ty)] => rust_type(ty),
        many => format!(
            "({})",
            many.iter().map(|(_, ty)| rust_type(ty)).collect::<Vec<_>>().join(", ")
        ),
    };
    let arg_sep = if args.is_empty() { "" } else { ", " };
    format!(
        "/// GENERATED seam (#3551) — implement in {verb}.rs. Signature projected from the\n\
         /// VerbShape; a model contract change re-projects it, so a stale impl will not compile.\n\
         pub trait {stem}Logic {{\n    \
         fn run(&self{sep}{args}) -> R<{ret}>;\n}}",
        verb = verb_local,
        stem = stem,
        sep = arg_sep,
        args = args,
        ret = ret,
    )
}

/// #3551 — the projected verb (the `generate()→RouteTable` analog for the `verb` target).
/// Each field is PROJECTED from the VerbShape; the emitter consumes it. `edges` is the
/// wire-before-make placement (predicate, target-localname) — vs-step/product/domain/service.
#[derive(Debug, Clone, Default)]
pub struct VerbTable {
    pub verb: String,                  // athena-deploy
    pub family: String,                // athena
    pub invocability: String,          // invoked | woven
    pub inputs: Vec<(String, String)>, // (name, type-token: datatype:X / edge:X)
    pub outputs: Vec<(String, String)>,
    pub edges: Vec<(String, String)>,  // (predicate, target localname) — the model wiring
    pub repo_target: String,           // where <verb>_generated.rs + <verb>.rs land
}

/// #3551 — `athena-make verb`'s emitter: project the GENERATED half of a verb crate
/// (`<verb>_generated.rs`) from a VerbTable. athena-make owns + regenerates this file freely;
/// the human owns `<verb>.rs` (the `<Stem>Logic` impl) — NEVER touched here (skeleton-seed).
/// The Generation Gap: this file declares the trait seam + the woven scaffold (parse →
/// trace → run(handwritten) → emit); a model contract change re-projects the seam, so a
/// stale hand-impl fails to compile. Pure (no I/O) so the structure is unit-pinned; the
/// SPARQL read of the VerbShape in generate_verb() is integration-proven separately.
pub fn verb_skeleton_rust(t: &VerbTable) -> String {
    let stem = to_pascal(&t.verb);
    let ret = match t.outputs.as_slice() {
        [] => "()".to_string(),
        [(_, ty)] => rust_type(ty),
        many => format!(
            "({})",
            many.iter().map(|(_, ty)| rust_type(ty)).collect::<Vec<_>>().join(", ")
        ),
    };
    let mut s = String::new();
    s.push_str("// GENERATED by athena-make `athena-make verb` (#3551) — DO NOT EDIT.\n");
    s.push_str(&format!(
        "// Regenerated from the VerbShape; hand logic lives in {v}.rs (impl {stem}Logic).\n",
        v = t.verb, stem = stem
    ));
    s.push_str(&format!("// family={} · invocability={}\n\n", t.family, t.invocability));
    s.push_str("use crate::R;\n\n");
    // the seam — the contract the handwritten logic implements (the drift gate)
    s.push_str(&verb_trait_signature(&t.verb, &t.inputs, &t.outputs));
    s.push_str("\n\n");
    // typed inputs, projected
    s.push_str(&format!("#[derive(Debug, Clone)]\npub struct {stem}Input {{\n", stem = stem));
    if t.inputs.is_empty() {
        s.push_str("    // (no inputs)\n");
    }
    for (n, ty) in &t.inputs {
        s.push_str(&format!("    pub {}: {},\n", n, rust_type(ty)));
    }
    s.push_str("}\n\n");
    // wire-before-make: the model edges as registration data
    s.push_str("/// wire-before-make: the verb's model placement, projected from the graph.\n");
    s.push_str("pub const WIRING: &[(&str, &str)] = &[\n");
    for (p, tgt) in &t.edges {
        s.push_str(&format!("    (\"{}\", \"{}\"),\n", p, tgt));
    }
    s.push_str("];\n\n");
    // arg-parse: positional argv → typed input
    s.push_str(&format!(
        "pub fn parse_args(argv: &[String]) -> R<{stem}Input> {{\n    Ok({stem}Input {{\n",
        stem = stem
    ));
    for (i, (n, ty)) in t.inputs.iter().enumerate() {
        let rt = rust_type(ty);
        if rt == "String" {
            s.push_str(&format!(
                "        {n}: argv.get({i}).cloned().ok_or_else(|| \"missing arg: {n}\".to_string())?,\n",
                n = n, i = i
            ));
        } else {
            s.push_str(&format!(
                "        {n}: argv.get({i}).ok_or_else(|| \"missing arg: {n}\".to_string())?.parse::<{rt}>().map_err(|e| e.to_string())?,\n",
                n = n, i = i, rt = rt
            ));
        }
    }
    s.push_str("    })\n}\n\n");
    // dispatch: the woven aspect scaffold (trace/emit) around the handwritten run()
    s.push_str("/// dispatch: the woven aspect scaffold (trace in / emit out) wrapping the\n");
    s.push_str("/// handwritten logic. athena-make owns this; the run() body lives in the seam impl.\n");
    s.push_str(&format!(
        "pub fn dispatch<L: {stem}Logic>(logic: &L, argv: &[String]) -> R<{ret}> {{\n",
        stem = stem, ret = ret
    ));
    s.push_str("    let input = parse_args(argv)?;\n");
    s.push_str(&format!("    eprintln!(\"[trace] {} start\");\n", t.verb));
    let call_args = t
        .inputs
        .iter()
        .map(|(n, _)| format!("input.{}", n))
        .collect::<Vec<_>>()
        .join(", ");
    s.push_str(&format!("    let out = logic.run({});\n", call_args));
    s.push_str(&format!("    eprintln!(\"[emit] {} done\");\n", t.verb));
    s.push_str("    out\n}\n");
    s
}

/// #3551 — `athena-make verb`'s graph-read half (AC1/AC2): read a VerbShape instance
/// `chorus:verb-<local>` from the ontology graph, project its VerbTable, and emit
/// `<verb>_generated.rs` via verb_skeleton_rust. This is the SAME read-shape → emit →
/// place spine the api/mcp/page targets share — the `verb` target is one more shape the
/// engine projects, not a switch wearing one name. inputs/outputs/edges are encoded as
/// `name|type` / `predicate|target` literals (the flat, queryable VerbTable contract).
pub fn generate_verb(verb_local: &str) -> R<String> {
    // graph is config-as-data (VERB_GRAPH env) defaulting to the shared ontology graph;
    // overridable so the verb contract can be read from an isolated graph in tests/CI
    // without touching shared multi-role state.
    let graph = std::env::var("VERB_GRAPH").unwrap_or_else(|_| ONTOLOGY_GRAPH.to_string());
    let iri = format!("{}verb-{}", NS, verb_local);
    let one = |pred: &str| -> R<Option<String>> {
        let q = format!(
            "SELECT ?v WHERE {{ GRAPH <{g}> {{ <{iri}> <{ns}{pred}> ?v }} }}",
            g = graph, iri = iri, ns = NS, pred = pred
        );
        Ok(select_v(&sparql_json(&q)?).into_iter().next())
    };
    let many = |pred: &str| -> R<Vec<String>> {
        let q = format!(
            "SELECT ?v WHERE {{ GRAPH <{g}> {{ <{iri}> <{ns}{pred}> ?v }} }} ORDER BY ?v",
            g = graph, iri = iri, ns = NS, pred = pred
        );
        Ok(select_v(&sparql_json(&q)?))
    };
    let family = one("verbFamily")?.ok_or_else(|| {
        format!("no VerbShape instance <{}> in {} — wire the shape before make", iri, graph)
    })?;
    let invocability = one("invocability")?.unwrap_or_else(|| "invoked".to_string());
    let repo_target = one("repoTarget")?.unwrap_or_default();
    let split_pair = |s: &String| -> (String, String) {
        let mut it = s.splitn(2, '|');
        (it.next().unwrap_or("").to_string(), it.next().unwrap_or("").to_string())
    };
    let inputs: Vec<(String, String)> = many("verbInput")?.iter().map(split_pair).collect();
    let outputs: Vec<(String, String)> = many("verbOutput")?.iter().map(split_pair).collect();
    let edges: Vec<(String, String)> = many("verbEdge")?.iter().map(split_pair).collect();
    Ok(verb_skeleton_rust(&VerbTable {
        verb: verb_local.to_string(),
        family,
        invocability,
        inputs,
        outputs,
        edges,
        repo_target,
    }))
}

/// #3567 SPIKE (govern-Fuseki #3564) — PURE verify-side scope control. A write to
/// `target_graph` is allowed ONLY if the token's `scope` claim names that graph.
/// This is the NEW enforcement that makes per-product scope a REAL boundary (403 on
/// miss), not decoration: today verify() does sig + aud + exp but carries no scope.
/// Empty scope = deny-all (fail-closed). Pure + unit-pinned — the Claims.scope read
/// wires into verify() separately (mirrors project_secured / field_exposed).
pub fn scope_allows(target_graph: &str, scope: &[String]) -> bool {
    scope.iter().any(|g| g == target_graph)
}

/// #4096 — scope by ROW, not by graph, for classes that carry an owner (Silas,
/// 2026-09-03 17:11: "the graph owner governs the class and shape; the row's
/// ownedBy governs who writes the row"). A Commitment owned by silas lives in the
/// services graph, which wren owns; silas writes it. Create injects the caller
/// as owner; replace and delete already refuse a non-owner (authz_allows). A class
/// with no owner field stays graph-governed: the token's scope must name the graph.
pub fn row_owner_governed(fields: &[String]) -> bool {
    fields.iter().any(|f| f.split('|').next() == Some("ownedBy"))
}

/// Authentication alone is not write authority. A model Principal may exist
/// without a holdsRole edge (guest/service identity); such a principal gets an
/// empty resolved agent id and must never create an entity with ownedBy="".
pub fn resolved_write_role(role: &str) -> bool {
    !role.trim().is_empty()
}

/// #3567 SPIKE — the `generate-dal` emitter: project a STANDALONE, committable,
/// per-product TypeScript write-edge for a class FROM ITS SHAPE — the read-side
/// projection (generate()→RouteTable) is reused verbatim; only the emitter is new
/// (the same move as #3551's verb target, Rust→TS instead of Rust→Rust = one model,
/// two projections, no competing implementation). The emitted lib runs with ZERO
/// athena-make callback (membrane #6): it MINTS a scoped service token (claims =
/// agentId, webId, aud, exp, + NEW scope=containment-graph URIs + jti audit id),
/// SHACL-validates the completeness floor (mandatory), then writes via the DAL.
/// `scope` is the generate-time per-product parameter (the graphs the product owns).
/// Pure string-builder (no codegen dep), mirroring verb_skeleton_rust.
pub fn dal_skeleton_ts(t: &RouteTable, scope: &[String]) -> String {
    // RouteTable.class is the full IRI (e.g. https://…/chorus#Test) — project the
    // LOCAL name for the TS symbol + route, the same shape serve() uses.
    let class = t.class.rsplit(['#', '/']).next().unwrap_or(&t.class);
    let plural = format!("{}s", class.to_lowercase()); // PoC route stub (serve uses the projected plural)
    let scope_lits = scope
        .iter()
        .map(|g| format!("  {:?}", g))
        .collect::<Vec<_>>()
        .join(",\n");
    let required = t
        .mandatory
        .iter()
        .map(|m| format!("  {:?}", m.split('|').next().unwrap_or(m)))
        .collect::<Vec<_>>()
        .join(",\n");
    let mut s = String::new();
    s.push_str(&format!(
        "// GENERATED by athena-make `generate-dal` (#3567) — DO NOT EDIT.\n\
         // Standalone per-product write-edge for {class}, projected from its shape.\n\
         // Mints an ES256 CSS IDENTITY token, SHACL-validates the floor, writes via the DAL.\n\
         // No athena-make callback (membrane #6). Scope is chorus:hasScope model data (#3722).\n\
         import {{ execFileSync }} from \"node:child_process\";\n\n"
    ));
    s.push_str(&format!(
        "// scope = graph URIs this product's domains own — PROJECTED from containment (#3564).\n\
         export const SCOPE: string[] = [\n{scope_lits}\n];\n\n"
    ));
    s.push_str(&format!(
        "// completeness floor (sh:minCount ≥ 1) — PROJECTED from the shape.\n\
         const REQUIRED: string[] = [\n{required}\n];\n\n"
    ));
    s.push_str(
        "// #3722 — mint an ES256 CSS IDENTITY token via chorus-identity-token. No\n\
         // secret, no self-declared scope: what this token may write is the\n\
         // Principal's chorus:hasScope grants, resolved at the athena-make door. SCOPE\n\
         // (above) stays as a projected DECLARATION of what this product SHOULD be\n\
         // granted — kept for the operator to author as model data, not signed in.\n\
         export function mintServiceToken(role: string): string {\n  \
         const bin = process.env.CHORUS_IDENTITY_TOKEN_BIN\n    \
         || `${process.env.HOME}/CascadeProjects/chorus/platform/scripts/chorus-identity-token`;\n  \
         const tok = execFileSync(bin, [role], { timeout: 8000 }).toString(\"utf-8\").trim();\n  \
         if (tok.split(\".\").length !== 3) throw new Error(\"identity-token: no verifiable token\");\n  \
         return tok;\n}\n\n",
    );
    s.push_str(
        "export function validate(props: Record<string, unknown>): void {\n  \
         const missing = REQUIRED.filter((k) => props[k] === undefined || props[k] === null || props[k] === \"\");\n  \
         if (missing.length) throw new Error(`shape-violation: missing required ${missing.join(\", \")}`);\n}\n\n",
    );
    s.push_str(&format!(
        "export async function write{class}(\n  \
         props: Record<string, unknown>,\n  role: string,\n  \
         targetGraph: string,\n  owlApiBase = \"http://localhost:3360\",\n): Promise<unknown> {{\n  \
         validate(props);\n  \
         const token = mintServiceToken(role);\n  \
         const res = await fetch(`${{owlApiBase}}/{plural}`, {{\n    \
         method: \"PUT\",\n    \
         headers: {{ Authorization: `Bearer ${{token}}`, \"Content-Type\": \"application/json\", \"X-Target-Graph\": targetGraph }},\n    \
         body: JSON.stringify(props),\n  }});\n  \
         if (!res.ok) throw new Error(`write failed: ${{res.status}} ${{await res.text()}}`);\n  \
         return res.json();\n}}\n"
    ));
    s
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

/// #3570 — a kind's instance HOME graph, derived (the domains.* data/noun spine).
/// `declared` (`chorus:instancesGraph`) is the explicit override / migration target;
/// otherwise project `urn:chorus:domains:<domain>` from the kind's domain.
/// #3640 (ADR-051): a class that is NEITHER shape-declared NOR domain-declared is
/// REFUSED — the silent `urn:chorus:instances` fallback is deleted. The fallback let
/// the generator serve classes the model never placed, which is how Product instances
/// scattered across graphs unnoticed (the July products incident). Refusal makes the
/// missing declaration the loud, fixable event: land the model, then the API exists.
pub fn resolve_instances_graph(declared: Option<&str>, domain: Option<&str>) -> R<String> {
    if let Some(g) = declared.map(str::trim).filter(|s| !s.is_empty()) {
        return Ok(g.to_string());
    }
    if let Some(d) = domain.map(str::trim).filter(|s| !s.is_empty()) {
        return Ok(format!("urn:chorus:domains:{}", d));
    }
    Err("no instance home: no domain definesVocabulary this class and its shape declares no chorus:instancesGraph — land the model first (ADR-051; was the silent urn:chorus:instances fallback)".to_string())
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
    // (chorus-model::read_shape) enforces: sh:minCount >= 1. This is athena-make's
    // READ-ONLY completeness GAUGE (the migration thermometer) — it MEASURES how
    // far an instance sits below the floor; the floor itself is ENFORCED at write
    // by the DAL, not here (athena-make is read-only; writes delegate to the DAL).
    // Edge properties (sh:class: ownedBy/atStep/membership) are excluded from the
    // human completeness gauge, but retained in `write_required`: create bodies
    // must advertise the same floor the DAL enforces, including required edges.
    let mq = format!(
        "PREFIX sh: <http://www.w3.org/ns/shacl#> SELECT ?v WHERE {{ GRAPH <{g}> {{ ?s sh:targetClass <{c}> ; sh:property ?p . ?p sh:path ?path ; sh:minCount ?mc . FILTER(?mc >= 1) FILTER(isIRI(?path)) OPTIONAL {{ ?p sh:class ?cl }} BIND(CONCAT(REPLACE(STR(?path), '.*#', ''), '|', IF(BOUND(?cl), 'edge', 'field')) AS ?v) }} }} ORDER BY ?v",
        g = ONTOLOGY_GRAPH, c = class
    );
    let required_rows = select_v(&sparql_json(&mq)?);
    let mut write_required: Vec<String> = required_rows
        .iter()
        .filter_map(|row| row.split_once('|').map(|(name, _)| name.to_string()))
        .collect();
    write_required.sort();
    write_required.dedup();
    let mut mandatory: Vec<String> = required_rows
        .iter()
        .filter_map(|row| row.strip_suffix("|field").map(str::to_string))
        .collect();
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
    // #3570 — resolve the instance HOME graph (the domains.* spine). chorus:instancesGraph
    // is the explicit override / migration target; else urn:chorus:domains:<domain> derived
    // from the domain that definesVocabulary this class; else the back-compat default. A
    // kind serves from its real home instead of the undifferentiated urn:chorus:instances bucket.
    let igq = format!(
        "PREFIX sh: <http://www.w3.org/ns/shacl#> PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <{g}> {{ ?s sh:targetClass <{c}> ; chorus:instancesGraph ?v }} }}",
        ns = NS, g = ONTOLOGY_GRAPH, c = class
    );
    let declared_ig = select_v(&sparql_json(&igq)?).into_iter().next();
    let dq = format!(
        "PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <{g}> {{ ?d chorus:definesVocabulary <{c}> BIND(REPLACE(STR(?d), '.*[#/]', '') AS ?v) }} }} LIMIT 1",
        ns = NS, g = ONTOLOGY_GRAPH, c = class
    );
    let domain_of = select_v(&sparql_json(&dq)?).into_iter().next();
    let instances_graph = resolve_instances_graph(declared_ig.as_deref(), domain_of.as_deref())?;
    // #3660 — PROJECT the tree read from the shape's declared recursive edges:
    // chorus:treeEdge (multi-valued, the descent predicates) + chorus:treeOrder
    // (the sibling rank property). A shape with no treeEdge emits NOTHING —
    // zero impact on kinds that don't opt in.
    let teq = format!(
        "PREFIX sh: <http://www.w3.org/ns/shacl#> PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <{g}> {{ ?s sh:targetClass <{c}> ; chorus:treeEdge ?e BIND(REPLACE(STR(?e), '.*[#/]', '') AS ?v) }} }} ORDER BY ?v",
        ns = NS, g = ONTOLOGY_GRAPH, c = class
    );
    let mut tree_edges: Vec<String> = select_v(&sparql_json(&teq)?);
    tree_edges.sort();
    tree_edges.dedup();
    let toq = format!(
        "PREFIX sh: <http://www.w3.org/ns/shacl#> PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <{g}> {{ ?s sh:targetClass <{c}> ; chorus:treeOrder ?o BIND(REPLACE(STR(?o), '.*[#/]', '') AS ?v) }} }} LIMIT 1",
        ns = NS, g = ONTOLOGY_GRAPH, c = class
    );
    let tree_order = select_v(&sparql_json(&toq)?).into_iter().next();
    // #3706 — PROJECT the class's model-version from chorus:modelVersion. ABSENT →
    // "unclassified": no human has reviewed the class, and absence is never promoted
    // to canonical. (The born-v2 default that lived here stamped ~120 unreviewed
    // classes "v2" in every envelope — the rule survived in this code after Jeff
    // rejected it in the ontology, which is exactly the model/serving incoherence
    // #3706 exists to end.) Read off the CLASS itself, not the shape's targetClass.
    let mvq = format!(
        "PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <{g}> {{ <{c}> chorus:modelVersion ?v }} }} LIMIT 1",
        ns = NS, g = ONTOLOGY_GRAPH, c = class
    );
    let model_version = select_v(&sparql_json(&mvq)?).into_iter().next().unwrap_or_else(|| "unclassified".to_string());
    routes.extend(tree_routes(&plural, &tree_edges));
    Ok(RouteTable { class, fields, routes, secured, mandatory, write_required, repo_target, exposure, instances_graph, tree_edges, tree_order, model_version })
}

/// #3660 — route emission for the tree read: ONE route iff the shape declares
/// at least one recursive-descent edge. No declaration → no phantom route.
pub fn tree_routes(plural: &str, tree_edges: &[String]) -> Vec<String> {
    if tree_edges.is_empty() {
        return Vec::new();
    }
    vec![format!("GET /{}/:name/tree", plural)]
}

/// #3660 — pure recursive tree builder. `edges` are (parent, child) localname
/// pairs; `ranks` are (node, rank). `depth` bounds descent below the root
/// (0 = root only). A node that is its own ANCESTOR on the current path is a
/// cycle → named Err; node REUSE across branches is legal (composition can
/// revisit — Borg serves two steps).
pub fn build_tree(root: &str, edges: &[(String, String)], ranks: &[(String, i64)], depth: usize) -> Result<String, String> {
    let mut children: std::collections::HashMap<&str, Vec<&str>> = std::collections::HashMap::new();
    for (p, c) in edges {
        children.entry(p.as_str()).or_default().push(c.as_str());
    }
    let rank: std::collections::HashMap<&str, i64> = ranks.iter().map(|(n, r)| (n.as_str(), *r)).collect();
    fn rec(
        node: &str,
        children: &std::collections::HashMap<&str, Vec<&str>>,
        rank: &std::collections::HashMap<&str, i64>,
        depth: usize,
        path: &mut Vec<String>,
    ) -> Result<String, String> {
        if path.iter().any(|a| a == node) {
            return Err(format!("cycle: '{}' is its own ancestor via [{} → {}]", node, path.join(" → "), node));
        }
        path.push(node.to_string());
        let mut kids: Vec<String> = Vec::new();
        if depth > 0 {
            if let Some(cs) = children.get(node) {
                let mut cs = cs.clone();
                cs.sort();
                cs.dedup();
                // ranked first (ascending), unranked fall to the bottom — visible,
                // never dropped; name-sort breaks ties deterministically
                cs.sort_by_key(|c| (rank.get(c).is_none(), rank.get(c).copied().unwrap_or(i64::MAX), c.to_string()));
                for c in cs {
                    kids.push(rec(c, children, rank, depth - 1, path)?);
                }
            }
        }
        path.pop();
        Ok(format!("{{ \"name\": \"{}\", \"children\": [{}] }}", json_escape(node), kids.join(", ")))
    }
    rec(root, &children, &rank, depth, &mut Vec::new())
}

/// #3454 AC1 — the WRITE routes generated per edge, mirroring the read routes.
/// POST creates an entity or adds an edge, PUT replaces an entity, DELETE removes
/// an entity or edge. Generated from the same plural/edge vocabulary as the read
/// routes, so a new edge type yields its write routes automatically. Pure +
/// unit-pinned. (The live execution + authZ + shape-rejection are the handler
/// increment; this is the contract.)
pub fn write_routes(plural: &str) -> Vec<String> {
    let mut routes = vec![
        format!("POST /{}", plural),                  // create entity
        format!("PUT /{}/:name", plural),             // replace entity
        format!("DELETE /{}/:name", plural),          // delete entity
        format!("POST /{}/:name/partof", plural),     // add partOf edge
        format!("DELETE /{}/:name/partof", plural),   // remove partOf edge
        format!("POST /{}/:name/contains", plural),   // add contains edge
        format!("DELETE /{}/:name/contains", plural), // remove contains edge
        format!("POST /{}/:name/has-child", plural),  // add has-child edge
        format!("DELETE /{}/:name/has-child", plural),// remove has-child edge
    ];
    // Phase A exposes the bulk transport only for TestResult. The DAL primitive
    // is entity-generic, but widening the HTTP mutation surface for every class
    // is a separate contract change.
    if plural == "testresults" {
        routes.insert(1, format!("POST /{}/batch", plural));
    }
    routes
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

// #3688 / ADR-054 §3.3 — the webid→role STRING parser is retired. The role now
// arrives on `Claims.agent_id`, resolved by the shared verifier from
// `chorus:holdsRole` (chorus_oidc::oidc::PRINCIPAL_ROLE_QUERY). Role assignment
// is governed data; a WebID naming convention that can disagree with the model
// is not an authZ input.

#[derive(Debug, PartialEq, Eq)]
pub enum WriteOp {
    CreateEntity,
    CreateBatch,
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
        ("POST", 2) if parts[1] == "batch" && plural == "testresults" => {
            Some(WriteOp::CreateBatch)
        }
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
/// values are decoded with the same escape-aware scanner used by the SPARQL JSON
/// reader, then validated (names) or re-escaped at their output boundary.
pub fn json_field(body: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\"", key);
    let i = body.find(&needle)? + needle.len();
    let after_colon = &body[i..][body[i..].find(':')? + 1..];
    let q = after_colon.find('"')? + 1;
    let val = &after_colon[q..];
    let raw = scan_json_string(val)?;
    Some(json_unescape(raw))
}

/// Strict parser for the flat string-valued objects accepted by create and
/// create-batch. The API deliberately has no general JSON dependency, but the
/// write door still has to enforce JSON grammar before it projects a request
/// into trusted NDJSON. In particular, missing commas, duplicate keys, unknown
/// escapes, non-string values, and trailing content are refusals rather than
/// inputs that a substring extractor can silently normalize.
/// #4096 — a value is a JSON string OR a JSON array of strings (a multi-valued
/// shape property: a product's four hasDomain targets, its two chorus:diagram
/// sources). One string parses as a one-element list. An empty array, a nested
/// object/array, or a non-string element is a refusal: the door does not guess.
fn parse_create_object(body: &str) -> R<std::collections::BTreeMap<String, Vec<String>>> {
    fn skip_ws(bytes: &[u8], mut i: usize) -> usize {
        while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n') {
            i += 1;
        }
        i
    }

    fn string_at(input: &str, i: &mut usize) -> R<String> {
        let bytes = input.as_bytes();
        if bytes.get(*i) != Some(&b'"') {
            return Err("expected a JSON string".to_string());
        }
        *i += 1;
        let start = *i;
        while *i < bytes.len() {
            match bytes[*i] {
                b'"' => {
                    let raw = &input[start..*i];
                    *i += 1;
                    return Ok(json_unescape(raw));
                }
                b'\\' => {
                    *i += 1;
                    let Some(&escape) = bytes.get(*i) else {
                        return Err("unterminated JSON escape".to_string());
                    };
                    match escape {
                        b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => {
                            *i += 1;
                        }
                        b'u' => {
                            if *i + 4 >= bytes.len()
                                || !bytes[*i + 1..=*i + 4].iter().all(u8::is_ascii_hexdigit)
                            {
                                return Err("invalid JSON unicode escape".to_string());
                            }
                            *i += 5;
                        }
                        _ => return Err("invalid JSON escape".to_string()),
                    }
                    continue;
                }
                c if c < 0x20 => return Err("unescaped control character in JSON string".to_string()),
                _ => *i += 1,
            }
        }
        Err("unterminated JSON string".to_string())
    }

    let bytes = body.as_bytes();
    let mut i = skip_ws(bytes, 0);
    if bytes.get(i) != Some(&b'{') {
        return Err("create body must be a JSON object".to_string());
    }
    i += 1;
    let mut out = std::collections::BTreeMap::new();
    loop {
        i = skip_ws(bytes, i);
        if bytes.get(i) == Some(&b'}') {
            i = skip_ws(bytes, i + 1);
            return if i == bytes.len() {
                Ok(out)
            } else {
                Err("create JSON object has trailing content".to_string())
            };
        }
        let key = string_at(body, &mut i)?;
        i = skip_ws(bytes, i);
        if bytes.get(i) != Some(&b':') {
            return Err(format!("create property '{}' must be followed by ':'", key));
        }
        i = skip_ws(bytes, i + 1);
        let value: Vec<String> = if bytes.get(i) == Some(&b'[') {
            i = skip_ws(bytes, i + 1);
            let mut items = Vec::new();
            loop {
                if bytes.get(i) == Some(&b']') {
                    i += 1;
                    break;
                }
                let item = string_at(body, &mut i)
                    .map_err(|e| format!("create property '{}': every list element must be a JSON string ({})", key, e))?;
                items.push(item);
                i = skip_ws(bytes, i);
                match bytes.get(i) {
                    Some(b',') => {
                        i = skip_ws(bytes, i + 1);
                        if bytes.get(i) == Some(&b']') {
                            return Err(format!("create property '{}': list must not have a trailing comma", key));
                        }
                    }
                    Some(b']') => { i += 1; break; }
                    Some(_) => return Err(format!("create property '{}': list elements must be separated by ','", key)),
                    None => return Err(format!("create property '{}': list is not closed", key)),
                }
            }
            if items.is_empty() {
                return Err(format!("create property '{}': an empty list says nothing — omit the property instead", key));
            }
            items
        } else {
            vec![string_at(body, &mut i)
                .map_err(|e| format!("create property '{}' must be a JSON string or a list of strings ({})", key, e))?]
        };
        if out.insert(key.clone(), value).is_some() {
            return Err(format!("create property '{}' appears more than once", key));
        }
        i = skip_ws(bytes, i);
        match bytes.get(i) {
            Some(b',') => {
                i = skip_ws(bytes, i + 1);
                if bytes.get(i) == Some(&b'}') {
                    return Err("create JSON object must not have a trailing comma".to_string());
                }
            }
            Some(b'}') => {
                i = skip_ws(bytes, i + 1);
                return if i == bytes.len() {
                    Ok(out)
                } else {
                    Err("create JSON object has trailing content".to_string())
                };
            }
            Some(_) => return Err("create JSON properties must be separated by ','".to_string()),
            None => return Err("create JSON object is not closed".to_string()),
        }
    }
}

/// The edge target name from a write body: { "target": "<name>" }.
pub fn parse_body_target(body: &str) -> Option<String> {
    json_field(body, "target")
}

/// Split one JSON array whose elements must be objects, preserving each object
/// as an exact source slice. This is deliberately string/escape/depth-aware: a
/// property value containing `},{`, an escaped quote, or a nested object/array
/// can never fabricate an entity boundary. Full property validation remains in
/// the same create preparation used by the single-entity route.
pub fn split_json_array_objects(body: &str) -> R<Vec<&str>> {
    fn skip_ws(bytes: &[u8], mut i: usize) -> usize {
        while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n') {
            i += 1;
        }
        i
    }

    let bytes = body.as_bytes();
    let mut i = skip_ws(bytes, 0);
    if bytes.get(i) != Some(&b'[') {
        return Err("batch body must be a JSON array of objects".to_string());
    }
    i += 1;
    let mut out = Vec::new();
    loop {
        i = skip_ws(bytes, i);
        if i >= bytes.len() {
            return Err("batch JSON array is not closed".to_string());
        }
        if bytes[i] == b']' {
            i = skip_ws(bytes, i + 1);
            if i != bytes.len() {
                return Err("batch JSON array has trailing content".to_string());
            }
            return Ok(out);
        }
        if bytes[i] != b'{' {
            return Err(format!("batch item {} must be a JSON object", out.len() + 1));
        }

        let start = i;
        let mut closers: Vec<u8> = Vec::new();
        let mut in_string = false;
        let mut escaped = false;
        let mut end = None;
        while i < bytes.len() {
            let c = bytes[i];
            if in_string {
                if escaped {
                    escaped = false;
                } else if c == b'\\' {
                    escaped = true;
                } else if c == b'"' {
                    in_string = false;
                } else if c < 0x20 {
                    return Err(format!("batch item {} contains an unescaped control character", out.len() + 1));
                }
                i += 1;
                continue;
            }
            match c {
                b'"' => in_string = true,
                b'{' => closers.push(b'}'),
                b'[' => closers.push(b']'),
                b'}' | b']' => {
                    if closers.pop() != Some(c) {
                        return Err(format!("batch item {} has mismatched JSON delimiters", out.len() + 1));
                    }
                    if closers.is_empty() {
                        end = Some(i + 1);
                        i += 1;
                        break;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        if in_string || escaped {
            return Err(format!("batch item {} has an unterminated JSON string", out.len() + 1));
        }
        let Some(end) = end else {
            return Err(format!("batch item {} JSON object is not closed", out.len() + 1));
        };
        out.push(&body[start..end]);

        i = skip_ws(bytes, i);
        match bytes.get(i) {
            Some(b',') => {
                i = skip_ws(bytes, i + 1);
                if bytes.get(i) == Some(&b']') {
                    return Err("batch JSON array must not have a trailing comma".to_string());
                }
            }
            Some(b']') => {
                i = skip_ws(bytes, i + 1);
                if i != bytes.len() {
                    return Err("batch JSON array has trailing content".to_string());
                }
                return Ok(out);
            }
            Some(_) => return Err(format!("batch item {} must be followed by ',' or ']'", out.len())),
            None => return Err("batch JSON array is not closed".to_string()),
        }
    }
}


/// The shape's DATATYPE/plain fields present in the body, as (field, value) pairs.
/// Edge fields (edge:*) are skipped — edges are written through the edge endpoints,
/// not the entity body. Pure + unit-tested.
pub fn collect_entity_props(body: &str, fields: &[String]) -> Vec<(String, String)> {
    // #4096 — every value of a list-valued property, through the strict parser
    // (a malformed body yields no props, and replace then refuses as empty).
    let values = parse_create_object(body).unwrap_or_default();
    let mut out = Vec::new();
    for f in fields {
        let (name, kind) = f.split_once('|').unwrap_or((f.as_str(), "plain"));
        if kind.starts_with("edge:") {
            continue;
        }
        if let Some(vs) = values.get(name) {
            for v in vs {
                out.push((name.to_string(), v.clone()));
            }
        }
    }
    out
}

/// #3680 — the sibling of collect_entity_props for EDGE-typed shape props:
/// `prop|edge:Class` markers present in the body forward as DAL edges
/// (property, target_kind, target_name) instead of silently dropping — the
/// create-with-required-edge gap (TestResult.ofTest: 1226/1226 refused,
/// 2026-07-24). target_kind = kebab of the edge's sh:class (kind_of_class),
/// matching the DAL's mint contract.
pub fn collect_entity_edges(body: &str, fields: &[String]) -> Vec<(String, String, String)> {
    // #4096 — every target of a list-valued edge (a product's hasDomain list).
    let values = parse_create_object(body).unwrap_or_default();
    let mut out = Vec::new();
    for f in fields {
        if let Some((name, kind)) = f.split_once('|') {
            if let Some(class_local) = kind.strip_prefix("edge:") {
                if let Some(vs) = values.get(name) {
                    for v in vs {
                        if !v.is_empty() {
                            out.push((name.to_string(), kind_of_class(class_local), v.clone()));
                        }
                    }
                }
            }
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

// #3468 — athena-make's raw-SPARQL write builders (build_create_entity /
// build_replace_entity / build_edge_update) and sparql_update were RETIRED: every
// write now delegates to the DAL (chorus-model), the one governed write path.
// athena-make is read-only over Fuseki again, per its Cargo.toml contract.

/// #3468 — DELEGATE writes to the DAL (athena-model) — the ONE governed write
/// path. Shells to the DAL CLI (the same subprocess pattern athena-make uses for curl);
/// the DAL enforces the completeness floor (sh:minCount, fail-closed), mints the
/// IRI, validates sh:in enums + referential integrity, and stamps the audit/spine
/// witness. Verified ownership is projected as the field or typed Role edge the
/// class shape declares. #3722 — the caller's VERIFIED bearer is forwarded as
/// CHORUS_IDENTITY_TOKEN so the DAL's own identity gate (#3687) verifies the
/// SAME token this door verified: the token travels, each layer proves it.
/// (The retired DEPLOY_ROLE env this used to set was exactly what #3687
/// stopped trusting — from Jul 27 to this fix every doored DAL write
/// fail-closed with identity-token-required while the door reported its own
/// success paths green.) Returns the DAL's typed refusal text on failure.
/// #3774 — the DAL binary every write shells to. CHORUS_MODEL_BIN overrides
/// (hermetic tests wire a stub here); the default MUST be the live binary —
/// chorus-model was retired into a fail-loud stub by #3718.
pub fn dal_bin() -> String {
    std::env::var("CHORUS_MODEL_BIN").unwrap_or_else(|_| "athena-model".to_string())
}

fn dal_run(args: &[String], token: &str) -> R<()> {
    let bin = dal_bin();
    let out = Command::new(&bin)
        .args(args)
        .env("CHORUS_IDENTITY_TOKEN", token)
        .env_remove("DEPLOY_ROLE")
        .output()
        .map_err(|e| format!("dal-spawn: {}", e))?;
    dal_output(out)
}

/// Run one DAL command with a bounded document on stdin. Entity data never
/// appears in argv (and therefore not in process listings); only the verb does.
fn dal_run_stdin(args: &[String], token: &str, input: &str) -> R<()> {
    let bin = dal_bin();
    let mut child = Command::new(&bin)
        .args(args)
        .env("CHORUS_IDENTITY_TOKEN", token)
        .env_remove("DEPLOY_ROLE")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("dal-spawn: {}", e))?;
    let write_result = child
        .stdin
        .take()
        .ok_or_else(|| "dal-stdin: pipe unavailable".to_string())
        .and_then(|mut stdin| stdin.write_all(input.as_bytes()).map_err(|e| format!("dal-stdin: {}", e)));
    if let Err(e) = write_result {
        let _ = child.kill();
        let _ = child.wait();
        return Err(e);
    }
    let out = child.wait_with_output().map_err(|e| format!("dal-wait: {}", e))?;
    dal_output(out)
}

fn dal_output(out: std::process::Output) -> R<()> {
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

/// #3647 — the DAL kind for a class local name: ADR-040 kinds are KEBAB-CASE
/// (value-stream-step), but the old derivation just lowercased (valuestreamstep)
/// — chorus-model refused it as unknown-kind (found live in the #3613 drill).
/// CamelCase → kebab, single implementation, used by every dal_* call-site.
pub fn kind_of_class(class_local: &str) -> String {
    let mut out = String::with_capacity(class_local.len() + 4);
    for (i, c) in class_local.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Replace an entity via the DAL `add` (full governed upsert: floor + mint +
/// audit). Creates use the create-only `add-batch` transaction, including the
/// one-record case. The caller supplies already-prepared fields/edges, including
/// verified ownership. #3647: `graph` = the class's MODEL-DECLARED instance home
/// (RouteTable.instances_graph — the same value every read + the ownedBy authz
/// resolve), so create and authz agree on where the entity lives (the orphan fix).
fn dal_add(
    kind: &str, name: &str, token: &str,
    fields: &[(String, String)],
    edges: &[(String, String, String)],
    graph: &str,
) -> R<()> {
    let mut args: Vec<String> = vec![
        "add".into(), "--kind".into(), kind.to_string(), "--name".into(), name.to_string(),
        "--graph".into(), graph.to_string(),
    ];
    for (f, v) in fields {
        args.push("--field".into());
        args.push(format!("{}={}", f, v));
    }
    // #3680 — inline edges at create/replace: the DAL's req.edges path (referential
    // integrity + sh:class target-type) has existed all along; athena-make just never
    // filled it, making create-with-required-edge impossible via the route.
    for (p, tk, tn) in edges {
        args.push("--edge".into());
        args.push(format!("{}={}:{}", p, tk, tn));
    }
    dal_run(&args, token)
}

/// Create many same-class entities through one atomic DAL invocation. Each
/// newline is one WriteReq-like JSON object; the only argv token is `add-batch`.
fn dal_add_batch(input: &str, token: &str) -> R<()> {
    dal_run_stdin(&["add-batch".to_string()], token, input)
}

/// #4102 — replace several rows in one governed update (the row being written
/// and the Revision holding the version it displaces).
fn dal_write_many(input: &str, token: &str) -> R<()> {
    dal_run_stdin(&["write-many".to_string()], token, input)
}

/// Delete an entity via the DAL `delete` (governed, fail-closed, witnessed).
fn dal_delete(kind: &str, name: &str, token: &str, graph: &str) -> R<()> {
    dal_run(&["delete".into(), "--kind".into(), kind.to_string(), "--name".into(), name.to_string(),
              "--graph".into(), graph.to_string()], token)
}

/// Add/remove one edge via the DAL `link`/`unlink` (incremental + referential
/// integrity + witness). The structural edges (partOf/contains/hasChild) connect
/// bare-kind entities (Domain/Product), so the subject kind mints the target IRI
/// identically (mint is kind-independent for bare kinds).
fn dal_edge(insert: bool, kind: &str, name: &str, prop: &str, tname: &str, token: &str, graph: &str) -> R<()> {
    dal_edge_keeping(insert, kind, name, prop, tname, token, graph, None)
}

/// #4102 — an edge change is data like any other. When a Revision is passed it
/// rides the SAME governed update as the link/unlink, so a row cannot change
/// parent while its history says it never moved.
#[allow(clippy::too_many_arguments)]
fn dal_edge_keeping(insert: bool, kind: &str, name: &str, prop: &str, tname: &str, token: &str, graph: &str, revision: Option<&PreparedCreate>) -> R<()> {
    let verb = if insert { "link" } else { "unlink" };
    let mut args: Vec<String> = vec![verb.into(), "--kind".into(), kind.to_string(), "--name".into(), name.to_string(),
              "--graph".into(), graph.to_string(),
              "--edge".into(), format!("{}={}:{}", prop, kind, tname)];
    match revision {
        None => dal_run(&args, token),
        Some(rev) => {
            args.push("--revision-stdin".into());
            dal_run_stdin(&args, token, &prepared_create_ndjson(std::slice::from_ref(rev)))
        }
    }
}

/// #3573 — BATCH via the DAL `batch` op. graph is the scope-VALIDATED x-target-graph;
/// deletes/inserts are typed-slot triples passed as argv (never SPARQL text). The DAL
/// re-validates every slot + refuses empty/off-realm graph (defense-in-depth).
fn dal_batch(graph: &str, deletes: &[(String, String, String)], inserts: &[(String, String, String)], token: &str) -> R<()> {
    let mut args: Vec<String> = vec!["batch".into(), "--graph".into(), graph.to_string()];
    for (s, p, o) in deletes {
        args.push("--del".into()); args.push(s.clone()); args.push(p.clone()); args.push(o.clone());
    }
    for (s, p, o) in inserts {
        args.push("--ins".into()); args.push(s.clone()); args.push(p.clone()); args.push(o.clone());
    }
    dal_run(&args, token)
}

/// #3573 — governed BATCH route (POST /batch). Body is TAB-delimited lines:
///   DEL\t<s>\t<p>\t<o>   (o may be ?o wildcard)
///   INS\t<s>\t<p>\t<o>
/// Tab can't appear in a valid IRI/literal, so athena-make never assembles SPARQL — it
/// splits into typed argv and hands them to the chorus-model `batch` CLI (slot-
/// validation + structural single-graph). `graph` = the scope-validated x-target-graph.
fn handle_batch(graph: &str, body: &str, caller_role: &str, token: &str) -> (u16, String) {
    if graph.trim().is_empty() {
        return write_resp("validation", "batch requires x-target-graph (no default graph, ever)");
    }
    if body.len() > MAX_WRITE_BYTES {
        return write_resp("validation", &format!("batch body {} bytes exceeds {}-byte cap", body.len(), MAX_WRITE_BYTES));
    }
    let mut deletes: Vec<(String, String, String)> = Vec::new();
    let mut inserts: Vec<(String, String, String)> = Vec::new();
    for line in body.lines() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() { continue; }
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() != 4 {
            return write_resp("validation", "batch line must be OP<tab>S<tab>P<tab>O");
        }
        let triple = (f[1].to_string(), f[2].to_string(), f[3].to_string());
        match f[0] {
            "DEL" => deletes.push(triple),
            "INS" => inserts.push(triple),
            _ => return write_resp("validation", "batch op must be DEL or INS"),
        }
    }
    if deletes.is_empty() && inserts.is_empty() {
        return write_resp("validation", "batch: no DEL/INS lines");
    }
    match dal_batch(graph, &deletes, &inserts, token) {
        Ok(_) => {
            emit_write_spine(caller_role, "batch", graph, "", "ok");
            write_resp("ok", &format!("batch applied: {} del, {} ins -> <{}> (via DAL)", deletes.len(), inserts.len(), graph))
        }
        Err(e) => {
            emit_write_spine(caller_role, "batch", graph, "", "error");
            dal_err_resp(&e)
        }
    }
}

/// Map a DAL refusal string onto athena-make's typed write response.
fn dal_err_resp(e: &str) -> (u16, String) {
    if e.contains("conflict") || e.contains("already-exists") || e.contains("duplicate-identity") {
        write_resp("conflict", e)
    } else if e.contains("not-found") {
        write_resp("not-found", e)
    } else if e.contains("shape-violation")
        || e.contains("shape-channel-violation")
        || e.contains("unknown-endpoint")
        || e.contains("unknown-target")
        || e.contains("bad-property")
        || e.contains("double-prefix")
        || e.contains("empty-name")
        || e.starts_with("batch:")
    {
        // #3573 — a batch refusal (empty/off-realm graph, injection-shaped slot) is a
        // client-side validation reject, not a server error: return 4xx, never 502.
        write_resp("validation", e)
    } else {
        (502, format!("{{ \"error\": \"dal\", \"message\": \"{}\" }}", json_escape(e)))
    }
}

/// Query the ownedBy role of an entity (for authZ). None = no ownedBy on record →
/// authz_allows fails closed.
/// #4096 — the entity's IRI as the DAL mints it. Product, Domain and Test are
/// BARE grains (chorus:spine); every other kind is PREFIXED with its kebab kind
/// (chorus:document-x, chorus:value-stream-step-directing) — the DAL's KINDS table
/// (athena-model lib.rs) is the source of that flag. Until today every read,
/// existence check and owner lookup here used the bare form for every kind, so a
/// document written through the door could not be read back, and its own creator
/// was refused as "not the owner" on replace (round 14).
pub fn entity_subject(class: &str, name: &str) -> String {
    let local = class.rsplit('#').next().unwrap_or(class);
    match local {
        "Product" | "Domain" | "Test" => format!("{}{}", NS, name),
        _ => format!("{}{}-{}", NS, kind_of_class(local), name),
    }
}

fn query_owned_by(class: &str, entity: &str, instances_graph: &str) -> Option<String> {
    let q = format!(
        "PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <{g}> {{ <{s}> chorus:ownedBy ?o . BIND(REPLACE(STR(?o), '.*[#/]', '') AS ?v) }} }}",
        ns = NS, g = instances_graph, s = entity_subject(class, entity)
    );
    sparql_json(&q)
        .ok()
        .and_then(|b| select_v(&b).into_iter().next())
        .map(|owner| normalize_owned_role(&owner))
}

/// Model Role instances are minted as `role-<name>`, while verified claims
/// intentionally expose the local role name (`wren`). Legacy literal owners
/// already contain the local name. Normalize both storage representations onto
/// the verifier's comparison form so a newly created edge-owned entity remains
/// writable by its owner.
fn normalize_owned_role(owner: &str) -> String {
    owner.strip_prefix("role-").unwrap_or(owner).to_string()
}

/// Does the entity already have a partOf parent? (single-parent → 2nd add is 409).
fn partof_exists(entity: &str, instances_graph: &str) -> bool {
    let q = format!(
        "PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <{g}> {{ <{ns}{e}> chorus:partOf ?p . BIND('y' AS ?v) }} }}",
        ns = NS, g = instances_graph, e = entity
    );
    sparql_json(&q).map(|b| !select_v(&b).is_empty()).unwrap_or(false)
}

/// Does the entity exist at all? (create → 409 if it does; replace → 404 if it doesn't).
fn entity_exists(class: &str, entity: &str, instances_graph: &str) -> bool {
    let q = format!(
        "SELECT ?v WHERE {{ GRAPH <{g}> {{ <{s}> ?p ?o . BIND('y' AS ?v) }} }} LIMIT 1",
        g = instances_graph, s = entity_subject(class, entity)
    );
    sparql_json(&q).map(|b| !select_v(&b).is_empty()).unwrap_or(false)
}

/// The one prepared representation shared by single and batch create. At this
/// point the name is safe, closed-shape validation has passed, fields/edges have
/// been projected from the shape, the verified owner has been injected, and
/// kind + graph are fixed from the selected class table. Existing-identity
/// conflicts remain inside the governed create-only DAL transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PreparedCreate {
    kind: String,
    name: String,
    fields: Vec<(String, String)>,
    edges: Vec<(String, String, String)>,
    graph: String,
}

#[derive(Debug, PartialEq, Eq)]
struct CreatePrepareError {
    tag: &'static str,
    spine_result: &'static str,
    entity: String,
    message: String,
}

/// Project the verified caller into the representation declared by the shape.
/// Most older write shapes did not declare `ownedBy`, and the established DAL
/// contract stores the resolved role as a literal for those classes. When the
/// model declares `ownedBy` as an object property, however, sending that value
/// as a field bypasses the modeled edge type. Emit the corresponding typed Role
/// edge instead. Request bodies never participate in this choice.
fn verified_owner_projection(
    table: &RouteTable,
    caller_role: &str,
) -> (Vec<(String, String)>, Vec<(String, String, String)>) {
    let owner_annotation = table.fields.iter().find_map(|field| {
        let (property, annotation) = field
            .split_once('|')
            .unwrap_or((field.as_str(), "plain"));
        (property == "ownedBy").then_some(annotation)
    });
    match owner_annotation.and_then(|annotation| annotation.strip_prefix("edge:")) {
        Some(class_local) => (
            Vec::new(),
            vec![(
                "ownedBy".to_string(),
                kind_of_class(class_local),
                caller_role.to_string(),
            )],
        ),
        None => (
            vec![("ownedBy".to_string(), caller_role.to_string())],
            Vec::new(),
        ),
    }
}

/// Purely prepare one create using exactly the checks/projections both routes
/// need. This layer performs no existence reads: single and batch create both
/// delegate identity conflict detection to the DAL's atomic `add-batch` path.
fn prepare_create(body: &str, table: &RouteTable, caller_role: &str, landed_commit: &str) -> Result<PreparedCreate, CreatePrepareError> {
    let values = parse_create_object(body).map_err(|message| CreatePrepareError {
        tag: "validation",
        spine_result: "validation",
        entity: String::new(),
        message,
    })?;
    let name = match values.get("name").map(Vec::as_slice) {
        Some([one]) => one.clone(),
        Some(_) => {
            return Err(CreatePrepareError {
                tag: "validation",
                spine_result: "validation",
                entity: String::new(),
                message: "create 'name' must be one string, not a list".to_string(),
            })
        }
        None => {
            return Err(CreatePrepareError {
                tag: "validation",
                spine_result: "validation",
                entity: String::new(),
                message: "create requires a 'name' in the body".to_string(),
            })
        }
    };
    if !is_safe_local(&name) {
        return Err(CreatePrepareError {
            tag: "validation",
            spine_result: "validation",
            entity: name,
            message: "invalid entity name".to_string(),
        });
    }
    let declared: std::collections::HashSet<&str> = table
        .fields
        .iter()
        .map(|field| field.split('|').next().unwrap_or(field))
        .collect();
    if let Some(bad) = values.keys().find(|key| key.as_str() != "name" && !declared.contains(key.as_str())) {
        return Err(CreatePrepareError {
            tag: "validation",
            spine_result: "off-model",
            entity: name,
            message: format!("off-model property '{}' is not in the shape", bad),
        });
    }

    // The caller cannot self-select ownership. Remove any body-projected copy
    // and inject the role resolved from the verified token exactly once, in the
    // field/edge representation declared by this class's shape.
    let (mut fields, mut edges) = verified_owner_projection(table, caller_role);
    for field in &table.fields {
        let (property, annotation) = field.split_once('|').unwrap_or((field.as_str(), "plain"));
        let Some(vals) = values.get(property) else { continue };
        if property == "ownedBy" {
            continue;
        }
        for value in vals {
        if let Some(class_local) = annotation.strip_prefix("edge:") {
            if !value.is_empty() {
                if !is_safe_local(value) {
                    return Err(CreatePrepareError {
                        tag: "validation",
                        spine_result: "validation",
                        entity: name,
                        message: format!(
                            "invalid local name '{}' for edge '{}'",
                            value, property
                        ),
                    });
                }
                edges.push((property.to_string(), kind_of_class(class_local), value.clone()));
            }
        } else {
            fields.push((property.to_string(), value.clone()));
        }
        }
    }
    // #4101 — stamps from the write; a document with no declared word is a draft
    fields.retain(|(f, _)| f != "changedAt" && f != "changedIn" && f != "version");
    fields.extend(write_stamps(table, landed_commit));
    fields.extend(version_stamp(table, None));
    if table.fields.iter().any(|f| f.split('|').next() == Some("docState")) && !fields.iter().any(|(f, _)| f == "docState") {
        fields.push(("docState".to_string(), "draft".to_string()));
    }
    let class_local = table.class.rsplit('#').next().unwrap_or("");
    Ok(PreparedCreate {
        kind: kind_of_class(class_local),
        name,
        fields,
        edges,
        graph: table.instances_graph.clone(),
    })
}

/// Serialize one prepared create as the DAL's WriteReq-like NDJSON record. All
/// keys and values cross the existing JSON escaping boundary; no request data
/// is ever interpolated into process argv.
fn prepared_create_json(req: &PreparedCreate) -> String {
    // #4096 — a JSON object cannot repeat a key, so the FIRST value of each
    // property goes in `fields` and every further value rides in `more_values`
    // (the DAL's slot for multi-valued literals; it writes them as further
    // triples on the same predicate).
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut firsts: Vec<String> = Vec::new();
    let mut more: Vec<String> = Vec::new();
    for (key, value) in &req.fields {
        if seen.insert(key.as_str()) {
            firsts.push(format!("\"{}\":\"{}\"", json_escape(key), json_escape(value)));
        } else {
            more.push(format!("[\"{}\",\"{}\"]", json_escape(key), json_escape(value)));
        }
    }
    let fields = firsts.join(",");
    let more_values = more.join(",");
    let edges = req
        .edges
        .iter()
        .map(|(property, target_kind, target_name)| {
            format!(
                "[\"{}\",\"{}\",\"{}\"]",
                json_escape(property),
                json_escape(target_kind),
                json_escape(target_name),
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"kind\":\"{}\",\"name\":\"{}\",\"fields\":{{{}}},\"more_values\":[{}],\"edges\":[{}],\"graph\":\"{}\"}}",
        json_escape(&req.kind),
        json_escape(&req.name),
        fields,
        more_values,
        edges,
        json_escape(&req.graph),
    )
}

fn prepared_create_ndjson(reqs: &[PreparedCreate]) -> String {
    let mut out = reqs.iter().map(prepared_create_json).collect::<Vec<_>>().join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

fn handle_create(body: &str, table: &RouteTable, caller_role: &str, token: &str, landed_commit: &str) -> (u16, String) {
    let req = match prepare_create(body, table, caller_role, landed_commit) {
        Ok(req) => req,
        Err(e) => {
            emit_write_spine(caller_role, "create", &e.entity, "", e.spine_result);
            return write_resp(e.tag, &e.message);
        }
    };
    // Create must use the DAL's create-only transaction even for one record.
    // The older `add` verb is a governed full upsert for replace; using it here
    // left a check-then-upsert race and missed prefixed identities because the
    // API's legacy existence read did not own the ADR-040 mint.
    let input = prepared_create_ndjson(std::slice::from_ref(&req));
    match dal_add_batch(&input, token) {
        Ok(_) => {
            emit_write_spine(caller_role, "create", &req.name, "", "ok");
            write_resp("created", &format!("created {} via DAL (ownedBy {})", req.name, caller_role))
        }
        Err(e) => {
            let outcome = if e.contains("already-exists") || e.contains("duplicate-identity") {
                "conflict"
            } else if e.contains("shape-violation") {
                "incomplete"
            } else {
                "error"
            };
            emit_write_spine(caller_role, "create", &req.name, "", outcome);
            dal_err_resp(&e)
        }
    }
}

fn handle_create_batch(body: &str, table: &RouteTable, caller_role: &str, token: &str, landed_commit: &str) -> (u16, String) {
    let objects = match split_json_array_objects(body) {
        Ok(objects) => objects,
        Err(e) => return write_resp("validation", &e),
    };
    if objects.is_empty() {
        return write_resp("validation", "batch create requires at least one entity");
    }

    let mut reqs = Vec::with_capacity(objects.len());
    for (index, object) in objects.into_iter().enumerate() {
        let req = match prepare_create(object, table, caller_role, landed_commit) {
            Ok(req) => req,
            Err(e) => {
                emit_write_spine(caller_role, "create-batch", &e.entity, "", e.spine_result);
                return write_resp(e.tag, &format!("batch item {}: {}", index + 1, e.message));
            }
        };
        reqs.push(req);
    }

    // In-request duplicates are knowable here without a store read. Existing
    // identity checks belong to athena-model, which owns the ADR-040 mint (bare
    // versus type-prefixed subjects) and can therefore reject them correctly and
    // fail closed in the same governed add-batch operation.
    let mut seen = std::collections::HashSet::new();
    for (index, req) in reqs.iter().enumerate() {
        if !seen.insert(req.name.clone()) {
            emit_write_spine(caller_role, "create-batch", &req.name, "", "conflict");
            return write_resp(
                "conflict",
                &format!("batch item {}: duplicate entity name '{}' in request", index + 1, req.name),
            );
        }
    }

    let input = prepared_create_ndjson(&reqs);
    match dal_add_batch(&input, token) {
        Ok(_) => {
            let plural = pluralize(table.class.rsplit('#').next().unwrap_or("entity"));
            emit_write_spine(caller_role, "create-batch", &plural, "", "ok");
            write_resp(
                "created",
                &format!("created {} {} via one DAL batch (ownedBy {})", reqs.len(), plural, caller_role),
            )
        }
        Err(e) => {
            emit_write_spine(caller_role, "create-batch", &reqs.len().to_string(), "", "error");
            dal_err_resp(&e)
        }
    }
}

// (created/modified stamping now lives in the DAL's audit envelope — athena-make's
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
/// rejection → governed DAL execution → spine event, every outcome typed via
/// write_status. authN is done by serve() before this is called (caller_role =
/// the verified token's role).
/// #3573 — max bytes for a STRUCTURED write body, including a whole JSON batch.
/// 64 KiB bounds runaway/oversized writes before any graph work.
pub const MAX_WRITE_BYTES: usize = 65_536;

/// #3573 — max bytes for a GSP BULK load (a TTL blob, not one entity). Larger by
/// design; still bounded. Tune to the real harvest/migration sizes.
pub const MAX_BULK_BYTES: usize = 8 * 1024 * 1024; // 8 MiB

/// #3573 AC5 (closed-shape) — the first body property whose predicate the shape does
/// NOT declare (truly off-model), else None. `fields` = RouteTable.fields ("name|kind").
/// The write-envelope keys (name, target) are not shape props and pass; declared edges
/// are in the model and pass. SHACL is open by default; this is the sh:closed reject.
pub fn off_model_property(body: &str, fields: &[String]) -> Option<String> {
    let declared: std::collections::HashSet<&str> =
        fields.iter().map(|f| f.split('|').next().unwrap_or(f)).collect();
    for key in json_top_level_keys(body) {
        if key == "name" || key == "target" { continue; } // write-envelope, not shape props
        if !declared.contains(key.as_str()) {
            return Some(key);
        }
    }
    None
}

/// Enumerate the TOP-LEVEL object keys of a (flat) JSON body, zero-dep. Tracks
/// string-state + brace/bracket depth so only depth-1 keys (property names) are
/// returned — nested values, escaped quotes, and array elements don't leak in.
pub fn json_top_level_keys(json: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut depth: i32 = 0;
    let mut in_str = false;
    let mut esc = false;
    let mut cur = String::new();
    let mut last_str_at_1: Option<String> = None;
    for c in json.chars() {
        if in_str {
            if esc { esc = false; cur.push(c); }
            else if c == '\\' { esc = true; }
            else if c == '"' {
                in_str = false;
                if depth == 1 { last_str_at_1 = Some(std::mem::take(&mut cur)); } else { cur.clear(); }
            } else { cur.push(c); }
        } else {
            match c {
                '"' => { in_str = true; cur.clear(); }
                '{' | '[' => { depth += 1; if depth != 1 { last_str_at_1 = None; } }
                '}' | ']' => { depth -= 1; last_str_at_1 = None; }
                ':' if depth == 1 => { if let Some(k) = last_str_at_1.take() { keys.push(k); } }
                ',' => last_str_at_1 = None,
                _ => {}
            }
        }
    }
    keys
}

#[cfg(test)]
mod bounds_closedshape_tests {
    use super::*;

    fn test_fields() -> Vec<String> {
        vec![
            "filePath|datatype:string".into(),
            "testName|datatype:string".into(),
            "quarantined|datatype:boolean".into(),
            "covers|edge:Domain".into(),
        ]
    }

    #[test]
    fn off_model_property_rejects_unknown_predicate() {
        let body = r#"{"name":"t1","filePath":"a.rs","evil":"haha"}"#;
        assert_eq!(off_model_property(body, &test_fields()), Some("evil".to_string()));
    }

    #[test]
    fn off_model_property_allows_declared_and_envelope_keys() {
        let body = r#"{"name":"t1","filePath":"a.rs","quarantined":true,"covers":"tests"}"#;
        assert_eq!(off_model_property(body, &test_fields()), None);
    }

    #[test]
    fn json_top_level_keys_ignores_nested_and_escapes() {
        let body = r#"{"name":"t1","note":"a \"quote\" : here","meta":{"inner":"x"}}"#;
        let keys = json_top_level_keys(body);
        assert!(keys.contains(&"name".to_string()));
        assert!(keys.contains(&"note".to_string()));
        assert!(keys.contains(&"meta".to_string()));
        assert!(!keys.contains(&"inner".to_string())); // nested key not surfaced
        assert!(!keys.contains(&"here".to_string()));   // value-with-colon not a key
    }

    #[test]
    fn bounds_predicate_separates_oversized_from_ok() {
        assert!("x".repeat(MAX_WRITE_BYTES + 1).len() > MAX_WRITE_BYTES);
        assert!("x".repeat(1024).len() <= MAX_WRITE_BYTES);
    }

    #[test]
    fn json_array_object_split_is_string_escape_and_depth_aware() {
        let body = r#" [ {"name":"a","note":"keeps },{ and \"quotes\"","nested":{"xs":[1,{"k":"v"}]}}, {"name":"b"} ] "#;
        let got = split_json_array_objects(body).unwrap();
        assert_eq!(got.len(), 2);
        assert!(got[0].contains(r#""note":"keeps },{ and \"quotes\"""#));
        assert!(got[0].contains(r#""nested":{"xs":[1,{"k":"v"}]}"#));
        assert_eq!(got[1], r#"{"name":"b"}"#);
    }

    #[test]
    fn json_array_object_split_rejects_wrong_framing_and_trailing_comma() {
        assert!(split_json_array_objects("{}").unwrap_err().contains("JSON array"));
        assert!(split_json_array_objects("[1]").unwrap_err().contains("item 1"));
        assert!(split_json_array_objects(r#"[{"name":"a"},]"#).unwrap_err().contains("trailing comma"));
        assert!(split_json_array_objects(r#"[{"name":"a}]"#).unwrap_err().contains("unterminated"));
    }

    #[test]
    fn create_object_parser_is_strict_and_decodes_strings() {
        let parsed = parse_create_object(r#" { "name": "tr-1", "testName": "a \"quote\"", "note": "\u2014" } "#)
            .expect("valid flat create object");
        assert_eq!(parsed.get("name").map(|v| v[0].as_str()), Some("tr-1"));
        assert_eq!(parsed.get("testName").map(|v| v[0].as_str()), Some("a \"quote\""));
        assert_eq!(parsed.get("note").map(|v| v[0].as_str()), Some("—"));

        for bad in [
            r#"{"name":"x" "filePath":"a"}"#,
            r#"{"name":"x",}"#,
            r#"{"name":"x","name":"y"}"#,
            r#"{"name":42}"#,
            r#"{"name":"bad\q"}"#,
            r#"{"name":"x"} trailing"#,
            // #4096 lists: empty, non-string element, nested, trailing comma, unclosed
            r#"{"name":"x","hasDomain":[]}"#,
            r#"{"name":"x","hasDomain":[1]}"#,
            r#"{"name":"x","hasDomain":[["a"]]}"#,
            r#"{"name":"x","hasDomain":["a",]}"#,
            r#"{"name":"x","hasDomain":["a""#,
        ] {
            assert!(parse_create_object(bad).is_err(), "malformed JSON must be refused: {bad}");
        }
    }

    /// #4096 — a list value parses to every element, in order; a plain string is a
    /// one-element list, so the two forms are one shape to the rest of the door.
    /// #4101 — the door stamps changedAt/changedIn on a class whose shape carries
    /// them, from the write itself; a document with no declared word is a draft.
    /// NEGATIVE PROOF: a body that tries to set a stamp is named and refused.
    #[test]
    fn the_door_stamps_the_write_and_refuses_a_body_stamp() {
        let table = RouteTable {
            class: "https://jeffbridwell.com/chorus#Document".into(),
            fields: vec!["docTitle".into(), "docHref".into(), "changedAt".into(), "changedIn".into(), "docState".into(), "ownedBy|edge:Role".into()],
            routes: vec![], secured: vec![], mandatory: vec![], write_required: vec![], repo_target: String::new(), exposure: vec![],
            instances_graph: "urn:chorus:domains:documents".into(), tree_edges: vec![], tree_order: None, model_version: "unclassified".into(),
        };
        let req = prepare_create(r#"{"name":"d1","docTitle":"D","docHref":"/d.html"}"#, &table, "wren", "abc1234").unwrap();
        let get = |k: &str| req.fields.iter().find(|(f, _)| f == k).map(|(_, v)| v.clone());
        assert_eq!(get("changedIn").as_deref(), Some("abc1234"));
        assert!(get("changedAt").map_or(false, |v| v.ends_with('Z') && v.len() == 20), "{:?}", get("changedAt"));
        assert_eq!(get("docState").as_deref(), Some("draft"));
        let kept = prepare_create(r#"{"name":"d2","docTitle":"D","docHref":"/d.html","docState":"current"}"#, &table, "wren", "").unwrap();
        assert_eq!(kept.fields.iter().find(|(f, _)| f == "docState").map(|(_, v)| v.as_str()), Some("current"));
        assert_eq!(kept.fields.iter().find(|(f, _)| f == "changedIn").map(|(_, v)| v.as_str()), Some("unknown"));
        assert_eq!(body_sets_a_stamp(r#"{"name":"d3","changedIn":"deadbeef"}"#).as_deref(), Some("changedIn"));
        assert_eq!(body_sets_a_stamp(r#"{"name":"d3","docTitle":"x"}"#), None);
        // version: 1 on create, previous + 1 on replace, only where the shape carries it
        let mut vt = table.clone(); vt.fields.push("version".into());
        let v1 = prepare_create(r#"{"name":"d4","docTitle":"D","docHref":"/d.html"}"#, &vt, "wren", "").unwrap();
        assert_eq!(v1.fields.iter().find(|(f, _)| f == "version").map(|(_, v)| v.as_str()), Some("1"));
        assert_eq!(version_stamp(&vt, Some("7")), Some(("version".to_string(), "8".to_string())));
        assert_eq!(version_stamp(&vt, Some("junk")), Some(("version".to_string(), "1".to_string())));
        assert_eq!(body_sets_a_stamp(r#"{"name":"d5","version":"9"}"#).as_deref(), Some("version"));
        // a class without the stamp fields gets none
        let plain = RouteTable {
            class: "https://jeffbridwell.com/chorus#Card".into(), fields: vec!["label".into()],
            routes: vec![], secured: vec![], mandatory: vec![], write_required: vec![], repo_target: String::new(), exposure: vec![],
            instances_graph: "urn:chorus:instances".into(), tree_edges: vec![], tree_order: None, model_version: "unclassified".into(),
        };
        assert!(write_stamps(&plain, "abc").is_empty());
    }

    #[test]
    fn create_object_parser_takes_lists_of_strings() {
        let parsed = parse_create_object(r#"{"name":"athena","hasDomain":["products","services","domains"],"diagram":["%% a\\nflowchart TD","%% b\\nflowchart LR"],"label":"Athena"}"#)
            .expect("lists of strings are valid");
        assert_eq!(parsed["hasDomain"], vec!["products", "services", "domains"]);
        assert_eq!(parsed["diagram"].len(), 2);
        assert_eq!(parsed["label"], vec!["Athena"]);
    }

    /// #4096 — the NDJSON handed to the DAL carries the first value per key in
    /// `fields` and the rest in `more_values`; a JSON object cannot repeat a key.
    #[test]
    fn prepared_create_ndjson_puts_further_values_in_more_values() {
        let req = PreparedCreate {
            kind: "product".into(),
            name: "p".into(),
            fields: vec![("diagram".into(), "one".into()), ("diagram".into(), "two".into()), ("label".into(), "P".into())],
            edges: vec![("hasDomain".into(), "domain".into(), "a".into()), ("hasDomain".into(), "domain".into(), "b".into())],
            graph: "urn:chorus:instances".into(),
        };
        let out = prepared_create_ndjson(&[req]);
        assert!(out.contains("\"fields\":{\"diagram\":\"one\",\"label\":\"P\"}"), "{out}");
        assert!(out.contains("\"more_values\":[[\"diagram\",\"two\"]]"), "{out}");
        assert!(out.contains("[\"hasDomain\",\"domain\",\"a\"],[\"hasDomain\",\"domain\",\"b\"]"), "{out}");
    }

    #[test]
    fn prepared_create_ndjson_escapes_every_slot_and_ends_each_record() {
        let req = PreparedCreate {
            kind: "test-result".into(),
            name: "tr-1".into(),
            fields: vec![("ownedBy".into(), "wren".into()), ("testName".into(), "a \"quote\"".into())],
            edges: vec![("ofTest".into(), "test".into(), "test-a".into())],
            graph: "urn:chorus:domains:tests".into(),
        };
        assert_eq!(
            prepared_create_ndjson(&[req]),
            "{\"kind\":\"test-result\",\"name\":\"tr-1\",\"fields\":{\"ownedBy\":\"wren\",\"testName\":\"a \\\"quote\\\"\"},\"more_values\":[],\"edges\":[[\"ofTest\",\"test\",\"test-a\"]],\"graph\":\"urn:chorus:domains:tests\"}\n",
        );
    }

    #[test]
    fn verified_owner_uses_the_shape_declared_edge_and_ignores_body_owner() {
        let mut table = RouteTable {
            class: format!("{}Domain", NS),
            fields: vec![
                "comment|datatype:string".into(),
                "ownedBy|edge:Role".into(),
            ],
            routes: vec![],
            secured: vec![],
            mandatory: vec![],
            write_required: vec![],
            repo_target: String::new(),
            exposure: vec![],
            instances_graph: INSTANCES_GRAPH.to_string(),
            tree_edges: vec![],
            tree_order: None,
            model_version: "unclassified".to_string(),
        };
        let req = prepare_create(
            r#"{"name":"fresh","comment":"ok","ownedBy":"attacker"}"#,
            &table,
            "wren",
            "",
        )
        .expect("shape-aware owner projection");
        assert_eq!(req.fields, vec![("comment".into(), "ok".into())]);
        assert_eq!(
            req.edges,
            vec![("ownedBy".into(), "role".into(), "wren".into())],
        );

        table.fields = vec!["comment|datatype:string".into()];
        let legacy = prepare_create(
            r#"{"name":"fresh","comment":"ok"}"#,
            &table,
            "wren",
            "",
        )
        .expect("legacy literal owner projection");
        assert_eq!(
            legacy.fields,
            vec![("ownedBy".into(), "wren".into()), ("comment".into(), "ok".into())],
        );
        assert!(legacy.edges.is_empty());

        table.fields.push("partOf|edge:Domain".into());
        let invalid = prepare_create(
            r#"{"name":"fresh","comment":"ok","partOf":"!!!"}"#,
            &table,
            "wren",
            "",
        )
        .expect_err("invalid edge target local names are client validation errors");
        assert_eq!(invalid.tag, "validation");
        assert!(invalid.message.contains("edge 'partOf'"));
    }
}

/// #4101 — the stamps the door adds to every create and replace on a class whose
/// shape carries them: changedAt (UTC ISO, now) and changedIn (the land's commit,
/// from the X-Landed-Commit header the poster sends; "unknown" for a hand write).
/// A body that tries to set either is refused: they are facts about the write.
pub fn write_stamps(table: &RouteTable, landed_commit: &str) -> Vec<(String, String)> {
    let has = |k: &str| table.fields.iter().any(|f| f.split('|').next() == Some(k));
    let mut out = Vec::new();
    if has("changedAt") {
        let now = Command::new("date").args(["-u", "+%Y-%m-%dT%H:%M:%SZ"]).output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
        out.push(("changedAt".to_string(), now));
    }
    if has("changedIn") {
        out.push(("changedIn".to_string(), if landed_commit.trim().is_empty() { "unknown".to_string() } else { landed_commit.trim().to_string() }));
    }
    out
}

pub fn body_sets_a_stamp(body: &str) -> Option<String> {
    json_top_level_keys(body).into_iter().find(|k| k == "changedAt" || k == "changedIn" || k == "version")
}

/// #4101 — the version a person can say: the row's write count. `previous` is the
/// row's current version (None on create). Only for classes whose shape carries it.
pub fn version_stamp(table: &RouteTable, previous: Option<&str>) -> Option<(String, String)> {
    if !table.fields.iter().any(|f| f.split('|').next() == Some("version")) { return None; }
    let next = previous.and_then(|v| v.trim().parse::<u64>().ok()).unwrap_or(0) + 1;
    Some(("version".to_string(), next.to_string()))
}

fn query_version(class: &str, entity: &str, instances_graph: &str) -> Option<String> {
    let q = format!(
        "PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <{g}> {{ <{s}> chorus:version ?v }} }}",
        ns = NS, g = instances_graph, s = entity_subject(class, entity)
    );
    sparql_json(&q).ok().and_then(|b| select_v(&b).into_iter().next())
}

/// #4102 — the home graph of a class, resolved the way the route table resolves
/// it (declared chorus:instancesGraph, else the domain that definesVocabulary it).
/// A Revision is a row of chorus:Revision, so it lives where that class lives —
/// not in the graph of the row it is a revision of, and never in a catch-all.
fn class_instances_graph(class: &str) -> R<String> {
    let igq = format!(
        "PREFIX sh: <http://www.w3.org/ns/shacl#> PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <{g}> {{ ?s sh:targetClass <{c}> ; chorus:instancesGraph ?v }} }}",
        ns = NS, g = ONTOLOGY_GRAPH, c = class
    );
    let declared = select_v(&sparql_json(&igq)?).into_iter().next();
    let dq = format!(
        "PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <{g}> {{ ?d chorus:definesVocabulary <{c}> BIND(REPLACE(STR(?d), '.*[#/]', '') AS ?v) }} }} LIMIT 1",
        ns = NS, g = ONTOLOGY_GRAPH, c = class
    );
    let domain = select_v(&sparql_json(&dq)?).into_iter().next();
    resolve_instances_graph(declared.as_deref(), domain.as_deref())
}

/// Splice two flat JSON objects into one. Used to put a row's edges back beside
/// its literals for the revision snapshot; either side may be empty (`{  }`).
fn merge_json_objects(a: &str, b: &str) -> String {
    let inner = |o: &str| o.trim().trim_start_matches('{').trim_end_matches('}').trim().to_string();
    let (ai, bi) = (inner(a), inner(b));
    match (ai.is_empty(), bi.is_empty()) {
        (true, true) => "{  }".to_string(),
        (true, false) => format!("{{ {} }}", bi),
        (false, true) => format!("{{ {} }}", ai),
        (false, false) => format!("{{ {}, {} }}", ai, bi),
    }
}

/// #4102 — write the row's CURRENT version as a Revision (kind revision, name
/// `<kind>-<name>-v<version>`), snapshot = the row's data JSON as served. Rows
/// without a version (never written through the door since #4101) keep "0".
fn build_revision(table: &RouteTable, name: &str, caller_role: &str) -> R<PreparedCreate> {
    // The snapshot must carry the EDGES too, not only the literals: AC2's diff is
    // "an added domain, a removed diagram", and both are edges. entity_json splits
    // them (#3635 read-surface quirk — an entity read serves literals, edges come
    // back separately), so a snapshot built from `data` alone could never show the
    // change Jeff asked to see.
    let (data, links) = entity_json(&table.class, name, &table.exposure, true, &table.instances_graph)?;
    // A snapshot must look like the row as SERVED, so a diff of two versions reads
    // the same as the page: an entity read tags edge targets `chorus:<name>` while
    // every collection read serves the bare minted name, and a snapshot carrying
    // the other spelling makes every edge look changed when nothing changed.
    let links = links.replace("\"chorus:", "\"");
    let data = merge_json_objects(&data, &links);
    let prev = query_version(&table.class, name, &table.instances_graph).unwrap_or_else(|| "0".to_string());
    let class_local = table.class.rsplit('#').next().unwrap_or("");
    let kind = kind_of_class(class_local);
    let plural = pluralize(class_local);
    let rev_name = format!("{}-{}-v{}", kind, name, prev);
    let mut fields: Vec<(String, String)> = vec![
        ("label".to_string(), format!("{}/{} v{}", plural, name, prev)),
        ("ofRow".to_string(), format!("{}/{}", plural, name)),
        ("version".to_string(), prev),
        ("snapshot".to_string(), data),
    ];
    for k in ["changedAt", "changedIn"] {
        if let Some(v) = json_field(&fields[3].1, k) { fields.push((k.to_string(), v)); }
    }
    let edges = vec![("ownedBy".to_string(), "role".to_string(), caller_role.to_string())];
    // Every row in its own domain graph, never a catch-all (Jeff, 2026-09-03):
    // for a Revision that is the home of chorus:Revision, which is also the graph
    // /revisions reads — so a document's history is served by the same route as a
    // product's, whatever graph the row itself lives in.
    let rev_graph = class_instances_graph(&format!("{}Revision", NS))?;
    Ok(PreparedCreate { kind: "revision".to_string(), name: rev_name, fields, edges, graph: rev_graph })
}

pub fn handle_write(method: &str, path: &str, body: &str, table: &RouteTable, caller_role: &str, token: &str) -> (u16, String) {
    handle_write_stamped(method, path, body, table, caller_role, token, "")
}

pub fn handle_write_stamped(method: &str, path: &str, body: &str, table: &RouteTable, caller_role: &str, token: &str, landed_commit: &str) -> (u16, String) {
    let class_local = table.class.rsplit('#').next().unwrap_or("");
    let plural = pluralize(class_local);
    let op = match parse_write(method, path, &plural) {
        Some(o) => o,
        None => return write_resp("not-found", "no such write route"),
    };
    // #3573 (bounds) — cap the structured write body before any graph work. A single
    // entity write is < 1 KiB; an unbounded body is the runaway/oversized vector.
    if body.len() > MAX_WRITE_BYTES {
        return write_resp("validation", &format!(
            "write body {} bytes exceeds {}-byte cap", body.len(), MAX_WRITE_BYTES));
    }
    // #4102 — revisions are written by the door at replace, never by a caller
    if table.class.ends_with("#Revision") {
        return write_resp("validation", "revisions are kept by the door when a row is replaced; they cannot be written directly");
    }
    // #4101 — the stamps are the door's, never the body's
    if let Some(k) = body_sets_a_stamp(body) {
        return write_resp("validation", &format!("'{}' is stamped by the door from the write itself; it cannot be set in the body", k));
    }
    // entity name (None for create/create-batch) + injection-safety
    let entity: Option<String> = match &op {
        WriteOp::CreateEntity | WriteOp::CreateBatch => None,
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
        let owned = query_owned_by(&table.class, e, &table.instances_graph);
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
            if insert && edge_is_single_valued(edge) && partof_exists(name, &table.instances_graph) {
                emit_write_spine(caller_role, "add-edge", name, edge, "conflict");
                return write_resp("conflict", "partOf is single-valued: node already has a parent");
            }
            // #3468 — DELEGATE to the DAL (link/unlink): incremental edge write with
            // referential integrity + witness. Replaces the raw build_edge_update +
            // sparql_update path so edges ride the ONE governed write path too.
            let kind = kind_of_class(class_local);
            // #4102 — the version this edge change displaces goes with it, in one
            // update. A row with no predecessor (never written through the door)
            // simply has nothing to keep.
            let rev = build_revision(table, name, caller_role).ok();
            match dal_edge_keeping(insert, &kind, name, pred, &target, token, &table.instances_graph, rev.as_ref()) {
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
            let kind = kind_of_class(class_local);
            match dal_delete(&kind, name, token, &table.instances_graph) {
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
        WriteOp::CreateEntity => handle_create(body, table, caller_role, token, landed_commit),
        WriteOp::CreateBatch => handle_create_batch(body, table, caller_role, token, landed_commit),
        WriteOp::ReplaceEntity { name } => {
            // REPLACE: authZ (ownedBy == caller) already enforced in the entity block
            // above. Must exist (404 otherwise).
            if !entity_exists(&table.class, name, &table.instances_graph) {
                return write_resp("not-found", "entity does not exist");
            }
            // #3573 AC5 — closed-shape: reject an off-model property on replace too.
            if let Some(bad) = off_model_property(body, &table.fields) {
                emit_write_spine(caller_role, "replace", name, "", "off-model");
                return write_resp("validation", &format!("off-model property '{}' is not in the shape", bad));
            }
            let props = collect_entity_props(body, &table.fields);
            let edges = collect_entity_edges(body, &table.fields);
            if props.is_empty() && edges.is_empty() {
                return write_resp("validation", "replace requires at least one shape property in the body");
            }
            // #3468 — DELEGATE to the DAL `add` (idempotent full upsert). NOTE: the
            // DAL is single-writer full-replace by design (#3345) — a replace must
            // restate the COMPLETE entity (the floor re-applies; omitted edges/fields
            // are not preserved). This unifies replace onto the DAL's one write
            // semantic rather than athena-make's prior partial-update (a competing impl).
            let kind = kind_of_class(class_local);
            let (mut fields, mut owner_edges) = verified_owner_projection(table, caller_role);
            fields.extend(props.iter().filter(|(field, _)| field != "ownedBy" && field != "changedAt" && field != "changedIn" && field != "version").cloned());
            fields.extend(write_stamps(table, landed_commit));   // #4101
            let prev = query_version(&table.class, name, &table.instances_graph);
            fields.extend(version_stamp(table, prev.as_deref()));
            if table.fields.iter().any(|f| f.split('|').next() == Some("docState")) && !fields.iter().any(|(f, _)| f == "docState") {
                fields.push(("docState".to_string(), "draft".to_string()));
            }
            owner_edges.extend(
                edges
                    .iter()
                    .filter(|(property, _, _)| property != "ownedBy")
                    .cloned(),
            );
            // #4102 — keep the version being replaced, as a Revision row, before the
            // overwrite: its full data as one JSON snapshot, so any two versions diff
            // field by field on the page and through the API (Jeff: the Staples Athena
            // revision history). Fails closed: no revision, no replace.
            let mut records: Vec<PreparedCreate> = Vec::new();
            match build_revision(table, name, caller_role) {
                Ok(rev) => records.push(rev),
                // No predecessor to keep is not a failure: the row is not in the
                // graph this shape reads from, so this write is a create in all
                // but name. Any OTHER error fails closed — losing a version
                // silently is the one outcome this feature cannot have.
                Err(e) if e == "not-found" => {}
                Err(e) => {
                    emit_write_spine(caller_role, "replace", name, "", "revision-fail");
                    return write_resp("validation", &format!("could not keep the prior version as a revision, replace refused: {}", e));
                }
            }
            records.push(PreparedCreate {
                kind: kind.clone(), name: name.to_string(), fields: fields.clone(),
                edges: owner_edges.clone(), graph: table.instances_graph.clone(),
            });
            // The revision and the row it displaces go to the store as ONE update
            // (write-many): a failure between them would otherwise record a version
            // for a change that never happened.
            match dal_write_many(&prepared_create_ndjson(&records), token) {
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
      "title": "athena-make — generated {class_l} API (generated dashboard — do not hand-edit)" }},
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
  "uid": "athena-make-{class_l}",
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
    // Create bodies are not read projections. They carry a local `name` and
    // scalar field/edge values; ownedBy is injected from the verified principal.
    // Batch items therefore need their own schema rather than the read schema's
    // EdgeRef objects and server-generated audit fields.
    let mut replace_props = Vec::new();
    for f in &t.fields {
        let (name, _) = f.split_once('|').unwrap_or((f.as_str(), "plain"));
        if name != "ownedBy" {
            replace_props.push(format!("\"{}\": {{ \"type\": \"string\" }}", name));
        }
    }
    replace_props.sort();
    let mut create_props = replace_props.clone();
    create_props.push("\"name\": { \"type\": \"string\" }".to_string());
    create_props.sort();
    let mut create_required = vec!["name".to_string()];
    create_required.extend(
        t.write_required
            .iter()
            .filter(|name| name.as_str() != "ownedBy" && name.as_str() != "label")
            .cloned(),
    );
    create_required.sort();
    create_required.dedup();
    let create_required_json = create_required
        .iter()
        .map(|name| format!("\"{}\"", name))
        .collect::<Vec<_>>()
        .join(", ");
    let mut replace_required = t
        .write_required
        .iter()
        .filter(|name| name.as_str() != "ownedBy" && name.as_str() != "label")
        .cloned()
        .collect::<Vec<_>>();
    replace_required.sort();
    replace_required.dedup();
    let replace_required_json = replace_required
        .iter()
        .map(|name| format!("\"{}\"", name))
        .collect::<Vec<_>>()
        .join(", ");
    // #3454 — method-aware: group operations by path so a path with both a GET
    // (read) and POST/PUT/DELETE (generated write) emits one path object with
    // multiple operation keys (valid OpenAPI). Writes document the typed-error
    // taxonomy (write_status) + the per-route requestBody (batch = JSON array).
    let mut by_path: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    for r in &t.routes {
        let (method, raw) = r.split_once(' ').unwrap_or(("GET", r.as_str()));
        let p = raw.replace(":name", "{name}");
        let m = method.to_ascii_lowercase();
        let op = if m == "get" {
            let (resp, params) = if p.ends_with("/{name}") {
                (format!("#/components/schemas/{}", class_short), NAME_PARAM)
            } else if p.ends_with("/tree") {
                // #3660 — the tree read: nested recursion, depth-bounded
                ("#/components/schemas/Tree".to_string(), TREE_PARAMS)
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
            let request_schema = if p.ends_with("/batch") {
                Some(format!("{{ \"type\": \"array\", \"minItems\": 1, \"items\": {{ \"$ref\": \"#/components/schemas/{}Create\" }} }}", class_short))
            } else if m == "post" && !p.contains("{name}") {
                Some(format!("{{ \"$ref\": \"#/components/schemas/{}Create\" }}", class_short))
            } else if m == "put" && p.ends_with("/{name}") {
                Some(format!("{{ \"$ref\": \"#/components/schemas/{}Replace\" }}", class_short))
            } else if p.contains("/{name}/") {
                Some("{ \"type\": \"object\", \"additionalProperties\": false, \"properties\": { \"target\": { \"type\": \"string\" } }, \"required\": [\"target\"] }".to_string())
            } else {
                None
            };
            let request_body = request_schema
                .map(|schema| format!("\"requestBody\": {{ \"required\": true, \"content\": {{ \"application/json\": {{ \"schema\": {} }} }} }}, ", schema))
                .unwrap_or_default();
            format!(
                "\"{}\": {{ {}{}\"responses\": {{ \"200\": {{ \"description\": \"ok\" }}, \"201\": {{ \"description\": \"created\" }}, \"401\": {{ \"description\": \"authn-missing\" }}, \"403\": {{ \"description\": \"authz (ownedBy)\" }}, \"404\": {{ \"description\": \"not-found\" }}, \"409\": {{ \"description\": \"conflict (single-parent partOf)\" }}, \"422\": {{ \"description\": \"validation\" }}, \"502\": {{ \"description\": \"DAL unavailable or failed\" }} }} }}",
                m, params, request_body
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
        "{{\n  \"openapi\": \"3.1.0\",\n  \"info\": {{ \"title\": \"OWL API — generated {class_short} API\", \"version\": \"0\", \"description\": \"Generated from {class} shapes in {graph}. Regenerate, never hand-edit (#3354).\" }},\n  \"paths\": {{\n{paths}\n  }},\n  \"components\": {{ \"schemas\": {{\n    \"EdgeRef\": {{ \"type\": \"object\", \"properties\": {{ \"name\": {{ \"type\": \"string\" }}, \"label\": {{ \"type\": \"string\" }} }} }},\n    \"{class_short}\": {{ \"type\": \"object\", \"properties\": {{ {props} }}{required} }},\n    \"{class_short}Create\": {{ \"type\": \"object\", \"additionalProperties\": false, \"properties\": {{ {create_props} }}, \"required\": [{create_required}] }},\n    \"{class_short}Replace\": {{ \"type\": \"object\", \"additionalProperties\": false, \"properties\": {{ {replace_props} }}, \"required\": [{replace_required}] }},\n    \"List\": {{ \"type\": \"object\", \"properties\": {{ \"count\": {{ \"type\": \"integer\" }}, \"items\": {{ \"type\": \"array\", \"items\": {{ \"type\": \"object\", \"properties\": {{ \"name\": {{ \"type\": \"string\" }}, \"label\": {{ \"type\": \"string\" }}, \"status\": {{ \"type\": \"string\" }} }} }} }} }} }},\n    \"Fold\": {{ \"type\": \"object\", \"properties\": {{ \"{class_l}\": {{ \"type\": \"string\" }}, \"count\": {{ \"type\": \"integer\" }}, \"contains\": {{ \"type\": \"array\", \"items\": {{ \"type\": \"string\" }} }} }} }},\n    \"Tree\": {{ \"type\": \"object\", \"properties\": {{ \"name\": {{ \"type\": \"string\" }}, \"children\": {{ \"type\": \"array\", \"items\": {{ \"$ref\": \"#/components/schemas/Tree\" }} }} }} }},\n    \"Schema\": {{ \"type\": \"object\" }}\n  }} }}\n}}\n",
        class_short = class_short,
        required = required,
        class = t.class,
        graph = ONTOLOGY_GRAPH,
        paths = paths,
        props = props.join(", "),
        create_props = create_props.join(", "),
        create_required = create_required_json,
        replace_props = replace_props.join(", "),
        replace_required = replace_required_json,
        class_l = class_l
    )
}

const NAME_PARAM: &str = "\"parameters\": [ { \"name\": \"name\", \"in\": \"path\", \"required\": true, \"schema\": { \"type\": \"string\" } } ], ";

// #3660 — the tree read's parameters: path name + the depth bound (query).
const TREE_PARAMS: &str = "\"parameters\": [ { \"name\": \"name\", \"in\": \"path\", \"required\": true, \"schema\": { \"type\": \"string\" } }, { \"name\": \"depth\", \"in\": \"query\", \"required\": false, \"schema\": { \"type\": \"integer\" } } ], ";

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
    let write_required = t.write_required.iter().map(|m| format!("\"{}\"", m)).collect::<Vec<_>>().join(", ");
    format!(
        "{{\n  \"generatedFrom\": \"{}\",\n  \"graph\": \"{}\",\n  \"fields\": [{}],\n  \"routes\": [{}],\n  \"secured\": [{}],\n  \"mandatory\": [{}],\n  \"writeRequired\": [{}]\n}}\n",
        t.class, ONTOLOGY_GRAPH, fields, routes, secured, mandatory, write_required
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
        tool("add", format!("POST /{}", plural), ", \"delegatesTo\": \"DAL (athena-model)\""),
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

/// #3902 — read the vocabulary semver once per process from the ontology graph
/// (chorus:model chorus:vocabVersion, written by the pen's generated projection).
/// Cached: deploys restart this service, so the cache lifetime IS the deploy.
/// #3947 — TTL cache, NOT OnceLock. The original OnceLock cached at startup
/// forever, so after a model-deploy changed the store the envelope served the
/// pre-deploy version until someone bounced the process (observed live
/// 2026-08-20: served 1.1.0 while the store held 1.0.0; the restart "fix"
/// just swapped which staleness was served). 60s of staleness is honest;
/// forever is the cached-at-startup defect class.
fn versioned_cached(slot: &std::sync::Mutex<Option<(std::time::Instant, String)>>, q: &str) -> String {
    const TTL: std::time::Duration = std::time::Duration::from_secs(60);
    if let Ok(g) = slot.lock() {
        if let Some((at, v)) = g.as_ref() {
            if at.elapsed() < TTL {
                return v.clone();
            }
        }
    }
    let fresh = sparql_json(q).ok()
        .map(|r| select_v(&r))
        .and_then(|v| v.into_iter().next())
        .unwrap_or_else(|| "unversioned".to_string());
    if let Ok(mut g) = slot.lock() {
        *g = Some((std::time::Instant::now(), fresh.clone()));
    }
    fresh
}

fn vocab_version_cached() -> String {
    static VV: std::sync::Mutex<Option<(std::time::Instant, String)>> = std::sync::Mutex::new(None);
    let q = format!(
        "PREFIX chorus: <{ns}> SELECT ?v WHERE {{ GRAPH <{g}> {{ chorus:model chorus:vocabVersion ?v }} }} LIMIT 1",
        ns = NS, g = ONTOLOGY_GRAPH
    );
    versioned_cached(&VV, &q)
}

/// #3947 — the ONTOLOGY's own version (owl:versionInfo on the chorus# node),
/// distinct from per-class modelVersion (review state, #3704) and from the
/// vocabulary ledger's vocabVersion (#3902). Three versions, three questions;
/// this one answers "which ontology release is this store serving?". ABSENT
/// reads "unversioned" LOUDLY — never an empty string a consumer can misparse.
fn ontology_version_cached() -> String {
    static OV: std::sync::Mutex<Option<(std::time::Instant, String)>> = std::sync::Mutex::new(None);
    let q = format!(
        "PREFIX owl: <http://www.w3.org/2002/07/owl#> SELECT ?v WHERE {{ GRAPH <{g}> {{ <https://jeffbridwell.com/chorus#> owl:versionInfo ?v }} }} LIMIT 1",
        g = ONTOLOGY_GRAPH
    );
    versioned_cached(&OV, &q)
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c <= '\u{001F}' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// #3435 — coerce a Property's string-encoded value to its typed JSON fragment,
/// ONCE per propertyValueType (string | int | bool | json | list). The effective-config
/// read calls this so a consumer gets a typed value, not a raw literal. Fails LOUD on a
/// type mismatch — never silently defaults to string (a wrong type must surface, not hide).
/// list/json are stored already-encoded: coercion is a structural shape-check + passthrough
/// (zero-dep — athena-make takes no JSON parser; a full parse is a follow-on if depth matters).
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
/// each Property as "iri|key|valueType|value" into ?v (athena-make's single-var seam). `value`
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

/// #3863 — read a ScopeKind from a class, by name. Accepts a full IRI or a bare
/// localname; anything that is not one of the six cascade scopes returns None.
///
/// The `None` matters more than the `Some`s. `build_scope_node`'s caller used to
/// pass `ScopeKind::Service` for whatever node was asked about, which meant a
/// node of any class silently became a Service — and since a chain of one is
/// trivially "strictly descending", nothing ever complained. Reading the kind
/// and refusing the unknown is what turns the cascade from decoration into a
/// rule that can be violated.
pub fn scope_kind_from_class(class: &str) -> Option<ScopeKind> {
    let local = class.rsplit(['#', '/']).next().unwrap_or(class);
    match local {
        "Role" => Some(ScopeKind::Role),
        "Service" => Some(ScopeKind::Service),
        "Domain" => Some(ScopeKind::Domain),
        "Product" => Some(ScopeKind::Product),
        "ValueStreamStep" => Some(ScopeKind::ValueStreamStep),
        "ValueStream" => Some(ScopeKind::ValueStream),
        _ => None,
    }
}

/// #3863 — parse one ancestry row: `owner|ownerClass|propIri|key|valueType|value`.
///
/// `value` is LAST and `splitn(6)` gives it the remainder, so a config value may
/// itself contain '|'. Every field before it is required: a row missing its owner
/// or class cannot be placed in a chain, and placing it anyway is how a property
/// ends up attributed to the wrong scope.
fn parse_ancestry_row(row: &str) -> R<(String, ScopeKind, properties_resolver::PropertyDatum)> {
    let mut it = row.splitn(6, '|');
    let (owner, class, iri, key, vtype, value) =
        match (it.next(), it.next(), it.next(), it.next(), it.next(), it.next()) {
            (Some(o), Some(c), Some(i), Some(k), Some(t), Some(v))
                if !o.is_empty() && !c.is_empty() && !i.is_empty() && !k.is_empty() =>
            {
                (o, c, i, k, t, v)
            }
            _ => return Err(format!("malformed ancestry row: {:?}", row)),
        };
    let kind = scope_kind_from_class(class)
        .ok_or_else(|| format!("class {:?} is not a cascade scope (row {:?})", class, row))?;
    Ok((
        owner.to_string(),
        kind,
        properties_resolver::PropertyDatum {
            iri: iri.to_string(),
            key: key.to_string(),
            value: value.to_string(),
            value_type: vtype.to_string(),
        },
    ))
}

/// #3863 — assemble ancestry rows into the scope chain, most specific first.
///
/// Rows arrive flat (one per property, each tagged with its owning scope and
/// that scope's class) because the fetch is ONE round-trip. Properties on the
/// same scope collapse into one node — a scope with three properties is one
/// link in the chain, not three.
///
/// Order is by `ScopeKind::rank`, descending, which is the precondition
/// `decide_effective_value` enforces. Sorting here rather than trusting the
/// query's ORDER BY keeps the guarantee in code the tests can reach.
pub fn build_scope_chain(rows: &[String]) -> R<Vec<ScopeNode>> {
    let mut nodes: Vec<ScopeNode> = Vec::new();
    for row in rows {
        let (owner, kind, datum) = parse_ancestry_row(row)?;
        match nodes.iter_mut().find(|n| n.iri == owner) {
            Some(existing) => existing.properties.push(datum),
            None => nodes.push(ScopeNode { kind, iri: owner, properties: vec![datum] }),
        }
    }
    nodes.sort_by(|a, b| b.kind.rank().cmp(&a.kind.rank()));
    Ok(nodes)
}

/// #3435 — the node-scoped effective-config fetch. Reads `urn:chorus:instances` LIVE
/// via SPARQL — NO projection/mirror/sqlite store (the AC invariant; the query-builder
/// test asserts the instances graph + hasProperty traversal so a projection swap goes
/// red). Traverses hasProperty→Property and returns ALL the node's declared properties
/// as "iri|key|valueType|value" rows (value LAST so it may contain '|') in ONE round-trip.
/// The key is selected in pure code (decide_effective_value), never filtered in SPARQL —
/// so the round-trip stays one and key-selection stays unit-tested.
/// #3863 — now walks the node's ANCESTRY, not just the node.
///
/// The containment path is zero-or-more hops, so the node itself is the first
/// link and each enclosing scope follows. Each row carries the OWNING scope and
/// its class, because a property means nothing without knowing which scope set
/// it — that is precisely what the old single-node fetch could not say, and why
/// the handler had to assume `ScopeKind::Service`.
///
/// `?ownerClass` is constrained with VALUES to the six cascade scopes. Without
/// it, `?owner a ?c` returns every type an individual carries (owl:Class,
/// owl:NamedIndividual, its domain class) and one node would multiply into
/// several chain links, most of them nonsense.
pub fn effective_fetch_query(node_iri: &str, instances_graph: &str) -> String {
    format!(
        // #3876 — TWO named graph blocks, because the facts genuinely live in
        // two homes and the rule says which:
        //
        //   an EDGE lives in the home graph of the node it hangs off
        //   FIELDS live with their subject
        //
        // `role-wren hasProperty prop-x` sits in the ROLE's home (instances,
        // per RoleShape's pin); `prop-x propertyKey "response.word.cap"` sits
        // in the PROPERTY's home (ontology, per PropertyShape's pin).
        //
        // One block spanning both finds a typed Role with no properties and
        // answers 404 — which it did for two days after every other piece of
        // this path existed. This is NOT a union: each block names the home the
        // rule assigns it, so a misplaced triple still fails loudly.
        "SELECT ?v WHERE {{ \
           GRAPH <{ng}> {{ \
             VALUES ?ownerClass {{ <{ns}Role> <{ns}Service> <{ns}Domain> <{ns}Product> <{ns}ValueStreamStep> <{ns}ValueStream> }} \
             <{node}> (<{ns}partOf>|<{ns}memberOf>|<{ns}atStep>|<{ns}ownedBy>)* ?owner . \
             ?owner a ?ownerClass . \
             ?owner <{ns}hasProperty> ?prop . \
           }} \
           GRAPH <{g}> {{ \
             ?prop <{ns}propertyKey> ?key . \
             ?prop <{ns}propertyValue> ?value . \
             ?prop <{ns}propertyValueType> ?vtype . \
           }} \
           BIND(CONCAT(STR(?owner), \"|\", STR(?ownerClass), \"|\", STR(?prop), \"|\", STR(?key), \"|\", STR(?vtype), \"|\", STR(?value)) AS ?v) \
         }}",
        g = instances_graph,
        // #3876 — the NODE's home. Named separately from the Property's home
        // above. Today every cascade scope (Role, Service, Domain, Product,
        // Step, Stream) is an individual pinned to instances, so this is a
        // constant; it is a parameter-shaped constant rather than an inlined
        // string so that when a shape moves its pin, the fix is one call site
        // and not a hunt through a format literal.
        ng = NODE_HOME_GRAPH,
        ns = NS,
        node = node_iri
    )
}

/// #3876 — where cascade scope individuals live. RoleShape pins it; the other
/// five scope shapes agree today. Kept named so the coupling is visible.
pub const NODE_HOME_GRAPH: &str = "urn:chorus:instances";

/// #3435 — shape the effective-config response from a node's already-fetched rows.
/// The handler's pure core (it adds only the live `sparql_json` fetch): build the
/// 1-element scope chain, resolve `key`, coerce. 200 with the typed value + provenance,
/// 404 if the key is unset on the node, 500 on a malformed row / coercion mismatch.
/// `value` is the coerced JSON fragment (bare `3000`, `true`, or a quoted string).
pub fn effective_response(node_name: &str, key: &str, rows: &[String]) -> (u16, String) {
    // #3863 — the chain is the node's own ancestry, built from the rows' own
    // owner+class tags. Was: one node, `ScopeKind::Service` assumed. The
    // cascade could not cascade, because there was never more than one link.
    let chain = match build_scope_chain(rows) {
        Ok(c) => c,
        Err(e) => return (500, format!("{{\"error\":\"{}\"}}", json_escape(&e))),
    };
    match decide_effective_value(&chain, key) {
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

/// #3739 — the /schema projection as a PURE FUNCTION (was inline in the serve
/// loop), extended to carry the model's LINES: every `field|edge:Target`
/// annotation becomes a typed relationship {property, range}. A class with no
/// edges serves an explicit empty array — "no relationships" is a served
/// finding (an isolated class is real information), distinct from "not served".
pub fn schema_set_json(tables: &[RouteTable]) -> String {
    let items: Vec<String> = tables
        .iter()
        .map(|t| {
            let local = t.class.rsplit('#').next().unwrap_or("");
            let fields: Vec<String> =
                t.fields.iter().map(|f| format!("\"{}\"", json_escape(f))).collect();
            let mandatory: Vec<String> =
                t.mandatory.iter().map(|m| format!("\"{}\"", json_escape(m))).collect();
            let relationships: Vec<String> = t
                .fields
                .iter()
                .filter_map(|f| {
                    let (prop, ann) = f.split_once('|')?;
                    let range = ann.strip_prefix("edge:")?;
                    Some(format!(
                        "{{ \"property\": \"{}\", \"range\": \"{}\" }}",
                        json_escape(prop),
                        json_escape(range)
                    ))
                })
                .collect();
            format!(
                "{{ \"kind\": \"{}\", \"fields\": [{}], \"mandatory\": [{}], \"relationships\": [{}], \"modelVersion\": \"{}\" }}",
                json_escape(local),
                fields.join(", "),
                mandatory.join(", "),
                relationships.join(", "),
                json_escape(&t.model_version)
            )
        })
        .collect();
    format!(
        "{{ \"apiVersion\": \"{}\", \"service\": \"athena-make\", \"kind\": \"SchemaSet\", \"graph\": \"{}\", \"count\": {}, \"classes\": [{}] }}",
        API_VERSION, ONTOLOGY_GRAPH, tables.len(), items.join(", ")
    )
}

/// #3420 — GENERATE the Athena domain page as a PROJECTION on the #3415 design system,
/// replacing the hand-built domain-detail page. page_html emits the STATIC SHELL — the
/// real anatomy (breadcrumb → identity → stats → promise → completeness → facet sections)
/// with system.css classes + the generated marker — and the shared /js/domain-renderer.js
/// fills it: client-fetching the EXISTING Athena/chorus-domain facet endpoints (same-origin)
/// + the athena-make model identity overlay (owner/step/comment). One page renders any domain
/// via ?name=. PROJECTION — regenerate, never hand-edit. Built in stages (#3420 design pass):
/// shell + identity/stats/completeness first; the 17 facet sections in the renderer.
pub fn page_html(t: &RouteTable) -> String {
    let class_short = t.class.rsplit('#').next().unwrap_or("Domain").to_string();
    let collection = pluralize(&class_short);
    // #3545 — Domain keeps its rich renderer; every other class projects via the generic entity-renderer.
    let renderer = if class_short == "Domain" { "domain-renderer.js" } else { "entity-renderer.js" };
    let tmpl = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{{CLASS}} — Athena</title>
<!-- GENERATED by athena-make page_html (#3420). PROJECTION — regenerate from the model, never hand-edit. system.css = #3415 vocabulary; facet data from the existing Athena endpoints (same-origin); identity + decomposition (contains/partof) from athena-make. -->
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
  <p class="muted" style="margin-top:var(--space-5)">Athena &middot; Chorus &middot; GENERATED page (athena-make) — live from the model</p>
</div>
<script>window.OWL_PORT = 3360; window.OWL_CLASS = "{{CLASS}}"; window.OWL_COLLECTION = "{{COLLECTION}}";</script>
<script src="https://cdn.jsdelivr.net/npm/mermaid@10/dist/mermaid.min.js"></script>
<script src="/js/{{RENDERER}}" defer></script>
<script src="/js/content-actions.js" defer></script>
</body>
</html>
"##;
    tmpl.replace("{{CLASS}}", &class_short)
        .replace("{{COLLECTION}}", &collection)
        .replace("{{RENDERER}}", renderer)
}

/// Build the JSON for one entity: every direct property in the instances graph.
/// #3506 / ADR-047 §1+§3 — project an entity into (data, links). DATATYPE props are
/// scalars in `data`; EDGE props (NS-IRI objects) project into `links` as target id
/// refs — single-valued → string, multi-valued → ARRAY. The array is the fix for a
/// real bug the contract exposed: the old shape emitted a multi-valued edge (e.g.
/// `contains`, ~80 files on the cards domain) as duplicate JSON keys — malformed
/// JSON. links also drops the per-edge label lookup (#3354): a link is a traversal
/// ref, the label lives on the target — fewer queries, ADR-conformant.
fn entity_json(class: &str, name: &str, exposure: &[(String, String)], authed: bool, instances_graph: &str) -> R<(String, String)> {
    let subject = entity_subject(class, name);
    let q = format!(
        "SELECT ?v WHERE {{ GRAPH <{g}> {{ <{s}> ?p ?o }} BIND(CONCAT(STR(?p), \"|\", STR(?o)) AS ?v) }} ORDER BY ?v",
        g = instances_graph, s = subject
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

/// #3506 / ADR-047 — the uniform response envelope. Every athena-make read (and, as the
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
    served_from: &str,
    requires_auth: bool,
    data_json: &str,
    links_json: &str,
    count: Option<i64>,
    model_version: &str,
) -> String {
    let mut p: Vec<String> = Vec::new();
    p.push(format!("\"apiVersion\": \"{}\"", API_VERSION));
    p.push(format!("\"kind\": \"{}\"", json_escape(kind)));
    // #3704 — the class's model-version, PROJECTED from chorus:modelVersion (born-v2
    // default when absent). Makes the strangler-fig legible on every generated surface.
    p.push(format!("\"modelVersion\": \"{}\"", json_escape(model_version)));
    // #3902 — the vocabulary's SEMVER, declared at the pen (athena-model bumps
    // the ledger; the store carries chorus:vocabVersion via the generated
    // MODEL_SET projection). A consumer pins against this to detect breaking
    // model changes. "unversioned" = the projection is absent from the store —
    // loud, never defaulted to a number.
    p.push(format!("\"vocabVersion\": \"{}\"", json_escape(&vocab_version_cached())));
    // #3947 — the ontology RELEASE (owl:versionInfo on chorus#). Third version
    // axis: modelVersion = per-class review state, vocabVersion = ledger semver,
    // ontologyVersion = which ontology release this store serves. Kade's 422
    // triage conflated the first and third; naming them separately ends that.
    p.push(format!("\"ontologyVersion\": \"{}\"", json_escape(&ontology_version_cached())));
    if let Some(i) = id {
        p.push(format!("\"id\": \"{}\"", json_escape(i)));
    }
    p.push(format!("\"self\": \"{}\"", json_escape(self_url)));
    p.push(format!(
        "\"generatedFrom\": {{ \"graph\": \"{}\", \"shape\": \"{}\", \"shapeVersion\": \"{}\", \"commit\": \"{}\" }}",
        json_escape(ONTOLOGY_GRAPH), json_escape(shape), json_escape(shape_version), json_escape(commit)
    ));
    // #3749 — servedFrom = the INSTANCE graph this read actually queried.
    // generatedFrom.graph names the schema source; without this field a
    // surface reading the wrong graph is indistinguishable from one reading
    // the right graph (the dual-tenant ambiguity Silas caught 2026-08-05).
    p.push(format!("\"servedFrom\": \"{}\"", json_escape(served_from)));
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
    envelope("Error", None, &instance, &shape, &shape_version, &commit, &table.instances_graph, false, &data, "{}", None, "unclassified")
    // #3706 — an error envelope has no reviewed class behind it; it must not claim "v2".
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
/// #4022 — the collection page is TWO store round-trips, both cheap:
///   1. `collection_page_query`  — subjects only: `?s a <C>` ORDER BY ?s LIMIT/OFFSET
///   2. `collection_project_query` — the projection (labels, status, exposed
///      fields, CONCAT) with `VALUES ?s { <page> }` INSIDE the GRAPH block.
/// The previous shape — one query, `... BIND(CONCAT(...) AS ?v) } ORDER BY ?v LIMIT n`
/// — made the store evaluate every OPTIONAL for every row and sort the CONCAT
/// string before it could take the first n. On 219,366 chorus:TestResult rows that
/// was 20s+ and GET /testresults answered 502 while the same page answered in
/// milliseconds once the subjects were bound. #4010 pushed LIMIT into SPARQL but
/// the OPTIONALs and the sort still ran over the whole collection.
/// Measured 2026-08-28 against the live store (219k rows, page of 20):
///   subjects-only page 0.14s · VALUES-inside-GRAPH projection 0.005s
///   VALUES OUTSIDE the graph block 6.3s · subquery-join 5.8s · old shape 5.2–20s
/// so the placement of VALUES is the fix, not the subquery. Ordering by subject IRI
/// is a total, stable order, so cursor pagination keeps its meaning.
pub fn collection_page_query(graph: &str, class: &str, limit: usize, offset: usize) -> String {
    format!(
        "SELECT (STR(?s) AS ?v) WHERE {{ GRAPH <{g}> {{ ?s a <{c}> }} }} ORDER BY ?s LIMIT {limit} OFFSET {offset}",
        g = graph, c = class, limit = limit, offset = offset
    )
}

/// The projection for ONE page: `where_body` is the collection's GRAPH block
/// (`GRAPH <g> { ?s a <C> . OPTIONAL ... BIND(CONCAT(...) AS ?v) }`); the page's
/// subjects are bound with VALUES placed just inside the block so every OPTIONAL
/// is an index lookup on a bound ?s. Empty page → caller must not query.
pub fn collection_project_query(graph: &str, where_body: &str, subjects: &[String]) -> String {
    let open = format!("GRAPH <{}> {{ ", graph);
    let values: String = subjects.iter()
        .filter(|s| !s.contains(['<', '>', ' ', '"']))
        .map(|s| format!("<{}> ", s)).collect();
    let bound = format!("{}VALUES ?s {{ {}}} ", open, values);
    let body = where_body.replacen(&open, &bound, 1);
    format!("SELECT ?v WHERE {{ {} }} ORDER BY ?s", body)
}

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
        return ((200, "{ \"ok\": true, \"service\": \"athena-make\" }".to_string()), meta);
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

/// The collection row's column separator (see collection_items). U+001F, never '|'.
pub const COL_SEP: char = '\u{1f}';

/// Collection marshal — one JSON object per SUBJECT, multi-valued fields aggregated.
/// #3558 deduped rows by subject (first row wins), which fixed the fan-out counts but
/// silently DROPPED every value after the first (borg showed 1 of 5 hasDomain). #3635:
/// group the fanned rows per subject and merge — a field renders as a string when it
/// has one value and a JSON array when it has several (additive under ADR-047; single-
/// valued fields keep their exact prior shape). label/status take the first non-empty.
pub fn collection_items(rows: Vec<String>, extra_names: &[String]) -> Vec<String> {
    let mut order: Vec<String> = Vec::new();
    let mut by_subj: std::collections::HashMap<String, Vec<Vec<String>>> =
        std::collections::HashMap::new();
    for rowv in rows {
        // #4045 — the column separator is U+001F (unit separator), not '|'. With '|'
        // any value containing a pipe (a table rendered as text: "Path | Today | To-be")
        // shifted every column after it: spine's hasDomain served a sentence from its
        // apiSurface and hasDesignDoc served "404 — no class". Proven on the werk
        // variant 2026-09-02 10:31. U+001F is a control character no authored text
        // carries; the CONCAT in the collection query emits the same byte.
        let cols: Vec<String> = rowv.split(COL_SEP).map(|s| s.to_string()).collect();
        let subj = cols.first().cloned().unwrap_or_default();
        if !by_subj.contains_key(&subj) {
            order.push(subj.clone());
        }
        by_subj.entry(subj).or_default().push(cols);
    }
    order
        .iter()
        .map(|subj| {
            let group = &by_subj[subj];
            let name = subj.rsplit('#').next().unwrap_or(subj);
            let pick = |idx: usize| {
                group
                    .iter()
                    .filter_map(|c| c.get(idx))
                    .find(|v| !v.is_empty())
                    .cloned()
                    .unwrap_or_default()
            };
            let mut obj = format!(
                "{{ \"name\": \"{}\", \"label\": \"{}\", \"status\": \"{}\"",
                json_escape(name),
                json_escape(&pick(1)),
                json_escape(&pick(2))
            );
            for (i, fname) in extra_names.iter().enumerate() {
                let mut vals: Vec<String> = Vec::new();
                for c in group {
                    if let Some(v) = c.get(3 + i) {
                        if !v.is_empty() && !vals.contains(v) {
                            vals.push(v.clone());
                        }
                    }
                }
                let rendered = match vals.len() {
                    0 => "\"\"".to_string(),
                    1 => format!("\"{}\"", json_escape(&vals[0])),
                    _ => format!(
                        "[{}]",
                        vals.iter()
                            .map(|v| format!("\"{}\"", json_escape(v)))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                };
                obj.push_str(&format!(", \"{}\": {}", json_escape(fname), rendered));
            }
            obj.push_str(" }");
            obj
        })
        .collect()
}

fn handle_inner(path: &str, table: &RouteTable, meta: &mut ReqMeta, authed: bool) -> (u16, String) {
    // #3506 / ADR-047 §7 — split the query string off BEFORE route matching, so
    // `?limit=&cursor=` (cursor pagination, AIP-158) never breaks the path parse.
    let (path, query) = match path.split_once('?') {
        Some((p, q)) => (p, q),
        None => (path, ""),
    };
    // #3561 — SERVE the path the discovery document ADVERTISES. Every envelope and
    // every `collection` under `/` is written `/v1/<plural>`, but the router only
    // matched the bare form, so a client doing the correct thing — read discovery,
    // follow the route it names — got a 404. Found by athena.yml's deploy-served gate
    // on 2026-08-21: `/v1/streamevents` 404 while `/streamevents` answered 200.
    // DECLARED != SERVED inside a single binary. Strip the version prefix once, here,
    // where every path already passes; bare paths keep working unchanged.
    let versioned;
    let path = match path.strip_prefix("/v1/") {
        Some(rest) => {
            versioned = format!("/{rest}");
            versioned.as_str()
        }
        None => path,
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
        let q = effective_fetch_query(&entity_subject(&table.class, node), &table.instances_graph);
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
        let t = RouteTable { class: table.class.clone(), fields: table.fields.clone(), routes: table.routes.clone(), secured: table.secured.clone(), mandatory: table.mandatory.clone(), write_required: table.write_required.clone(), repo_target: table.repo_target.clone(), exposure: table.exposure.clone(), instances_graph: table.instances_graph.clone(), tree_edges: vec![], tree_order: None, model_version: table.model_version.clone() };
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
        // #3522 — SHAPE-DRIVEN collection projection. The old query hardcoded
        // name|label|status, dropping every other shape field (e.g. value-stream
        // stageOrder/inStream) — so collections served hollow rows even when the model
        // carried the data (the empty value-stream view). Now also project the shape's
        // direct-path fields (table.fields), ADDITIVELY (extra keys are safe under
        // ADR-047 — name/label/status stay): datatypes as their literal, edges as the
        // target's localname. Pure projection of the shape, so it can't go stale.
        // #3675 — the ADR-048 §3 exposure gate applies to COLLECTION projection too
        // (same per-shape opt-in as entity_json): the first exposure-annotated shape
        // with a live instance (ServiceShape / service-clearing) showed /services
        // emitting the secret-marked implementationPlan unauth. name/label/status stay
        // as the row floor (the #3506 proven contract); extra fields pass field_exposed
        // BEFORE entering the SPARQL projection — ungated fields are never even queried.
        let extra: Vec<(String, bool)> = exposed_projection(
            table
                .fields
                .iter()
                .map(|f| (f.split('|').next().unwrap_or(f).to_string(), f.contains("|edge:")))
                .filter(|(n, _)| n != "label" && n != "status")
                .collect(),
            &table.exposure,
            authed,
        );
        let opts: String = extra
            .iter()
            .enumerate()
            .map(|(i, (n, edge))| {
                if *edge {
                    format!(" OPTIONAL {{ ?s <{ns}{n}> ?e{i} . BIND(REPLACE(STR(?e{i}), \".*[#/]\", \"\") AS ?f{i}) }}", ns = NS, n = n, i = i)
                } else if n == "comment" || n == "label" {
                    // #3749 — rdfs well-knowns: the shape may path rdfs:label /
                    // rdfs:comment (Principle does); the parser strips the
                    // namespace, so probe BOTH and coalesce. chorus: wins ties.
                    format!(" OPTIONAL {{ ?s <{ns}{n}> ?c{i} }} OPTIONAL {{ ?s <http://www.w3.org/2000/01/rdf-schema#{n}> ?r{i} }} BIND(COALESCE(?c{i}, ?r{i}) AS ?f{i})", ns = NS, n = n, i = i)
                } else {
                    format!(" OPTIONAL {{ ?s <{ns}{n}> ?f{i} }}", ns = NS, n = n, i = i)
                }
            })
            .collect();
        let cat: String = (0..extra.len())
            .map(|i| format!(", \"\\u001F\", COALESCE(STR(?f{i}), \"\")", i = i))
            .collect();
        // #4010 — LIMIT/OFFSET pushed into SPARQL, and the total asked separately.
        //
        // Before this the collection query fetched EVERY row and paginated in
        // memory. On the tests domain that is 190,941 chorus:TestResult rows —
        // 74.6MB and 22.3s, past the 20s client deadline at lib.rs:47, so
        // GET /testresults answered 502 and six months of test results were
        // unreadable. The same shape degrades every collection as it grows: the
        // cost of asking for ONE row was the cost of asking for all of them.
        //
        // The cursor is already a plain integer offset (see `paginate`), so
        // pushing it down preserves the response contract exactly — same
        // envelope, same links.next, same meaning — while the store does the
        // slicing it is built to do.
        let limit = query_param(query, "limit")
            .and_then(|l| l.parse::<usize>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(100);
        let offset = query_param(query, "cursor")
            .and_then(|c| c.parse::<usize>().ok())
            .unwrap_or(0);
        let where_body = format!(
            "GRAPH <{g}> {{ ?s a <{c}> . OPTIONAL {{ ?s <{ns}label> ?clabel }} OPTIONAL {{ ?s <http://www.w3.org/2000/01/rdf-schema#label> ?rlabel }} OPTIONAL {{ ?s <{ns}status> ?status }}{opts} BIND(CONCAT(STR(?s), \"\\u001F\", COALESCE(?clabel, ?rlabel, \"\"), \"\\u001F\", COALESCE(?status, \"\"){cat}) AS ?v) }}",
            g = table.instances_graph, c = table.class, ns = NS, opts = opts, cat = cat
        );
        // The TOTAL is a COUNT over subjects only — no OPTIONALs, no CONCAT, so
        // it stays cheap as the collection grows. `count` in the envelope keeps
        // meaning "how many exist", not "how many this page holds".
        let count_q = format!(
            "SELECT (COUNT(DISTINCT ?s) AS ?v) WHERE {{ GRAPH <{g}> {{ ?s a <{c}> }} }}",
            g = table.instances_graph, c = table.class
        );
        let total: i64 = match sparql_json(&count_q) {
            Ok(body) => select_v(&body).first().and_then(|v| v.parse::<i64>().ok()).unwrap_or(0),
            Err(_) => 0,
        };
        let page_q = collection_page_query(&table.instances_graph, &table.class, limit, offset);
        let subjects: Vec<String> = match sparql_json(&page_q) {
            Ok(body) => select_v(&body),
            Err(e) => return (502, error_envelope(table, "", 502, "upstream", &json_escape(&e), &[])),
        };
        let projected: R<String> = if subjects.is_empty() {
            Ok(String::new())
        } else {
            sparql_json(&collection_project_query(&table.instances_graph, &where_body, &subjects))
        };
        return match projected {
            Ok(body) => {
                let extra_names: Vec<String> = extra.iter().map(|(n, _)| n.clone()).collect();
                let items = collection_items(select_v(&body), &extra_names);
                {
                    // #3506 / ADR-047 — the collection list, enveloped + paginated:
                    // kind = the item class, no `id`, `data` = the page, `count` = the
                    // TOTAL, `links.next` = the cursor URL when more remain. Uniform
                    // with the entity read; consumers learn ONE shape.
                    // #4010 — the store already sliced. `items` IS the page.
                    let page: &[String] = &items;
                    let next: Option<usize> = if (offset + page.len()) < (total as usize) {
                        Some(offset + page.len())
                    } else {
                        None
                    };
                    meta.result_count = page.len() as i64;
                    let data = format!("[\n  {}\n]", page.join(",\n  "));
                    let kind = table.class.rsplit('#').next().unwrap_or("Domain");
                    let (shape, shape_version, commit) = shape_meta(kind);
                    let self_url = format!("/{}{}", API_VERSION, plural);
                    let links = match next {
                        Some(n) => format!("{{ \"next\": \"/{}{}?cursor={}&limit={}\" }}", API_VERSION, plural, n, limit),
                        None => "{}".to_string(),
                    };
                    (200, envelope(kind, None, &self_url, &shape, &shape_version, &commit, &table.instances_graph, !table.secured.is_empty(), &data, &links, Some(total), &table.model_version))
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
        // #3660 — GET /<plural>/:name/tree?depth=N — the generated recursive tree
        // read. Descent predicates + sibling rank are PROJECTED from the shape
        // (chorus:treeEdge / chorus:treeOrder); the whole edge set is fetched in
        // ONE query and the recursion runs in pure code (build_tree): nested JSON,
        // rank-ordered siblings, depth-bounded, cycle-refused (node-reuse legal).
        if parts.len() == 3 && parts[2] == "tree" {
            if table.tree_edges.is_empty() {
                // kind did not opt in — no phantom surface
                return (404, "{ \"error\": \"not-found\" }".to_string());
            }
            let depth = query_param(query, "depth")
                .and_then(|d| d.parse::<usize>().ok())
                .unwrap_or(32)
                .min(64);
            let unions: Vec<String> = table
                .tree_edges
                .iter()
                .map(|e| format!("{{ ?s <{ns}{e}> ?o }}", ns = NS, e = e))
                .collect();
            let eq = format!(
                "SELECT ?v WHERE {{ GRAPH <{g}> {{ {u} BIND(CONCAT(REPLACE(STR(?s), '.*[#/]', ''), '|', REPLACE(STR(?o), '.*[#/]', '')) AS ?v) }} }}",
                g = table.instances_graph, u = unions.join(" UNION ")
            );
            let edge_pairs: Vec<(String, String)> = match sparql_json(&eq) {
                Ok(body) => select_v(&body)
                    .into_iter()
                    .filter_map(|row| row.split_once('|').map(|(a, b)| (a.to_string(), b.to_string())))
                    .collect(),
                Err(e) => return (502, format!("{{ \"error\": \"{}\" }}", json_escape(&e))),
            };
            let rank_pairs: Vec<(String, i64)> = match &table.tree_order {
                Some(op) => {
                    let rq = format!(
                        "SELECT ?v WHERE {{ GRAPH <{g}> {{ ?s <{ns}{op}> ?r BIND(CONCAT(REPLACE(STR(?s), '.*[#/]', ''), '|', STR(?r)) AS ?v) }} }}",
                        g = table.instances_graph, ns = NS, op = op
                    );
                    match sparql_json(&rq) {
                        Ok(body) => select_v(&body)
                            .into_iter()
                            .filter_map(|row| {
                                row.split_once('|')
                                    .and_then(|(n, r)| r.parse::<i64>().ok().map(|r| (n.to_string(), r)))
                            })
                            .collect(),
                        Err(e) => return (502, format!("{{ \"error\": \"{}\" }}", json_escape(&e))),
                    }
                }
                None => Vec::new(),
            };
            return match build_tree(name, &edge_pairs, &rank_pairs, depth) {
                Ok(tree) => {
                    meta.result_count = 1;
                    (200, tree)
                }
                Err(e) if e.starts_with("cycle") => {
                    (409, format!("{{ \"error\": \"cycle\", \"detail\": \"{}\" }}", json_escape(&e)))
                }
                Err(e) => (502, format!("{{ \"error\": \"{}\" }}", json_escape(&e))),
            };
        }
        if parts.len() == 3 && parts[2] == "contains" {
            // DOWN containment, symmetric with /partof's UNION below: a node "contains"
            // its children via chorus:contains (domain→sub) OR chorus:hasDomain
            // (product→domain). Querying only `contains` left /products/:p/contains
            // empty though the hasDomain edges exist — the UP bind Kade's product-rooted
            // tree render needs (#3466). One predicate set, both directions mirror.
            // #3545 — three derivations of DOWN containment, model-faithful:
            //   chorus:contains  (domain→sub, explicit)
            //   chorus:hasDomain (product→domain, the UP bind for product-rooted trees, #3466)
            //   inStream-INVERSE (a stream contains the steps whose chorus:inStream points at it)
            // The inverse is THE move that retires hand-added chorus:contains edges: containment
            // derives from the shape's declared edge (sh:inversePath chorus:inStream), one source of truth.
            let q = format!(
                "SELECT DISTINCT ?v WHERE {{ GRAPH <{g}> {{ {{ <{ns}{n}> <{ns}contains> ?o }} UNION {{ <{ns}{n}> <{ns}hasDomain> ?o }} UNION {{ ?o <{ns}inStream> <{ns}{n}> }} }} BIND(STR(?o) AS ?v) }}",
                g = table.instances_graph, ns = NS, n = name
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
                g = table.instances_graph, ns = NS, n = name
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
                g = table.instances_graph, ns = NS, n = name
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
                g = table.instances_graph, ns = NS, n = name
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
        return match entity_json(&table.class, name, &table.exposure, authed, &table.instances_graph) {
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
                    &table.instances_graph,
                    !table.secured.is_empty(), &data, &links, None, &table.model_version,
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
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>OWL API — generated {cs} API</title>\n<!-- GENERATED by athena-make openapi_html (#3453). Static shell + client fetch — the spec is the live /openapi.json projection of the model; never hand-edit. -->\n<style>body{{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;margin:2rem;max-width:64rem}}h1{{font-size:1.2rem}}pre{{background:#f6f8fa;padding:1rem;border-radius:6px;overflow:auto;white-space:pre-wrap}}a{{color:#0969da}}</style>\n</head>\n<body>\n<h1>OWL API — generated {cs} API</h1>\n<p>Self-documenting. This spec is generated from the model on every request — <a href=\"/openapi.json\">/openapi.json</a> (raw).</p>\n<pre id=\"spec\">loading /openapi.json …</pre>\n<script>\nfetch('/openapi.json').then(function(r){{return r.json()}}).then(function(s){{document.getElementById('spec').textContent=JSON.stringify(s,null,2)}}).catch(function(e){{document.getElementById('spec').textContent='failed to load /openapi.json: '+e}});\n</script>\n</body>\n</html>\n",
        cs = class_short
    )
}

/// #3466 — pick the RouteTable whose class owns this request's path resource.
/// `/products/loom` → resource "products" → the Product table; `/schema/product`
/// → "product" → the Product table; `/health` is handled server-level upstream.
/// None = no class owns the resource (typed 404). Multi-class serve dispatches
/// through this so one server fronts every generated API on one origin.
/// #3845 — what the dispatcher should do with a path, decided in pure code.
///
/// WHY this exists as its own function: the decision used to live inline in the
/// accept loop, where nothing could test it — and it was wrong for two months.
/// `/effective/:node/:key` is a CROSS-CLASS route; select_table matches a path's
/// first segment against a class plural, "effective" matches none, so the
/// dispatcher answered "unknown route" before reaching the handler that had been
/// compiled in since #3435 (2026-06-17). A routing bug that read exactly like a
/// missing feature, and that two of us first mis-diagnosed as a stale deploy.
#[derive(Debug, PartialEq)]
pub enum Dispatch {
    /// Serve with this table (index into the slice).
    Table(usize),
    /// An /effective request with no Property table mounted — distinct from
    /// "unknown route", because the path is RIGHT and the model is missing.
    /// Collapsing the two would send the next reader hunting a typo.
    EffectiveUnavailable,
    /// Genuinely unknown.
    NotFound,
}

pub fn dispatch_for(path: &str, tables: &[RouteTable]) -> Dispatch {
    let bare = path.split('?').next().unwrap_or("").trim_start_matches('/');
    if bare.starts_with("effective/") {
        return match tables
            .iter()
            .position(|t| t.class.rsplit('#').next().unwrap_or("").eq_ignore_ascii_case("Property"))
        {
            // Property's table, deliberately — the effective fetch reads
            // table.instances_graph, and the rows being resolved ARE Property
            // individuals. Falling back to the first table would silently read
            // the wrong graph, which is the failure this route exists to stop.
            Some(i) => Dispatch::Table(i),
            None => Dispatch::EffectiveUnavailable,
        };
    }
    match select_table(path, tables) {
        // Kade (#3845 review): unwrap_or(0) here would silently fall back to the
        // FIRST table on an invariant violation — and "first table" means a
        // different instances_graph, so the request would succeed against the
        // wrong graph and return a confident wrong answer. select_table returns a
        // reference INTO this slice, so a missing position is impossible; if it
        // ever happens the invariant is broken and we want to know immediately,
        // not to serve someone else's data.
        Some(t) => match tables.iter().position(|x| std::ptr::eq(x, t)) {
            Some(i) => Dispatch::Table(i),
            None => unreachable!("select_table returned a table outside the slice it was given"),
        },
        None => Dispatch::NotFound,
    }
}

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

/// #3609 — read a full HTTP request: headers to CRLFCRLF, then the body until
/// Content-Length is satisfied, capped at `max_body + 1` bytes of body (one
/// past the cap so an oversize body is DETECTABLE and 422s with the cap
/// message downstream, instead of truncating silently at the old single-4KB
/// read — which made every >4KB /batch body fail 4-field validation).
/// Never hangs: the caller sets a read timeout; EOF/timeout returns what
/// arrived. Generic over Read so the loop is unit-tested with chunked mocks.
pub fn read_http_request<Rd: std::io::Read>(r: &mut Rd, max_body: usize) -> String {
    let mut data: Vec<u8> = Vec::with_capacity(8192);
    let mut buf = [0u8; 4096];
    // 1) headers — read until the blank line; bound headers at 64KB (flood guard)
    let header_end = loop {
        match r.read(&mut buf) {
            Ok(0) => break None,
            Ok(n) => {
                data.extend_from_slice(&buf[..n]);
                if let Some(pos) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                    break Some(pos + 4);
                }
                if data.len() > 64 * 1024 {
                    break None;
                }
            }
            Err(_) => break None,
        }
    };
    let Some(hend) = header_end else {
        return String::from_utf8_lossy(&data).into_owned();
    };
    // 2) body — honor Content-Length, capped one past max_body (oversize detectable)
    let head_lower = String::from_utf8_lossy(&data[..hend]).to_ascii_lowercase();
    let content_length: usize = head_lower
        .lines()
        .find(|l| l.starts_with("content-length:"))
        .and_then(|l| l.splitn(2, ':').nth(1))
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0);
    let want = content_length.min(max_body + 1);
    while data.len() < hend + want {
        match r.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => data.extend_from_slice(&buf[..n]),
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&data).into_owned()
}

/// #4004 — the bind host, loopback unless an operator says otherwise. Jeff's
/// 2026-08-25 phone photo showed the ontology ER page rendering its frame and
/// then "Failed to load": the PAGE is served LAN-wide on :3340 while the data it
/// fetches was pinned to 127.0.0.1:3360, so it works on the laptop and is broken
/// on every other device. Same shape as #3965's bind-scope gap.
///
/// The default stays loopback deliberately — this surface is read-only but
/// unauthenticated, and widening it is a reachability change, not a config tweak
/// (the same rule the share-guard allowlist holds itself to). An operator who
/// wants it on the LAN says so explicitly, and the value is auditable.
pub fn bind_host() -> String {
    std::env::var("ATHENA_MAKE_BIND").ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "127.0.0.1".to_string())
}

pub fn serve(port: u16, tables: &[RouteTable]) -> R<()> {
    let host = bind_host();
    let listener = TcpListener::bind((host.as_str(), port)).map_err(|e| format!("bind {}:{}: {}", host, port, e))?;
    let classes: Vec<&str> = tables.iter().map(|t| t.class.rsplit('#').next().unwrap_or("")).collect();
    eprintln!("athena-make: serving {} generated API(s) on :{} [{}] (read-only; writes go through athena-model)", tables.len(), port, classes.join(", "));
    let mut req_counter: u64 = 0;
    // #3402→#3719 — the HS256 KeyRegistry/shared-secret seam config that loaded
    // here was DELETED with the machinery: the ES256/CSS verifier below is the
    // ONE verify path (identity from the token, scope from the model).
    // #3613 / ADR-052 — the ES256 (Solid-OIDC/CSS) verifier, built ONCE at boot:
    // issuer from env (deployment config, not per-principal data — §5), the
    // Principal allow-set resolved from the model in ONE boot query (§5; empty ⇒
    // fail-closed),
    // JWKS fetched via curl with a kid-keyed cache (§2). Warm-fetch warns loudly
    // on CSS-down-at-boot but never blocks boot.
    let css_issuer = std::env::var("CSS_ISSUER").unwrap_or_else(|_| "http://localhost:3001/".to_string());
    let jwks_url = format!("{}/.oidc/jwks", css_issuer.trim_end_matches('/'));
    let oidc_verifier = oidc::OidcVerifier::new(
        &css_issuer,
        // allow-set resolver: re-run lazily on the ALLOW_TTL cadence so a model
        // revocation propagates within one token TTL (no restart, no per-request call)
        || oidc::resolve_principal_webids(|q| sparql_json(q).ok()),
        // #3688 / ADR-054 §3.3 — the role resolver over chorus:holdsRole, on the
        // same cadence: a role REASSIGNMENT is a model edit and lands within one TTL.
        || oidc::resolve_principal_roles(|q| sparql_json(q).ok()),
        // #3689 — the scope resolver over chorus:hasScope: what a Principal may
        // WRITE is model data too. ES256 tokens carry no scope claim (CSS cannot
        // mint one — spiked 2026-07-30); the door resolves grants from the graph
        // and feeds the SAME scope_allows check the HS256 claims fed.
        || oidc::resolve_principal_scopes(|q| sparql_json(q).ok()),
        move || {
        let out = Command::new("curl")
            .args(["-sf", "--max-time", "3", &jwks_url])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
        },
    );
    let boot_now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let warmed_allow = oidc_verifier.warm_allow(boot_now);
    if warmed_allow == 0 {
        // #3785 — name the graph in the WARNING too. On 2026-08-06 this line
        // fired and told nobody which graph came up empty, which is the whole
        // difference between "the store is down" and "you are reading the wrong
        // place" — two states this message could not distinguish.
        eprintln!("athena-make: WARNING — {} — ES256 tokens are refused (fail-closed) until the TTL'd re-resolve finds Principals. HS256 legacy path unaffected.",
            oidc::graph_provenance(0, "EMPTY: no chorus:Principal there, or Fuseki unreachable"));
    } else {
        eprintln!("athena-make: {}", oidc::graph_provenance(warmed_allow, "boot warm, model-resolved, TTL'd"));
    }
    // #3688 — the role map. EMPTY is loud: every verified caller would carry no
    // role, and ownedBy authZ would refuse every write (fail-closed, not silent).
    let warmed_roles = oidc_verifier.warm_roles(boot_now);
    if warmed_roles == 0 {
        eprintln!("athena-make: WARNING — holdsRole map is EMPTY (no chorus:holdsRole in <{}>, or Fuseki unreachable); verified callers carry NO role, so ownedBy authZ refuses every write until the TTL'd re-resolve finds edges.",
            oidc::allow_set_graph());
    } else {
        eprintln!("athena-make: role map = {} holdsRole edge(s) (model-resolved, TTL'd)", warmed_roles);
    }
    let warmed = oidc_verifier.warm_fetch(boot_now);
    if warmed == 0 {
        eprintln!("athena-make: WARNING — JWKS warm-fetch got 0 keys from {} (CSS down or no keys); ES256 verifies fail-closed until a kid-triggered refetch succeeds.", css_issuer);
    } else {
        eprintln!("athena-make: JWKS warm-fetch cached {} key(s) from {}", warmed, css_issuer);
    }
    // #3494 — composed domain surfaces (the definesVocabulary fan-out): every domain
    // with chorus:repoTarget + definesVocabulary mounts at /<repoTarget>, composing
    // its vocab classes (whose RouteTables are already in `tables` via the serve
    // fan-out) as sub-resources. Read once at boot; empty → no composed routes.
    let surfaces = read_domain_surfaces().unwrap_or_default();
    for s in &surfaces {
        eprintln!("athena-make: + /{} domain surface [{}]", s.mount, s.classes.join(", "));
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
        // #3609 — bounded read timeout so a client that stalls mid-body can never
        // hang the single-threaded serve loop; read_http_request returns whatever
        // arrived and downstream validation 422s a truncated batch.
        let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
        let req_string = read_http_request(&mut stream, MAX_WRITE_BYTES);
        let req = req_string.as_str();
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
            let resp = http_response_ct(status_line(200), "{ \"ok\": true, \"service\": \"athena-make\" }", "application/json");
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
                "{{ \"apiVersion\": \"{}\", \"service\": \"athena-make\", \"kind\": \"Discovery\", \"count\": {}, \"primitives\": [{}] }}",
                API_VERSION, tables.len(), prims.join(", ")
            );
            let resp = http_response_ct(status_line(200), &doc, "application/json");
            let _ = stream.write_all(resp.as_bytes());
            continue;
        }
        // #3706 — BULK SCHEMA: GET /schema returns every shaped class's schema in
        // ONE document (kind, fields, mandatory, modelVersion). The per-class
        // /schema/<class> route stays; this exists because a consumer that wants
        // the WHOLE model — the model view — otherwise pays one round-trip per
        // class. The class view was making 42 requests to draw one screen, which
        // is fine on loopback and hangs over wifi through the /owl proxy. Same
        // projection, same source, one response. No new data, no second
        // implementation — it reads the identical RouteTables the per-class route
        // reads, so the two can never disagree.
        // #3723 — GET /reconcile: where each class lives and whether it serves.
        // Four independent sources (repo, deploy script, shapes, store); the
        // DISAGREEMENT between them is the output. See reconcile.rs for the
        // partition rule and Silas's ADR-051 x 025 ruling that shapes it.
        if path == "/reconcile" {
            let doc = crate::reconcile::reconcile_json(tables);
            let resp = http_response_ct(status_line(200), &doc, "application/json");
            let _ = stream.write_all(resp.as_bytes());
            continue;
        }
        if path == "/schema" {
            let doc = schema_set_json(tables);
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
        // #3573 — /batch is a CROSS-CLASS governed write (owned by no single class
        // table), so it's handled here, before table-selection. Same gate as per-class
        // writes: Bearer required (else 401), x-target-graph must be in the token scope
        // (else 403). Delegates to handle_batch → the typed-slot chorus-model batch op.
        if method == "POST" && path == "/batch" {
            let now_secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let auth_hdr = header("authorization");
            let token = auth_hdr
                .strip_prefix("Bearer ")
                .or_else(|| auth_hdr.strip_prefix("bearer "))
                .unwrap_or(&auth_hdr);
            let (code, body) = match oidc::verify_any(token, &oidc_verifier, now_secs) {
                Err(_) => (
                    401u16,
                    "{ \"error\": \"authn-missing\", \"message\": \"a valid Bearer service-token is required for a batch write\" }".to_string(),
                ),
                Ok(claims) => {
                    let target_graph = header("x-target-graph");
                    // #3573 Wren gate — /batch REQUIRES a non-empty scope claim. Unlike
                    // entity writes (which allow legacy/unscoped mixed-state), batch is the
                    // most destructive op; a legacy allow-all token has no business here.
                    if claims.scope.is_empty() || !scope_allows(&target_graph, &claims.scope) {
                        (
                            403u16,
                            format!("{{ \"error\": \"out-of-scope\", \"message\": \"batch requires a scoped token whose scope names target graph '{}'\" }}", json_escape(&target_graph)),
                        )
                    } else if !resolved_write_role(&claims.agent_id) {
                        (
                            403u16,
                            "{ \"error\": \"authz-role\", \"message\": \"batch writes require a model-resolved role\" }".to_string(),
                        )
                    } else {
                        let role = claims.agent_id.clone();
                        let body_str = req.splitn(2, "\r\n\r\n").nth(1).unwrap_or("");
                        handle_batch(&target_graph, body_str, &role, token)
                    }
                }
            };
            let resp = http_response_ct(status_line(code), &body, "application/json");
            let _ = stream.write_all(resp.as_bytes());
            continue;
        }
        // #3845 — dispatch decided in pure code (dispatch_for), because this
        // decision lived inline here and was wrong for two months with nothing
        // able to test it.
        let table = match dispatch_for(&path, tables) {
            Dispatch::Table(i) => &tables[i],
            Dispatch::EffectiveUnavailable => {
                let nf = "{ \"error\": \"effective-unavailable\", \"message\": \"no Property route table is mounted, so no effective-config graph can be resolved\" }";
                let resp = http_response_ct(status_line(503), nf, "application/json");
                let _ = stream.write_all(resp.as_bytes());
                continue;
            }
            Dispatch::NotFound => {
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
            match oidc::verify_any(token, &oidc_verifier, now_secs) {
                Err(_) => {
                    let (c, t) = write_status("authn-missing");
                    ((c, format!("{{ \"error\": \"{}\", \"message\": \"a valid Bearer service-token is required for writes\" }}", t)),
                     ReqMeta { route: "write-authn".into(), ..Default::default() })
                }
                Ok(claims) => {
                    // #3573 Part C — SCOPE enforcement (the realm-isolation control,
                    // Silas's invariant): the verified principal may write ONLY graphs
                    // named by its model-resolved grants. The presented ES256 token does
                    // not need a scope claim; verify_any resolves Principal hasScope data
                    // into Claims.scope. Empty resolved scope is deny-all, fail closed.
                    let target_graph = header("x-target-graph");
                    // The selected class table owns the graph the DAL will
                    // actually mutate. A caller-supplied header may confirm that
                    // graph, but can never substitute a different in-scope graph
                    // as an authorization decoy for the real write target.
                    let effective_target = table.instances_graph.as_str();
                    if !resolved_write_role(&claims.agent_id) {
                        ((403u16, "{ \"error\": \"authz-role\", \"message\": \"writes require a model-resolved role\" }".to_string()),
                         ReqMeta { route: "write-authz-role".into(), ..Default::default() })
                    } else if !target_graph.is_empty() && target_graph != effective_target {
                        ((403u16, format!("{{ \"error\": \"out-of-scope\", \"message\": \"x-target-graph '{}' does not match this class's write graph '{}'\" }}", json_escape(&target_graph), json_escape(effective_target))),
                         ReqMeta { route: "write-authz-graph-mismatch".into(), ..Default::default() })
                    } else if !scope_allows(effective_target, &claims.scope) && !row_owner_governed(&table.fields) {
                        // #4096 — an owned class is governed by the row's owner (checked in
                        // handle_write); only an ownerless class needs the graph in scope.
                        ((403u16, format!("{{ \"error\": \"out-of-scope\", \"message\": \"target graph '{}' is not in this token's scope and this class carries no row owner (#3573/#3689, #4096)\" }}", json_escape(effective_target))),
                         ReqMeta { route: "write-authz-scope".into(), ..Default::default() })
                    } else {
                        let role = claims.agent_id.clone();
                        let body_str = req.splitn(2, "\r\n\r\n").nth(1).unwrap_or("");
                        // (POST /batch is handled by the cross-class pre-table block above,
                        // which `continue`s — it can't reach here. One site, no drift.)
                        // #4101 — the land names the commit that is changing the row
                        let landed_commit = header("x-landed-commit");
                        let (c, b) = handle_write_stamped(&method, &path, body_str, table, &role, token, &landed_commit);
                        ((c, b), ReqMeta { route: format!("write:{}", method.to_ascii_lowercase()), ..Default::default() })
                    }
                }
            }
        } else {
            match oidc::seam_auth_any(&path, &header("authorization"), &oidc_verifier, now_secs, &table.secured) {
                Some((c, b)) => ((c, b), ReqMeta { route: "auth-refused".into(), ..Default::default() }),
                None => {
                    let hp = if query.is_empty() { path.clone() } else { format!("{}?{}", path, query) };
                    // #3506 / ADR-048 §3 — authed = a valid service token is present; it
                    // gates `internal`-exposure fields on an exposure-enforced shape.
                    let ah = header("authorization");
                    let tok = ah.strip_prefix("Bearer ").or_else(|| ah.strip_prefix("bearer ")).unwrap_or(&ah);
                    let authed = oidc::verify_any(tok, &oidc_verifier, now_secs).is_ok();
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

    // #3551 — the Generation-Gap drift gate. The trait seam is projected from the
    // verb's typed I/O; changing the model I/O re-projects a DIFFERENT signature, so a
    // stale hand-impl fails to compile. This pins the projection + the gate property.
    #[test]
    fn verb_trait_seam_projects_from_io_and_drifts_on_change() {
        let inputs = vec![
            ("card".to_string(), "datatype:integer".to_string()),
            ("role".to_string(), "datatype:string".to_string()),
        ];
        let outputs = vec![("landed".to_string(), "datatype:boolean".to_string())];
        let sig = verb_trait_signature("athena-deploy", &inputs, &outputs);
        // projected from the model: PascalCase trait, typed args, typed R<_> return
        assert!(sig.contains("pub trait AthenaDeployLogic"), "PascalCase trait stem: {sig}");
        assert!(
            sig.contains("fn run(&self, card: i64, role: String) -> R<bool>"),
            "signature projected from typed I/O: {sig}"
        );
        // DRIFT GATE: drop an input in the model → the re-projected signature differs,
        // so the old hand-impl no longer matches the trait and won't compile.
        let changed = verb_trait_signature("athena-deploy", &inputs[..1], &outputs);
        assert_ne!(sig, changed, "a model I/O change must re-project a different seam (the gate)");
        // no-input / no-output verbs project the degenerate but valid forms
        let none = verb_trait_signature("borg-tend", &[], &[]);
        assert!(none.contains("fn run(&self) -> R<()>"), "empty I/O → run(&self)->R<()>: {none}");
    }

    #[test]
    fn to_pascal_and_rust_type_map_shape_tokens() {
        assert_eq!(to_pascal("athena-deploy"), "AthenaDeploy");
        assert_eq!(to_pascal("werk_pull"), "WerkPull");
        assert_eq!(rust_type("datatype:integer"), "i64");
        assert_eq!(rust_type("datatype:boolean"), "bool");
        assert_eq!(rust_type("edge:ValueStream"), "String"); // IRI carried as String
    }

    // #3551 — the emitter projects the full generated half of a verb crate, and is a
    // PURE function of the VerbTable (re-run safety: it never reads <verb>.rs).
    fn athena_deploy_table() -> VerbTable {
        VerbTable {
            verb: "athena-deploy".into(),
            family: "athena".into(),
            invocability: "invoked".into(),
            inputs: vec![
                ("domain".into(), "datatype:string".into()),
                ("ttl".into(), "datatype:string".into()),
            ],
            outputs: vec![("ok".into(), "datatype:boolean".into())],
            edges: vec![
                ("inStream".into(), "value-stream-athena".into()),
                ("partOf".into(), "product-athena".into()),
                ("operatesOn".into(), "deploys".into()),
            ],
            repo_target: "platform/services/athena-deploy/src".into(),
        }
    }

    #[test]
    fn verb_skeleton_emits_all_generation_gap_pieces() {
        let g = verb_skeleton_rust(&athena_deploy_table());
        // the seam (the drift gate) + typed input + arg-parse
        assert!(g.contains("pub trait AthenaDeployLogic"), "trait seam: {g}");
        assert!(g.contains("fn run(&self, domain: String, ttl: String) -> R<bool>"), "seam sig from I/O");
        assert!(g.contains("pub struct AthenaDeployInput"), "typed input struct");
        assert!(g.contains("pub fn parse_args(argv: &[String]) -> R<AthenaDeployInput>"), "arg-parse");
        // wire-before-make: the model edges projected as registration data
        assert!(g.contains("pub const WIRING"), "edge-wiring const");
        assert!(g.contains("(\"inStream\", \"value-stream-athena\")"), "vs-step edge wired");
        assert!(g.contains("(\"partOf\", \"product-athena\")"), "product edge wired");
        // the woven aspect scaffold (trace in / emit out) around the handwritten run()
        assert!(g.contains("pub fn dispatch<L: AthenaDeployLogic>"), "dispatch entrypoint");
        assert!(g.contains("[trace] athena-deploy start"), "trace aspect woven");
        assert!(g.contains("[emit] athena-deploy done"), "emit aspect woven");
        assert!(g.contains("logic.run(input.domain, input.ttl)"), "calls handwritten logic via seam");
        assert!(g.contains("DO NOT EDIT"), "generated-half marked athena-make-owned");
    }

    #[test]
    fn verb_skeleton_is_deterministic_rerun_safe() {
        // Re-run safety: the generated half is a pure projection of the VerbTable —
        // regenerating produces byte-identical output and never consults <verb>.rs.
        let t = athena_deploy_table();
        assert_eq!(verb_skeleton_rust(&t), verb_skeleton_rust(&t), "regeneration is deterministic");
    }

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
            "urn:chorus:instances",
            true,
            "{ \"purpose\": \"config-as-data\" }",
            "{ \"partOf\": \"/v1/products/borg\" }",
            None,
            "v2",
        );
        for needle in [
            "\"apiVersion\": \"v1\"",
            "\"modelVersion\": \"v2\"",   // #3704 — every envelope carries the class version (born-v2 default)
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

    // #3675 — collection projection honors the exposure gate (the /services leak).
    #[test]
    fn collection_projection_gated_by_exposure() {
        let fields = vec![
            ("overview".to_string(), false),
            ("implementationPlan".to_string(), false),
            ("asIs".to_string(), false),
            ("hasDesignDoc".to_string(), true),
        ];
        let exposure = vec![
            ("label".to_string(), "public".to_string()),
            ("overview".to_string(), "internal".to_string()),
            ("implementationPlan".to_string(), "secret".to_string()),
        ];
        // unauth on an annotated shape: everything non-public drops, incl. unmarked.
        let unauth = exposed_projection(fields.clone(), &exposure, false);
        assert!(unauth.is_empty(), "unauth must see no extra fields here: {:?}", unauth);
        // authed: internal shows; secret and unmarked stay hidden.
        let authed: Vec<String> =
            exposed_projection(fields.clone(), &exposure, true).into_iter().map(|(n, _)| n).collect();
        assert_eq!(authed, vec!["overview".to_string()], "authed = internal only: {:?}", authed);
        // un-annotated shape (opt-in): projection passes through untouched.
        let open = exposed_projection(fields.clone(), &[], false);
        assert_eq!(open.len(), fields.len(), "no annotations → fully open (migration-safe)");
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

    // #3635 — select_v decodes Fuseki's JSON escapes; pages were rendering the
    // escape text (an em dash arrived as backslash-u-2014 and json_escape doubled
    // the backslash). Also: closing-quote scan honors escaped quotes.
    #[test]
    fn select_v_decodes_json_escapes_from_fuseki() {
        let body = r#"{"results":{"bindings":[
            {"v":{"value":"dash \u2014 here"}},
            {"v":{"value":"quote \" inside"}},
            {"v":{"value":"emoji \ud83d\ude00 pair"}}
        ]}}"#;
        let vals = select_v(body);
        assert_eq!(vals[0], "dash \u{2014} here", "\\uXXXX decodes to the real char");
        assert_eq!(vals[1], "quote \" inside", "escaped quote doesn't truncate the value");
        assert_eq!(vals[2], "emoji \u{1F600} pair", "surrogate pairs decode");
    }

    #[test]
    fn json_escape_roundtrips_a_decoded_em_dash() {
        // end-to-end contract: graph literal — → Fuseki — → select_v — →
        // json_escape leaves the multibyte char alone → page shows —
        assert_eq!(json_escape("a \u{2014} b"), "a \u{2014} b");
        assert_eq!(json_escape("a\tb\u{0008}c\u{0000}"), "a\\tb\\bc\\u0000");
    }

    // #3635 — collection marshal aggregates multi-valued fields per subject.
    // #4045 — a value that CONTAINS a pipe must not shift the columns after it.
    // Negative proof: on the old '|'-separated rows this same fixture split into
    // six columns and hasDomain served "Today" — the test fails on pre-fix code.
    #[test]
    fn collection_items_value_containing_a_pipe_does_not_shift_columns() {
        let sep = super::COL_SEP;
        let rows = vec![format!(
            "https://x#spine{s}Spine{s}operating{s}Path | Today | To-be{s}events",
            s = sep
        )];
        let items = collection_items(rows, &["apiSurface".to_string(), "hasDomain".to_string()]);
        assert_eq!(items.len(), 1);
        assert!(items[0].contains("\"apiSurface\": \"Path | Today | To-be\""), "pipe kept inside the value: {}", items[0]);
        assert!(items[0].contains("\"hasDomain\": \"events\""), "the column after it is intact: {}", items[0]);
    }

    // Pinned fixture for the #3558 fan-out: borg's 5 hasDomain rows must yield ONE
    // item carrying ALL values (the dedupe kept the first row and dropped the rest).
    #[test]
    fn collection_items_aggregates_multivalued_fields_per_subject() {
        let rows = vec![
            "https://x#borg\u{1f}Borg\u{1f}building\u{1f}logs".to_string(),
            "https://x#borg\u{1f}Borg\u{1f}building\u{1f}builds".to_string(),
            "https://x#borg\u{1f}Borg\u{1f}building\u{1f}deploys".to_string(),
            "https://x#athena\u{1f}\u{1f}building\u{1f}domains".to_string(),
        ];
        let items = collection_items(rows, &["hasDomain".to_string()]);
        assert_eq!(items.len(), 2, "entities, not SPARQL rows");
        assert!(
            items[0].contains("\"hasDomain\": [\"logs\", \"builds\", \"deploys\"]"),
            "multi-valued renders as array, order-preserving: {}",
            items[0]
        );
        assert!(items[0].contains("\"name\": \"borg\""));
        assert!(
            items[1].contains("\"hasDomain\": \"domains\""),
            "single-valued keeps the prior string shape (ADR-047 additive): {}",
            items[1]
        );
    }

    #[test]
    fn collection_items_cross_product_rows_dedupe_values() {
        // two multi-valued fields fan as a cross-product; values dedupe per field
        let rows = vec![
            "https://x#d\u{1f}D\u{1f}ok\u{1f}a\u{1f}v1".to_string(),
            "https://x#d\u{1f}D\u{1f}ok\u{1f}a\u{1f}v2".to_string(),
            "https://x#d\u{1f}D\u{1f}ok\u{1f}b\u{1f}v1".to_string(),
            "https://x#d\u{1f}D\u{1f}ok\u{1f}b\u{1f}v2".to_string(),
        ];
        let items = collection_items(rows, &["f1".to_string(), "f2".to_string()]);
        assert_eq!(items.len(), 1);
        assert!(items[0].contains("\"f1\": [\"a\", \"b\"]"), "{}", items[0]);
        assert!(items[0].contains("\"f2\": [\"v1\", \"v2\"]"), "{}", items[0]);
    }

    /// #4022 — the page is two queries: subjects first, then a projection whose
    /// VALUES sits INSIDE the GRAPH block. The old one-query shape is the negative
    /// fixture: it fails the same assertions.
    #[test]
    fn collection_page_is_subjects_then_values_bound_projection() {
        let body = "GRAPH <urn:g> { ?s a <urn:C> . OPTIONAL { ?s <urn:p> ?x } BIND(CONCAT(STR(?s), \"|\", COALESCE(?x, \"\")) AS ?v) }";
        let page = collection_page_query("urn:g", "urn:C", 20, 40);
        assert_eq!(page, "SELECT (STR(?s) AS ?v) WHERE { GRAPH <urn:g> { ?s a <urn:C> } } ORDER BY ?s LIMIT 20 OFFSET 40");
        assert!(!page.contains("OPTIONAL") && !page.contains("CONCAT"), "the page query carries no projection: {}", page);
        let subs = vec!["urn:a".to_string(), "urn:b".to_string(), "urn:evil> } <urn:x".to_string()];
        let proj = collection_project_query("urn:g", body, &subs);
        let values_at = proj.find("VALUES ?s { <urn:a> <urn:b> }").expect("VALUES bound to the page");
        assert!(values_at > proj.find("GRAPH <urn:g> {").unwrap(), "VALUES is INSIDE the graph block: {}", proj);
        assert!(values_at < proj.find("OPTIONAL").unwrap(), "VALUES precedes every OPTIONAL: {}", proj);
        assert!(!proj.contains("urn:evil"), "a subject carrying IRI/SPARQL metachars is dropped, never interpolated");
        assert!(proj.ends_with("ORDER BY ?s") && !proj.contains("LIMIT"), "page order by subject, no second LIMIT: {}", proj);
        // NEGATIVE PROOF — the #4010 shape this replaces fails the same checks.
        let old = format!("SELECT ?v WHERE {{ {} }} ORDER BY ?v LIMIT 20 OFFSET 40", body);
        assert!(!old.contains("VALUES ?s"), "old shape has no bound page");
        assert!(old.contains("ORDER BY ?v") && old.contains("OPTIONAL"), "old shape sorts the projection over the whole collection");
    }

    #[test]
    fn collection_items_empty_field_renders_empty_string() {
        let rows = vec!["https://x#d\u{1f}D\u{1f}ok\u{1f}".to_string()];
        let items = collection_items(rows, &["f1".to_string()]);
        assert!(items[0].contains("\"f1\": \"\""), "{}", items[0]);
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

    // #3561 — the advertised path MUST be the served path. Negative proof for the
    // deploy-served gate: this test compares the two states the gate exists to
    // separate — a route the discovery document names, and a route the router
    // answers. Before the fix, `/v1/domains` and `/domains` gave DIFFERENT status
    // codes from the same table, which is the whole defect.
    #[test]
    fn versioned_and_bare_paths_resolve_identically() {
        let t = RouteTable {
            class: format!("{}Domain", NS),
            fields: vec!["label|plain".into()],
            routes: vec!["GET /domains".into()],
            secured: vec![],
            mandatory: vec![],
            write_required: vec![],
            repo_target: String::new(),
            exposure: vec![],
            instances_graph: INSTANCES_GRAPH.to_string(),
            tree_edges: vec![],
            tree_order: None,
            model_version: "unclassified".to_string(),
        };
        let (bare_code, _) = handle("/domains", &t);
        let (v1_code, _) = handle("/v1/domains", &t);
        assert_eq!(
            v1_code, bare_code,
            "the discovery document advertises /v1/domains; serving it differently from \
             /domains is DECLARED != SERVED inside one binary (#3561)"
        );
        // and a genuinely unknown route must still 404 — otherwise this check could
        // not tell "prefix handled" from "everything accepted".
        let (unknown_code, _) = handle("/v1/definitely-not-a-route", &t);
        assert_ne!(
            unknown_code, bare_code,
            "stripping /v1 must not turn unknown routes into hits"
        );
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
            "2026-06-19", "534805b9", "urn:chorus:instances", false, "[]", "{}", Some(35), "v2",
        );
        assert!(c.contains("\"count\": 35"), "collection carries count: {}", c);
        assert!(!c.contains("\"id\":"), "collection omits id: {}", c);
        assert!(c.contains("\"requiresAuth\": false"), "open collection: {}", c);
        assert!(c.contains("\"data\": []"), "collection data is the array: {}", c);
        // #3749 — the envelope names its true data source, distinct from the schema source
        assert!(c.contains("\"servedFrom\": \"urn:chorus:instances\""), "envelope carries servedFrom: {}", c);
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
            write_required: vec![],
            repo_target: String::new(),
            exposure: vec![],
            instances_graph: INSTANCES_GRAPH.to_string(), tree_edges: vec![], tree_order: None, model_version: "unclassified".to_string(),
        };
        let h = page_html(&t);
        // projection doctrine — the generated marker says regenerate, never hand-edit
        assert!(h.contains("GENERATED by athena-make"), "must carry the generated marker");
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
            write_required: vec![],
            repo_target: String::new(),
            exposure: vec![],
            instances_graph: INSTANCES_GRAPH.to_string(), tree_edges: vec![], tree_order: None, model_version: "unclassified".to_string(),
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
            write_required: vec!["label".into()],
            repo_target: String::new(),
            exposure: vec![],
            instances_graph: INSTANCES_GRAPH.to_string(),
            routes: vec![
                "GET /domains".into(),
                "GET /domains/:name".into(),
                "GET /domains/:name/contains".into(),
                "GET /schema/domain".into(),
            ],
            secured: vec![],
            tree_edges: vec![],
            tree_order: None, model_version: "unclassified".to_string(),
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
    fn openapi_batch_create_advertises_json_array_contract() {
        let mut t = openapi_fixture();
        t.class = format!("{}TestResult", NS);
        t.fields.push("ofTest|edge:Test".into());
        t.write_required.push("ofTest".into());
        t.routes = vec!["POST /testresults/batch".into()];
        let body = openapi_json(&t);
        assert!(body.contains("\"/testresults/batch\""), "{}", body);
        assert!(body.contains("\"type\": \"array\", \"minItems\": 1"), "{}", body);
        assert!(body.contains("\"$ref\": \"#/components/schemas/TestResultCreate\""), "{}", body);
        assert!(body.contains("\"TestResultCreate\""), "{}", body);
        assert!(body.contains("\"name\": { \"type\": \"string\" }"), "{}", body);
        assert!(body.contains("\"ofTest\": { \"type\": \"string\" }"), "edge targets are scalar local names: {}", body);
        assert!(body.contains("\"required\": [\"name\", \"ofTest\"]"), "required edges match the DAL floor: {}", body);
        assert!(body.contains("\"additionalProperties\": false"), "closed runtime shape is documented: {}", body);
        assert!(body.contains("\"requestBody\": { \"required\": true"), "write bodies are required: {}", body);
        assert!(body.contains("\"502\": { \"description\": \"DAL unavailable or failed\" }"), "runtime DAL failure is documented: {}", body);
    }

    #[test]
    fn openapi_write_bodies_match_each_operation() {
        let mut t = openapi_fixture();
        t.fields.push("partOf|edge:Domain".into());
        t.write_required.push("comment".into());
        t.routes.extend([
            "POST /domains".into(),
            "PUT /domains/:name".into(),
            "DELETE /domains/:name".into(),
            "POST /domains/:name/partof".into(),
            "DELETE /domains/:name/partof".into(),
        ]);
        let body = openapi_json(&t);
        let entity_path = body
            .lines()
            .find(|line| line.trim_start().starts_with("\"/domains/{name}\":"))
            .expect("entity path");
        assert_eq!(entity_path.matches("\"requestBody\"").count(), 1, "DELETE has no body; PUT has one: {entity_path}");
        assert!(entity_path.contains("#/components/schemas/DomainReplace"), "PUT uses the replace schema: {entity_path}");
        assert!(entity_path.contains("\"delete\": { \"parameters\":"), "DELETE remains documented: {entity_path}");

        let edge_path = body
            .lines()
            .find(|line| line.trim_start().starts_with("\"/domains/{name}/partof\":"))
            .expect("edge path");
        assert_eq!(edge_path.matches("\"requestBody\"").count(), 2, "both edge mutations require a target: {edge_path}");
        assert_eq!(edge_path.matches("\"required\": [\"target\"]").count(), 2, "target is required for add and remove: {edge_path}");
        assert!(body.contains("\"DomainReplace\": { \"type\": \"object\", \"additionalProperties\": false"), "{body}");
        assert!(body.contains("\"required\": [\"comment\"]"), "replace schema carries the full-write floor: {body}");
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
        assert!(!r.contains(&"POST /domains/batch".to_string()), "batch is not exposed for unrelated classes");
        assert!(r.contains(&"PUT /domains/:name".to_string()), "replace entity");
        assert!(r.contains(&"DELETE /domains/:name".to_string()), "delete entity");
        // per-edge add/remove (mirrors the read edges)
        for edge in ["partof", "contains", "has-child"] {
            assert!(r.contains(&format!("POST /domains/:name/{}", edge)), "add {} edge", edge);
            assert!(r.contains(&format!("DELETE /domains/:name/{}", edge)), "remove {} edge", edge);
        }
        assert!(write_routes("testresults").contains(&"POST /testresults/batch".to_string()), "TestResult gets the Phase A batch route");
        // pluralization flows through (mirrors read-route generation)
        assert!(write_routes("properties").contains(&"POST /properties".to_string()));
    }

    // #3688 — `role_from_webid_extracts_role_or_none` is DELETED with the parser
    // it pinned. Its replacements live in chorus-oidc (role_comes_from_holds_role_
    // not_the_webid_string, opaque_webid_still_resolves_a_role,
    // principal_without_holds_role_carries_no_role, role_reassignment_lands_
    // within_one_ttl) — the role is asked of the graph, so there is no string
    // shape left to pin.

    #[test]
    fn kind_of_class_maps_camelcase_to_adr040_kebab() {
        // #3647 — the drill's third find: lowercasing ValueStreamStep produced
        // 'valuestreamstep', unknown to the DAL's kebab kind table.
        assert_eq!(kind_of_class("ValueStreamStep"), "value-stream-step");
        assert_eq!(kind_of_class("ValueStream"), "value-stream");
        assert_eq!(kind_of_class("Gate"), "gate");
        assert_eq!(kind_of_class("Domain"), "domain");
        assert_eq!(kind_of_class("Test"), "test");
        assert_eq!(kind_of_class("TestSuiteRun"), "test-suite-run");
    }

    #[test]
    fn parse_write_maps_method_and_shape() {
        assert_eq!(parse_write("POST", "/domains", "domains"), Some(WriteOp::CreateEntity));
        assert_eq!(parse_write("POST", "/domains/batch", "domains"), None);
        assert_eq!(parse_write("POST", "/testresults/batch", "testresults"), Some(WriteOp::CreateBatch));
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
        assert_eq!(normalize_owned_role("role-wren"), "wren");
        assert_eq!(normalize_owned_role("wren"), "wren");
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

    // ── #3680 — create-with-required-edge: edge-typed body props FORWARD, not drop ──
    #[test]
    fn collect_entity_edges_forwards_edge_typed_props_with_target_kind() {
        let fields = vec![
            "label|datatype:string".to_string(),
            "ofTest|edge:Test".to_string(),
            "partOf|edge:Domain".to_string(),
        ];
        let body = r#"{"name":"tr-1","label":"x","ofTest":"test-platform-api-a"}"#;
        let edges = collect_entity_edges(body, &fields);
        assert_eq!(edges, vec![("ofTest".to_string(), "test".to_string(), "test-platform-api-a".to_string())]);
    }

    #[test]
    fn collect_entity_edges_empty_when_no_edge_props_in_body() {
        let fields = vec!["label|datatype:string".to_string(), "ofTest|edge:Test".to_string()];
        assert!(collect_entity_edges(r#"{"name":"tr-1","label":"x"}"#, &fields).is_empty());
    }

    // (build_create_entity / build_replace_entity / sparql_lit tests retired with
    // their fns — writes delegate to the DAL, athena-make builds no raw SPARQL. #3468)

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
    fn add_batch_refusals_keep_conflicts_distinct_from_store_failures() {
        assert_eq!(
            dal_err_resp("add-batch: entity 'test-result:x': already-exists").0,
            409,
        );
        assert_eq!(
            dal_err_resp("add-batch: entity 'test-result:x': shape-violation: result").0,
            422,
        );
        assert_eq!(
            dal_err_resp("add-batch: commit failed for [test-result:x]: fuseki-update failed").0,
            502,
        );
        assert_eq!(dal_err_resp("batch: invalid typed slot").0, 422);
        assert_eq!(
            dal_err_resp("add-batch: entity 'test-result:test-result-x': double-prefix").0,
            422,
        );
        assert_eq!(
            dal_err_resp("add-batch: entity 'test-result:x': empty-name").0,
            422,
        );
        assert_eq!(
            dal_err_resp("add-batch: entity 'domain:x': shape-channel-violation: ownedBy").0,
            422,
        );
    }

    #[test]
    fn routes_json_is_deterministic() {
        let t = RouteTable {
            class: format!("{}Domain", NS),
            fields: vec!["comment".into(), "label".into()],
            routes: vec!["GET /domains".into()],
            secured: vec!["/schema/domain".into()],
            mandatory: vec!["label".into(), "comment".into()],
            write_required: vec!["label".into(), "comment".into()],
            repo_target: String::new(),
            exposure: vec![],
            instances_graph: INSTANCES_GRAPH.to_string(), tree_edges: vec![], tree_order: None, model_version: "unclassified".to_string(),
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
            write_required: vec!["label".into(), "comment".into()],
            repo_target: String::new(),
            exposure: vec![],
            instances_graph: INSTANCES_GRAPH.to_string(), tree_edges: vec![], tree_order: None, model_version: "unclassified".to_string(),
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
        let t = RouteTable { class: format!("{}Domain", NS), fields: vec![], routes: vec!["GET /domains".into()], secured: vec![], mandatory: vec![], write_required: vec![], repo_target: String::new(), exposure: vec![], instances_graph: INSTANCES_GRAPH.to_string(), tree_edges: vec![], tree_order: None, model_version: "unclassified".to_string() };
        let (code, body) = handle("/nope", &t);
        assert_eq!(code, 404);
        assert!(body.contains("GET /domains"));
    }

    // #3567 SPIKE — the negative test: scope is a REAL control only if a wrong-graph
    // write is REJECTED. These unit-pin the 403 deterministically (Silas's criterion).
    #[test]
    fn scope_allows_grants_write_to_in_scope_graph() {
        // the 200 case: token scoped to tests' instance graph, writing there
        assert!(scope_allows("urn:chorus:domains:tests", &["urn:chorus:domains:tests".to_string()]));
    }

    #[test]
    /// #4096 — an owned class is row-governed: the graph need not be in the token's
    /// scope (the owner check in handle_write governs). NEGATIVE PROOF: a class with
    /// no ownedBy field is still graph-governed, so the same out-of-scope write is refused.
    #[test]
    fn owned_classes_are_row_governed_and_ownerless_classes_stay_graph_governed() {
        let owned = vec!["label".to_string(), "ownedBy|edge:Role".to_string(), "statement".to_string()];
        let ownerless = vec!["label".to_string(), "trigger".to_string()];
        assert!(row_owner_governed(&owned));
        assert!(!row_owner_governed(&ownerless));
        let scope = vec!["urn:chorus:domains:security".to_string()];
        let target = "urn:chorus:domains:services";
        // the gate's expression, verbatim: refused only when out of scope AND ownerless
        assert!(!( !scope_allows(target, &scope) && !row_owner_governed(&owned) ), "owned class writes to a graph outside its scope");
        assert!(   !scope_allows(target, &scope) && !row_owner_governed(&ownerless),  "ownerless class is still refused");
    }

    /// #4096 — the read side mints the same IRI the DAL does: bare for Product,
    /// Domain, Test; kind-prefixed for everything else. NEGATIVE PROOF: a document
    /// resolved the bare way is a different IRI (the round-14 shape).
    #[test]
    fn entity_subject_mirrors_the_dal_mint_rule() {
        assert_eq!(entity_subject("https://jeffbridwell.com/chorus#Product", "spine"), format!("{}spine", NS));
        assert_eq!(entity_subject("chorus#Domain", "events"), format!("{}events", NS));
        assert_eq!(entity_subject("https://jeffbridwell.com/chorus#Document", "spine-product-design"), format!("{}document-spine-product-design", NS));
        assert_eq!(entity_subject("https://jeffbridwell.com/chorus#ValueStreamStep", "directing"), format!("{}value-stream-step-directing", NS));
        assert_ne!(entity_subject("https://jeffbridwell.com/chorus#Document", "x"), format!("{}x", NS));
    }

    #[test]
    fn scope_allows_rejects_write_to_out_of_scope_graph() {
        // the 403 case — THE #3564 control: same token, a DIFFERENT graph → denied.
        // If this passed, scope would be theater.
        assert!(!scope_allows("urn:chorus:domains:photos", &["urn:chorus:domains:tests".to_string()]));
    }

    #[test]
    fn scope_allows_empty_scope_denies_all_fail_closed() {
        assert!(!scope_allows("urn:chorus:domains:tests", &[]));
    }

    #[test]
    fn class_writes_require_a_model_resolved_role() {
        assert!(resolved_write_role("wren"));
        assert!(!resolved_write_role(""));
        assert!(!resolved_write_role("   "));
    }

    // #3567 SPIKE — the emit projects scope from the INSTANCE/write graph, never the
    // shape/ontology graph (Silas's scope-trap). And it carries the new claims.
    #[test]
    fn dal_emit_scopes_to_instance_graph_and_mints_scoped_token() {
        let t = RouteTable {
            class: format!("{}Test", NS),
            fields: vec!["filePath|datatype:string".into()],
            routes: vec!["PUT /tests".into()],
            secured: vec!["/schema/test".into()],
            mandatory: vec!["filePath".into(), "testName".into()],
            write_required: vec!["filePath".into(), "testName".into()],
            repo_target: String::new(),
            exposure: vec![],
            instances_graph: "urn:chorus:domains:tests".to_string(), tree_edges: vec![], tree_order: None, model_version: "unclassified".to_string(),
        };
        let scope = vec![t.instances_graph.clone()];
        let ts = dal_skeleton_ts(&t, &scope);
        // scope = the instance/write graph, NOT the ontology/shape graph — kept as
        // a projected DECLARATION of what the product should be granted (#3722).
        assert!(ts.contains("urn:chorus:domains:tests"));
        assert!(!ts.contains("urn:chorus:ontology"));
        // #3722 — the emitter mints an ES256 CSS IDENTITY token via
        // chorus-identity-token, NOT a self-signed HS256 token with a scope claim.
        assert!(ts.contains("chorus-identity-token"), "{}", ts);
        assert!(ts.contains("execFileSync"), "{}", ts);
        assert!(!ts.contains("createHmac"), "no HS256 minter may be emitted: {}", ts);
        assert!(!ts.contains("alg: \"HS256\""), "{}", ts);
        // SHACL floor projected from the shape; local-name symbol (not the full IRI)
        assert!(ts.contains("\"filePath\""));
        assert!(ts.contains("export async function writeTest("));
        assert!(!ts.contains("#Test")); // no raw IRI leaked into the symbol
    }
}

#[cfg(test)]
mod read_http_request_tests {
    use super::read_http_request;

    /// Mock reader delivering the request in fixed-size chunks — models a TCP
    /// stream where one read() never returns the whole body (the #3609 bug).
    struct Chunked {
        data: Vec<u8>,
        pos: usize,
        chunk: usize,
    }
    impl std::io::Read for Chunked {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.pos >= self.data.len() {
                return Ok(0);
            }
            let n = self.chunk.min(buf.len()).min(self.data.len() - self.pos);
            buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
            self.pos += n;
            Ok(n)
        }
    }

    fn req_with_body(body: &str) -> Vec<u8> {
        format!(
            "POST /batch HTTP/1.1\r\nContent-Length: {}\r\nx-target-graph: urn:chorus:t\r\n\r\n{}",
            body.len(),
            body
        )
        .into_bytes()
    }

    #[test]
    fn reads_a_58kb_body_to_content_length_across_many_chunks() {
        // the #3603 migration is 405 lines / ~58KB — the exact payload the old
        // single-4096-read truncated.
        let line = "INS\t<urn:chorus:s>\t<urn:chorus:p>\t<urn:chorus:o>\n";
        let body = line.repeat(58_000 / line.len() + 1);
        let mut r = Chunked { data: req_with_body(&body), pos: 0, chunk: 1024 };
        let req = read_http_request(&mut r, 65_536);
        let got_body = req.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or("");
        assert_eq!(got_body.len(), body.len(), "full body read to Content-Length");
        assert!(got_body.ends_with(line.trim_end_matches('\n')) || got_body.ends_with(line));
    }

    #[test]
    fn no_content_length_returns_headers_without_hanging() {
        let raw = b"GET /health HTTP/1.1\r\nHost: x\r\n\r\n".to_vec();
        let mut r = Chunked { data: raw, pos: 0, chunk: 7 };
        let req = read_http_request(&mut r, 65_536);
        assert!(req.starts_with("GET /health"));
    }

    #[test]
    fn oversize_body_reads_one_past_cap_so_422_fires_not_silent_truncation() {
        let body = "x".repeat(70_000);
        let mut r = Chunked { data: req_with_body(&body), pos: 0, chunk: 4096 };
        let req = read_http_request(&mut r, 65_536);
        let got_body = req.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or("");
        assert!(got_body.len() > 65_536, "body must exceed the cap so handle_batch 422s");
        assert!(got_body.len() <= 65_537 + 4096, "bounded — never reads the flood");
    }

    #[test]
    fn eof_mid_body_returns_partial_never_hangs() {
        let mut raw = req_with_body(&"y".repeat(10_000));
        raw.truncate(raw.len() - 6_000); // client dies mid-body
        let mut r = Chunked { data: raw, pos: 0, chunk: 2048 };
        let req = read_http_request(&mut r, 65_536);
        let got_body = req.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or("");
        assert_eq!(got_body.len(), 4_000, "returns what arrived; downstream validation refuses");
    }
}

/// #3845 — the dispatch bug, proven, then proven fixed.
///
/// `/effective/:node/:key` shipped in #3435 on 2026-06-17 and never answered a
/// single request. The handler was the FIRST branch of handle_inner and was
/// compiled into the running binary the whole time — but the accept loop chose a
/// route table before calling it, select_table matches a path's first segment
/// against a class plural, and "effective" is not a class. So every request
/// 404'd "unknown route" with a list of collections, which reads exactly like a
/// feature nobody built.
///
/// It stayed invisible because nothing tested the decision: it lived inline in
/// the accept loop. These run against `dispatch_for`, which is that decision
/// extracted — the point of extracting it.
#[cfg(test)]
mod dispatch_effective_3845 {
    use super::*;

    fn table(class: &str) -> RouteTable {
        RouteTable {
            class: format!("https://jeffbridwell.com/chorus#{class}"),
            fields: vec![],
            routes: vec![],
            secured: vec![],
            mandatory: vec![],
            write_required: vec![],
            repo_target: String::new(),
            exposure: vec![],
            instances_graph: format!("urn:chorus:test:{}", class.to_lowercase()),
            tree_edges: vec![],
            tree_order: None,
            model_version: "target".into(),
        }
    }

    /// The tables as mounted: Property is NOT first, so a "just use tables[0]"
    /// fix would pass a weaker test and read the wrong instances graph.
    fn tables() -> Vec<RouteTable> {
        vec![table("Domain"), table("Role"), table("Property"), table("Service")]
    }

    /// NEGATIVE PROOF — the OLD path, unchanged, still fails on the same input.
    /// select_table is what the dispatcher used to call, and it is still here:
    /// if this ever starts returning Some, the bug closed some other way and
    /// everything below is measuring nothing.
    #[test]
    fn the_old_selector_still_cannot_find_a_table_for_effective() {
        let t = tables();
        assert!(
            select_table("/effective/role-silas/response.word.cap", &t).is_none(),
            "select_table matching a class plural is exactly why /effective 404'd"
        );
    }

    #[test]
    fn effective_now_dispatches_to_the_property_table() {
        let t = tables();
        match dispatch_for("/effective/role-silas/response.word.cap", &t) {
            Dispatch::Table(i) => {
                assert!(t[i].class.ends_with("#Property"), "got {}", t[i].class);
                // The graph is the point: the effective fetch reads
                // table.instances_graph. Picking any old table would query the
                // wrong graph and return a confident wrong answer.
                assert_eq!(t[i].instances_graph, "urn:chorus:test:property");
            }
            other => panic!("expected the Property table, got {other:?}"),
        }
    }

    #[test]
    fn a_query_string_does_not_break_the_effective_match() {
        let t = tables();
        assert!(matches!(
            dispatch_for("/effective/role-silas/response.word.cap?trace=1", &t),
            Dispatch::Table(_)
        ));
    }

    /// NEGATIVE PROOF — the fix did not turn the dispatcher into a catch-all.
    /// An unknown path is still NotFound, and the collection routes still land
    /// on their own tables.
    #[test]
    fn ordinary_routes_are_untouched() {
        let t = tables();
        match dispatch_for("/domains", &t) {
            Dispatch::Table(i) => assert!(t[i].class.ends_with("#Domain")),
            other => panic!("expected the Domain table, got {other:?}"),
        }
        match dispatch_for("/properties", &t) {
            Dispatch::Table(i) => assert!(t[i].class.ends_with("#Property")),
            other => panic!("expected the Property table, got {other:?}"),
        }
        assert_eq!(dispatch_for("/nonsense", &t), Dispatch::NotFound);
        assert_eq!(dispatch_for("/", &t), Dispatch::NotFound);
    }

    /// The route is /effective/:node/:key — three segments. Neither the bare
    /// word nor a word merely STARTING with it is the route: without matching on
    /// "effective/" with the slash, /effectiveness would dispatch here.
    #[test]
    fn a_bare_or_prefixed_effective_is_not_the_route() {
        let t = tables();
        assert_eq!(dispatch_for("/effective", &t), Dispatch::NotFound);
        assert_eq!(dispatch_for("/effectiveness", &t), Dispatch::NotFound);
    }

    /// Kade's gap: a trailing slash DOES reach the Property table here, and then
    /// handle_inner rejects it on parts.len() != 3. Pinning that so the split of
    /// responsibility is deliberate — dispatch routes the family, the handler
    /// validates the shape — rather than an accident nobody wrote down.
    #[test]
    fn a_trailing_slash_dispatches_but_the_handler_owns_arity() {
        let t = tables();
        assert!(matches!(dispatch_for("/effective/role-silas/", &t), Dispatch::Table(_)));
        // Kade's degenerate: /effective/ with no node at all. Dispatch routes the
        // family; the handler refuses the arity. Pinned so the division stays
        // deliberate rather than becoming a surprise to whoever reads it next.
        assert!(matches!(dispatch_for("/effective/", &t), Dispatch::Table(_)));
        // 2 segments after the split -> not the 3 the handler requires.
        let parts: Vec<&str> = "/effective/role-silas/"
            .trim_end_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();
        assert_eq!(parts.len(), 2, "handle_inner requires 3 and will refuse this");
    }

    /// NEGATIVE PROOF for the third state. A model with no Property class means
    /// the path is RIGHT and the model is missing — reporting "unknown route"
    /// there sends the next reader hunting a typo that isn't there. Two states
    /// this check must never collapse into one (#3734).
    #[test]
    fn no_property_table_is_unavailable_not_unknown() {
        let t = vec![table("Domain"), table("Role")];
        assert_eq!(
            dispatch_for("/effective/role-silas/response.word.cap", &t),
            Dispatch::EffectiveUnavailable
        );
        // and the genuinely-unknown path is still unknown, in the same model
        assert_eq!(dispatch_for("/nonsense", &t), Dispatch::NotFound);
    }
}

#[cfg(test)]
mod version_axes_3947 {
    use super::*;

    /// The three version axes must never collapse into one field again —
    /// Kade's 422 triage read per-class review state as the ontology release.
    /// Asserted against the prelude builder's source: each axis is PUSHED
    /// (the escaped literal as it appears in the format! call).
    #[test]
    fn envelope_prelude_carries_all_three_axes() {
        let src = include_str!("lib.rs");
        for axis in ["modelVersion", "vocabVersion", "ontologyVersion"] {
            let pushed = format!("p.push(format!(\"\\\"{axis}\\\"");
            assert!(src.contains(&pushed), "axis {axis} not pushed in the prelude");
        }
    }

    /// NEGATIVE PROOF (#3734): absence is LOUD. With no store reachable the
    /// version reads "unversioned" — never empty, never a fabricated number.
    #[test]
    fn unreachable_store_reads_unversioned_not_empty() {
        let v = versioned_cached(
            &std::sync::Mutex::new(None),
            "PREFIX owl: <http://nowhere> SELECT ?v WHERE { GRAPH <urn:none> { <urn:x> owl:nothing ?v } }",
        );
        assert_eq!(v, "unversioned");
    }

    /// The TTL cache re-reads after expiry — the cached-at-startup defect
    /// (served 1.1.0 while the store held 1.0.0) cannot recur structurally.
    #[test]
    fn ttl_cache_expires_rather_than_caching_forever() {
        let slot: std::sync::Mutex<Option<(std::time::Instant, String)>> =
            std::sync::Mutex::new(Some((
                std::time::Instant::now() - std::time::Duration::from_secs(120),
                "stale-value".to_string(),
            )));
        let v = versioned_cached(&slot, "SELECT ?v WHERE { }");
        assert_ne!(v, "stale-value", "a 120s-old entry must be re-read, not served");
    }
}

#[cfg(test)]
mod bind_scope_tests_4004 {
    use super::bind_host;
    // in-flow (2026-09-03): both tests set and remove ATHENA_MAKE_BIND and raced
    // each other under the parallel runner (red twice today, green alone) — one lock
    static BIND_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// NEGATIVE PROOF — the default must stay LOOPBACK. This surface is
    /// read-only but unauthenticated; a change that quietly bound it to every
    /// interface would "fix" Jeff's phone by publishing the model to the LAN.
    /// Absent, empty, and whitespace-only all mean "not configured".
    #[test]
    fn the_default_is_loopback_and_blank_is_not_a_value() {
        let _g = BIND_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        for v in [None, Some(""), Some("   ")] {
            match v {
                None => std::env::remove_var("ATHENA_MAKE_BIND"),
                Some(x) => std::env::set_var("ATHENA_MAKE_BIND", x),
            }
            assert_eq!(bind_host(), "127.0.0.1", "unset/blank must not widen the bind");
        }
        std::env::remove_var("ATHENA_MAKE_BIND");
    }

    /// The other state: an operator CAN widen it, deliberately and auditably.
    /// Without this the check could pass by always answering loopback.
    #[test]
    fn an_operator_can_widen_it_explicitly() {
        let _g = BIND_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("ATHENA_MAKE_BIND", "0.0.0.0");
        assert_eq!(bind_host(), "0.0.0.0");
        std::env::remove_var("ATHENA_MAKE_BIND");
    }
}

#[cfg(test)]
mod sparql_argv_4022 {
    use super::*;

    /// Negative proof (#3734): the OLD shape — query on argv — cannot even spawn
    /// once the query outgrows ARG_MAX. This is the state the fix exists to
    /// separate from a real store error; keep it red-able here so the module
    /// cannot pass vacuously if someone puts the query back on argv.
    #[test]
    fn negative_proof_a_5mb_query_on_argv_fails_at_spawn_with_e2big() {
        let big = "x".repeat(5 * 1024 * 1024);
        let err = Command::new("curl")
            .args(["-sf", "--data-urlencode", &format!("query={}", big), "http://127.0.0.1:1/query"])
            .output()
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(err.contains("Argument list too long"), "expected E2BIG on argv, got: {:?}", err);
    }

    #[test]
    fn the_query_is_never_on_argv() {
        let args = sparql_curl_args("http://127.0.0.1:1");
        assert!(args.iter().any(|a| a == "@-"), "query must be read from stdin: {:?}", args);
        assert!(!args.iter().any(|a| a.starts_with("query=")), "query text leaked onto argv: {:?}", args);
        assert!(!args.iter().any(|a| a == "--data-urlencode"), "url-encoding a 10 MB body is curl's OOM: {:?}", args);
        assert!(args.iter().any(|a| a == "Content-Type: application/sparql-query"), "raw SPARQL POST body: {:?}", args);
        assert_eq!(args.last().map(String::as_str), Some("http://127.0.0.1:1/query"));
    }

    /// Control: the same 5 MB query through the fixed path gets PAST spawn and
    /// fails at the (dead) endpoint — a store error, not a process-table error.
    #[test]
    fn control_a_5mb_query_reaches_the_endpoint_via_stdin() {
        let big = "x".repeat(5 * 1024 * 1024);
        let err = sparql_json_at("http://127.0.0.1:1/pods", &big).unwrap_err();
        assert!(err.starts_with("fuseki-query failed"), "expected an endpoint failure, got: {}", err);
        assert!(!err.contains("Argument list too long"), "{}", err);
    }
}
