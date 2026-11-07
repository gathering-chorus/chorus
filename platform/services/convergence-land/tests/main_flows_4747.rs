// @test-type: unit — hermetic: the real binary against a loopback stub of the door, the ICD query and Grafana; no live service.
// @domain: alerts
// @card: #4747
//! #4747 — convergence-land's process half (src/main.rs): what it asks the door,
//! the ICD and Grafana for, in what order, and what it refuses. What Jeff sees:
//! an alert lands as a Grafana rule with its uid written back; a dry run writes
//! nothing; a silence names its rule; sync reports drift or says unmeasured.
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::sync::{Arc, Mutex};

type Log = Arc<Mutex<Vec<String>>>;

fn maps() -> Value {
    let b = |f: &str, t: &str, k: &str, x: &str| json!({"field": {"value": f}, "target": {"value": t}, "kind": {"value": k}, "transform": {"value": x}});
    json!({"results": {"bindings": [
        b("label", "title", "copy", ""), b("hasDomain", "folder", "map", ""), b("ownedBy", "labels.role", "map", ""),
        b("metricQuery", "data[A].model.expr", "copy", ""), b("metricDatasource", "data[A].datasourceUid", "copy", ""),
        b("forDuration", "for", "copy", ""), b("evalInterval", "group.interval", "copy", ""), b("name", "uid", "copy", ""),
        b("contactPoint", "notification.receiver", "constant", "chorus-roles"),
    ]}})
}

fn row(uid: bool) -> Value {
    let mut d = json!({"name": "host-pressure", "label": "Host memory pressure", "metricQuery": "chorus_host_pressure >= 4",
                       "metricDatasource": "prometheus", "forDuration": "5m", "evalInterval": "1m"});
    if uid { d["grafanaRuleUid"] = json!("host-pressure"); }
    json!({"id": "chorus:host-pressure", "data": d, "links": {"ownedBy": "chorus:principal-silas", "hasDomain": "chorus:infrastructure"}})
}

/// One canned answer per request line; every request is logged as "METHOD path".
fn answer(method: &str, path: &str, rule_exists: bool) -> (u16, String) {
    match (method, path) {
        ("GET", p) if p.starts_with("/v1/alerts/alerts?") => (200, json!({"data": [row(false)]}).to_string()),
        ("GET", "/v1/alerts/alerts/host-pressure") => (200, row(true).to_string()),
        ("GET", "/v1/alerts/alerts/nobody") => (404, "{}".into()),
        ("PATCH", "/v1/alerts/alerts/host-pressure") => (200, "{}".into()),
        ("POST", "/query") => (200, maps().to_string()),
        ("GET", p) if p.starts_with("/api/folders") => (200, "[]".into()),
        ("POST", "/api/folders") => (200, "{}".into()),
        ("PUT", "/api/v1/provisioning/alert-rules/host-pressure") if !rule_exists => (404, "{}".into()),
        ("PUT", p) if p.starts_with("/api/v1/provisioning/") => (200, "{}".into()),
        ("POST", "/api/v1/provisioning/alert-rules") => (201, "{}".into()),
        ("GET", p) if p.contains("/rule-groups/") => (200, json!({"interval": 30, "rules": []}).to_string()),
        ("GET", "/api/v1/provisioning/alert-rules") => (200, "[]".into()),
        ("POST", "/api/alertmanager/grafana/api/v2/silences") => (200, json!({"silenceID": "s1"}).to_string()),
        _ => (500, format!("stub has no answer for {method} {path}")),
    }
}

fn stub(rule_exists: bool) -> (String, Log) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://127.0.0.1:{}", l.local_addr().unwrap().port());
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let seen = log.clone();
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut first = String::new();
            if r.read_line(&mut first).is_err() { continue; }
            let mut len = 0usize;
            loop {
                let mut h = String::new();
                if r.read_line(&mut h).is_err() || h == "\r\n" || h.is_empty() { break; }
                if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") { len = v.trim().parse().unwrap_or(0); }
            }
            let mut body = vec![0u8; len];
            let _ = r.read_exact(&mut body);
            let mut parts = first.split_whitespace();
            let (m, p) = (parts.next().unwrap_or("").to_string(), parts.next().unwrap_or("").to_string());
            seen.lock().unwrap().push(format!("{m} {p}"));
            let (code, b) = answer(&m, &p, rule_exists);
            let mut s = s;
            let _ = write!(s, "HTTP/1.1 {code} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}", b.len());
        }
    });
    (base, log)
}

