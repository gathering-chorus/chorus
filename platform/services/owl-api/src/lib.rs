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

/// GENERATE — read the shape's direct-path properties for `class` from the
/// ontology graph and derive the route table.
pub fn generate(class_local: &str) -> R<RouteTable> {
    let class = format!("{}{}", NS, class_local);
    let q = format!(
        "PREFIX sh: <http://www.w3.org/ns/shacl#> SELECT ?v WHERE {{ GRAPH <{g}> {{ ?s sh:targetClass <{c}> ; sh:property ?p . ?p sh:path ?path . FILTER(isIRI(?path)) BIND(REPLACE(STR(?path), '.*#', '') AS ?v) }} }} ORDER BY ?v",
        g = ONTOLOGY_GRAPH, c = class
    );
    let body = sparql_json(&q)?;
    let mut fields: Vec<String> = select_v(&body);
    fields.sort();
    fields.dedup();
    if fields.is_empty() {
        return Err(format!("no shape found for {} in {} — land the schema first", class, ONTOLOGY_GRAPH));
    }
    let plural = format!("{}s", class_local.to_lowercase());
    let routes = vec![
        format!("GET /{}", plural),
        format!("GET /{}/:name", plural),
        format!("GET /{}/:name/contains", plural),
        format!("GET /schema/{}", class_local.to_lowercase()),
    ];
    Ok(RouteTable { class, fields, routes })
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
        let val = if o.starts_with("http") && o.contains('#') {
            o.rsplit('#').next().unwrap_or(&o).to_string()
        } else {
            o
        };
        parts.push(format!("\"{}\": \"{}\"", json_escape(&key), json_escape(&val)));
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
pub fn serve(port: u16, table: &RouteTable) -> R<()> {
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("bind {}: {}", port, e))?;
    eprintln!("owl-api: serving generated {} API on :{} (read-only; writes go through chorus-model)", table.class, port);
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
        let resp = format!(
            "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            status, body.len(), body
        );
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
                trace_id: header("x-chorus-trace-id"),
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
