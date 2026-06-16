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
    let routes = vec![
        format!("GET /{}", plural),
        format!("GET /{}/:name", plural),
        format!("GET /{}/:name/contains", plural),
        format!("GET /{}/:name/partof", plural),
        format!("GET /{}/:name/has-child", plural),
        format!("GET /schema/{}", class_local.to_lowercase()),
    ];
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
    Ok(RouteTable { class, fields, routes, secured })
}

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
    let mut paths: Vec<String> = t
        .routes
        .iter()
        .map(|r| {
            let p = r.trim_start_matches("GET ").replace(":name", "{name}");
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
                "    \"{}\": {{ \"get\": {{ {}\"responses\": {{ \"200\": {{ \"description\": \"ok\", \"content\": {{ \"application/json\": {{ \"schema\": {{ \"$ref\": \"{}\" }} }} }} }}, \"404\": {{ \"description\": \"typed refusal\" }} }} }} }}",
                p, params, resp
            )
        })
        .collect();
    paths.sort();
    format!(
        "{{\n  \"openapi\": \"3.0.3\",\n  \"info\": {{ \"title\": \"OWL API — generated {class_short} API\", \"version\": \"0\", \"description\": \"Generated from {class} shapes in {graph}. Regenerate, never hand-edit (#3354).\" }},\n  \"paths\": {{\n{paths}\n  }},\n  \"components\": {{ \"schemas\": {{\n    \"EdgeRef\": {{ \"type\": \"object\", \"properties\": {{ \"name\": {{ \"type\": \"string\" }}, \"label\": {{ \"type\": \"string\" }} }} }},\n    \"{class_short}\": {{ \"type\": \"object\", \"properties\": {{ {props} }} }},\n    \"List\": {{ \"type\": \"object\", \"properties\": {{ \"count\": {{ \"type\": \"integer\" }}, \"items\": {{ \"type\": \"array\", \"items\": {{ \"type\": \"object\", \"properties\": {{ \"name\": {{ \"type\": \"string\" }}, \"label\": {{ \"type\": \"string\" }}, \"status\": {{ \"type\": \"string\" }} }} }} }} }} }},\n    \"Fold\": {{ \"type\": \"object\", \"properties\": {{ \"{class_l}\": {{ \"type\": \"string\" }}, \"count\": {{ \"type\": \"integer\" }}, \"contains\": {{ \"type\": \"array\", \"items\": {{ \"type\": \"string\" }} }} }} }},\n    \"Schema\": {{ \"type\": \"object\" }}\n  }} }}\n}}\n",
        class_short = class_short,
        class = t.class,
        graph = ONTOLOGY_GRAPH,
        paths = paths.join(",\n"),
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
    format!(
        "{{\n  \"generatedFrom\": \"{}\",\n  \"graph\": \"{}\",\n  \"fields\": [{}],\n  \"routes\": [{}],\n  \"secured\": [{}]\n}}\n",
        t.class, ONTOLOGY_GRAPH, fields, routes, secured
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
        let t = RouteTable { class: table.class.clone(), fields: table.fields.clone(), routes: table.routes.clone(), secured: table.secured.clone() };
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
        let ((code, body), meta) = match auth::seam_auth(&path, &header("authorization"), &secret, &allowed_webids, now_secs, &table.secured) {
            Some((c, b)) => ((c, b), ReqMeta { route: "auth-refused".into(), ..Default::default() }),
            None => handle_meta(&path, table),
        };
        let upstream_ms = upstream_started.elapsed().as_millis();
        let status = match code {
            200 => "200 OK",
            401 => "401 Unauthorized",
            403 => "403 Forbidden",
            404 => "404 Not Found",
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

    #[test]
    fn routes_json_is_deterministic() {
        let t = RouteTable {
            class: format!("{}Domain", NS),
            fields: vec!["comment".into(), "label".into()],
            routes: vec!["GET /domains".into()],
            secured: vec!["/schema/domain".into()],
        };
        assert_eq!(routes_json(&t), routes_json(&t));
        assert!(routes_json(&t).contains("\"generatedFrom\""));
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
        let t = RouteTable { class: format!("{}Domain", NS), fields: vec![], routes: vec!["GET /domains".into()], secured: vec![] };
        let (code, body) = handle("/nope", &t);
        assert_eq!(code, 404);
        assert!(body.contains("GET /domains"));
    }
}