/// The binary, pointed at the stub, with a chorus-log that records nothing.
fn run(base: &str, args: &[&str]) -> (i32, String, String) {
    let home = std::env::temp_dir().join(format!("cl-4747-{}-{}", std::process::id(), args.join("-").replace('/', "_")));
    let scripts = home.join("platform/scripts");
    std::fs::create_dir_all(&scripts).unwrap();
    let log = scripts.join("chorus-log");
    std::fs::write(&log, "#!/bin/sh\nexit 0\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&log, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut c = Command::new(env!("CARGO_BIN_EXE_convergence-land"));
    c.args(args).env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", &home).env("CHORUS_HOME", &home).env("CHORUS_ROLE", "kade")
        .env("CHORUS_DOOR", format!("{base}/v1")).env("CHORUS_ICD_QUERY", format!("{base}/query"))
        .env("GRAFANA_URL", base).env("GRAFANA_ALERTING_TOKEN", "stub-token").env("CHORUS_IDENTITY_TOKEN", "stub-identity");
    // keep coverage's profile path, so the process's own lines count
    if let Ok(p) = std::env::var("LLVM_PROFILE_FILE") { c.env("LLVM_PROFILE_FILE", p); }
    let o = c.output().unwrap();
    let _ = std::fs::remove_dir_all(&home);
    (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).into(), String::from_utf8_lossy(&o.stderr).into())
}

#[test]
fn an_alert_lands_as_a_rule_creates_its_folder_and_writes_its_uid_back() {
    let (base, log) = stub(false);
    let (code, out, err) = run(&base, &["alert", "host-pressure"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("landed host-pressure as Grafana rule host-pressure in chorus-infrastructure/infrastructure"), "{out}");
    let seen = log.lock().unwrap().join("\n");
    for want in ["POST /api/folders", "PUT /api/v1/provisioning/alert-rules/host-pressure", "POST /api/v1/provisioning/alert-rules", "PATCH /v1/alerts/alerts/host-pressure"] {
        assert!(seen.contains(want), "missing {want} in\n{seen}");
    }
    assert!(seen.contains("PUT /api/v1/provisioning/folder/chorus-infrastructure/rule-groups/infrastructure"), "the group interval is set: {seen}");
}

/// NEGATIVE PROOF: the same alert as a dry run reads but writes nothing.
#[test]
fn a_dry_run_writes_nothing() {
    let (base, log) = stub(false);
    let (code, out, _) = run(&base, &["alert", "host-pressure", "--dry-run"]);
    assert_eq!(code, 0);
    assert!(out.starts_with("dry run — would land in folder chorus-infrastructure"), "{out}");
    let seen = log.lock().unwrap().clone();
    assert!(seen.iter().all(|r| r.starts_with("GET ") || r == "POST /query"), "a dry run wrote: {seen:?}");
}

#[test]
fn an_alert_with_no_row_is_refused() {
    let (base, _) = stub(false);
    let (code, _, err) = run(&base, &["alert", "nobody", "--dry-run", "false"]);
    assert_eq!(code, 1);
    assert!(err.contains("refused: no Alert row nobody (door 404)"), "{err}");
}

#[test]
fn a_silence_names_its_rule() {
    let (base, log) = stub(true);
    let (code, out, err) = run(&base, &["silence", "host-pressure", "30"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.starts_with("silenced host-pressure (rule host-pressure) for 30m"), "{out}");
    assert!(log.lock().unwrap().iter().any(|r| r == "POST /api/alertmanager/grafana/api/v2/silences"));
    let (code, _, err) = run(&base, &["silence", "host-pressure", "soon"]);
    assert_eq!(code, 1);
    assert!(err.contains("minutes must be a number"), "{err}");
}

#[test]
fn sync_reports_a_row_with_no_rule_as_missing() {
    let (base, _) = stub(true);
    let (code, out, err) = run(&base, &["sync"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.starts_with("measured tree=1 graph=0"), "{out}");
    assert!(out.contains("missing host-pressure"), "{out}");
}

/// NEGATIVE PROOF: a store it cannot read is unmeasured, never "0 missing".
#[test]
fn sync_against_no_store_is_unmeasured() {
    let (code, out, _) = run("http://127.0.0.1:1", &["sync"]);
    assert_eq!(code, 0);
    assert!(out.starts_with("unmeasured "), "{out}");
}

#[test]
fn an_unknown_command_prints_usage() {
    let (code, _, err) = run("http://127.0.0.1:1", &["bogus"]);
    assert_eq!(code, 1);
    assert!(err.contains("usage: convergence-land alert"), "{err}");
}
