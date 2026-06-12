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
    let plural = format!("{}s", class_local.to_lowercase());
    let routes = vec![
        format!("GET /{}", plural),
        format!("GET /{}/:name", plural),
        format!("GET /{}/:name/contains", plural),
        format!("GET /schema/{}", class_local.to_lowercase()),
    ];
    Ok(RouteTable { class, fields, routes })
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
    format!(
        "{{\n  \"generatedFrom\": \"{}\",\n  \"graph\": \"{}\",\n  \"fields\": [{}],\n  \"routes\": [{}]\n}}\n",
        t.class, ONTOLOGY_GRAPH, fields, routes
    )
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\r', "\\r")
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

fn handle_inner(path: &str, table: &RouteTable, meta: &mut ReqMeta) -> (u16, String) {
    let plural = format!("/{}s", table.class.rsplit('#').next().unwrap_or("domain").to_lowercase());
    let parts: Vec<&str> = path.trim_end_matches('/').split('/').filter(|s| !s.is_empty()).collect();

    // GET /schema/domain
    if path.starts_with("/schema/") {
        meta.route = "schema".into();
        let t = RouteTable { class: table.class.clone(), fields: table.fields.clone(), routes: table.routes.clone() };
        return (200, routes_json(&t));
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
    format!(
        "HTTP/1.1 {}\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status, body.len(), body
    )
}

pub fn serve(port: u16, table: &RouteTable) -> R<()> {
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("bind {}: {}", port, e))?;
    eprintln!("owl-api: serving generated {} API on :{} (read-only; writes go through chorus-model)", table.class, port);
    let mut req_counter: u64 = 0;
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
        let ((code, body), meta) = handle_meta(&path, table);
        let upstream_ms = upstream_started.elapsed().as_millis();
        let status = match code { 200 => "200 OK", 404 => "404 Not Found", _ => "502 Bad Gateway" };
        let resp = http_response(status, &body);
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
    fn routes_json_is_deterministic() {
        let t = RouteTable {
            class: format!("{}Domain", NS),
            fields: vec!["comment".into(), "label".into()],
            routes: vec!["GET /domains".into()],
        };
        assert_eq!(routes_json(&t), routes_json(&t));
        assert!(routes_json(&t).contains("\"generatedFrom\""));
    }

    #[test]
    fn unknown_route_404s_and_teaches_routes() {
        let t = RouteTable { class: format!("{}Domain", NS), fields: vec![], routes: vec!["GET /domains".into()] };
        let (code, body) = handle("/nope", &t);
        assert_eq!(code, 404);
        assert!(body.contains("GET /domains"));
    }
}
