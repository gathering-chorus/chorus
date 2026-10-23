//! #4091 AC1 — the pure half of convergence-land: the credential and loopback
//! rules, the ICD field maps, and the Grafana rule and silence it writes. The
//! I/O (door, graph, Grafana) is in main.rs, so every rule here is testable.
use serde_json::{json, Value};

/// The env var holding the Grafana service-account token (never its value).
pub const TOKEN_ENV: &str = "GRAFANA_ALERTING_TOKEN";
/// Where the token lives when the env var is unset: a 0600 file Jeff writes.
pub const TOKEN_FILE: &str = ".chorus/credentials/grafana-alerting.token";
/// The contact point every rule notifies: chorus-api's webhook (#4666).
pub const RECEIVER: &str = "chorus-roles";

/// The credential: the env var if set, else the file. Refuses naming both,
/// never printing a value. `file` is the file's contents and its mode.
pub fn credential(env: Option<String>, file: Option<(String, u32)>, home: &str) -> Result<String, String> {
    if let Some(t) = env.map(|t| t.trim().to_string()).filter(|t| !t.is_empty()) {
        return Ok(t);
    }
    let path = format!("{home}/{TOKEN_FILE}");
    match file {
        Some((_, mode)) if mode & 0o077 != 0 => Err(format!(
            "refused: {path} is readable by others (mode {:o}); chmod 600 it", mode & 0o777
        )),
        Some((t, _)) if !t.trim().is_empty() => Ok(t.trim().to_string()),
        _ => Err(format!(
            "refused: no Grafana credential — set {TOKEN_ENV} or write the token to {path} (mode 600). Nothing was written."
        )),
    }
}

/// Grafana is reached on loopback only (ADR-012): anything else is refused.
pub fn loopback(url: &str) -> Result<(), String> {
    let rest = url.strip_prefix("http://").or_else(|| url.strip_prefix("https://")).unwrap_or("");
    let host = if let Some(r) = rest.strip_prefix('[') {
        r.split(']').next().unwrap_or("")
    } else {
        rest.split(['/', ':']).next().unwrap_or("")
    };
    match host {
        "localhost" | "127.0.0.1" | "::1" => Ok(()),
        _ => Err(format!("refused: Grafana at {url} is not loopback (localhost, 127.0.0.1, ::1)")),
    }
}

/// One ICD field map: the Alert row's field → a place in the Grafana rule.
#[derive(Debug, Clone, PartialEq)]
pub struct Mapping {
    pub field: String,
    pub target: String,
    pub kind: String,
    pub transform: String,
}

/// The maps out of a SPARQL JSON result with ?field ?target ?kind ?transform.
pub fn mappings(sparql: &Value) -> Vec<Mapping> {
    let get = |b: &Value, k: &str| b.get(k).and_then(|v| v.get("value")).and_then(Value::as_str).unwrap_or("").to_string();
    sparql.pointer("/results/bindings").and_then(Value::as_array).into_iter().flatten()
        .map(|b| Mapping { field: get(b, "field"), target: get(b, "target"), kind: get(b, "kind"), transform: get(b, "transform") })
        .filter(|m| !m.field.is_empty() && !m.target.is_empty())
        .collect()
}

/// `chorus:principal-silas`, `principal-silas` or a WebID → `silas`.
pub fn bare_role(s: &str) -> String {
    s.rsplit([':', '#', '/']).next().unwrap_or(s).trim_start_matches("principal-").to_string()
}

/// One field of a door row: `data` first, then `links` (a link is an IRI such
/// as `chorus:infrastructure`, so it is read bare: `infrastructure`).
pub fn field(row: &Value, name: &str) -> Option<String> {
    let first = |v: &Value| match v {
        Value::String(s) => Some(s.clone()),
        Value::Array(a) => a.first().and_then(Value::as_str).map(str::to_string),
        _ => None,
    };
    let data = row.get("data").unwrap_or(row);
    let bare = |s: String| s.rsplit([':', '#']).next().map(str::to_string);
    let s = match data.get(name).and_then(first) {
        Some(s) => s,
        // a single row's name is its id (chorus:host-pressure)
        None if name == "name" => bare(first(row.get("id")?)?)?,
        None => bare(first(row.pointer(&format!("/links/{name}"))?)?)?,
    };
    Some(s).filter(|s| !s.is_empty())
}

/// A Grafana rule (provisioning API shape) and the group interval, built only
/// from the maps: a changed map changes what is written, with no code change.
/// Refuses a row missing a field a map needs to make a working rule.
pub fn rule(row: &Value, maps: &[Mapping], folder_uid: &str) -> Result<(Value, String), String> {
    if maps.is_empty() {
        return Err("refused: no Grafana ICD field maps in the graph (icd-instance-grafana.ttl not loaded?)".into());
    }
    let mut r = json!({
        "orgID": 1, "folderUID": folder_uid, "condition": "C", "noDataState": "OK", "execErrState": "Alerting",
        "labels": {}, "annotations": {}, "notification_settings": {},
        "data": [
            {"refId": "A", "relativeTimeRange": {"from": 900, "to": 0}, "model": {"refId": "A", "instant": true}},
            {"refId": "C", "datasourceUid": "__expr__", "relativeTimeRange": {"from": 900, "to": 0},
             "model": {"refId": "C", "type": "threshold", "expression": "A",
                       "conditions": [{"evaluator": {"type": "gt", "params": [0]}}]}}
        ]
    });
    let mut interval = String::new();
    let mut missing = Vec::new();
    for m in maps {
        let value = if m.kind == "constant" {
            Some(m.transform.clone())
        } else {
            field(row, &m.field).map(|v| if m.field == "ownedBy" { bare_role(&v) } else { v })
        };
        let Some(v) = value else {
            if matches!(m.target.as_str(), "title" | "uid" | "data[A].model.expr" | "data[A].datasourceUid" | "labels.role") {
                missing.push(m.field.clone());
            }
            continue;
        };
        match m.target.as_str() {
            "group.interval" => interval = v,
            "folder" => { r["labels"]["domain"] = json!(v); }
            "data[A].model.expr" => { r["data"][0]["model"]["expr"] = json!(v); }
            "data[A].datasourceUid" => { r["data"][0]["datasourceUid"] = json!(v); }
            "notification.receiver" => { r["notification_settings"]["receiver"] = json!(v); }
            t => {
                let mut parts = t.splitn(2, '.');
                let (a, b) = (parts.next().unwrap_or(""), parts.next());
                match b {
                    Some(b) => { r[a][b] = json!(v); }
                    None => { r[a] = json!(v); }
                }
            }
        }
    }
    if !missing.is_empty() {
        return Err(format!("refused: the Alert row has no {} — a rule needs them", missing.join(", ")));
    }
    if interval.is_empty() { interval = "1m".into(); }
    // the query step found by its refId, as loki_query does; "loki" is the provisioned uid (Abby, Kade: #4671 review)
    let is_loki = r["data"].as_array().is_some_and(|d| d.iter().any(|s| s["refId"] == "A" && s["datasourceUid"] == "loki"));
    if is_loki { loki_query(&mut r); }
    Ok((r, interval))
}

/// #4671 — a Loki query is not a Prometheus instant query: Grafana errors on it
/// (#4091 AC8). A is a Loki instant query, B reduces it to its last value, and the
/// threshold C reads B.
fn loki_query(r: &mut Value) {
    let b = json!({"refId": "B", "datasourceUid": "__expr__", "relativeTimeRange": {"from": 900, "to": 0},
                   "model": {"refId": "B", "type": "reduce", "expression": "A", "reducer": "last"}});
    let Some(d) = r["data"].as_array_mut() else { return };
    // each step found by its refId, not its position (Silas, Kade, Abby: #4671 review)
    for step in d.iter_mut() {
        match step["refId"].as_str() {
            Some("A") => {
                if let Some(m) = step["model"].as_object_mut() { m.remove("instant"); }
                step["model"]["queryType"] = json!("instant");
            }
            Some("C") => { step["model"]["expression"] = json!("B"); }
            _ => {}
        }
    }
    let at = d.iter().position(|s| s["refId"] == "C").unwrap_or(d.len());
    d.insert(at, b);
}

/// A silence on the rule's uid label for `minutes`, from `now` (RFC 3339 UTC).
pub fn silence(rule_uid: &str, minutes: u64, starts: &str, ends: &str, by: &str) -> Result<Value, String> {
    if minutes == 0 || minutes > 7 * 24 * 60 {
        return Err(format!("refused: a silence is 1 minute to 7 days, not {minutes} minutes"));
    }
    Ok(json!({
        "matchers": [{"name": "__alert_rule_uid__", "value": rule_uid, "isRegex": false, "isEqual": true}],
        "startsAt": starts, "endsAt": ends, "createdBy": by,
        "comment": format!("convergence-land silence by {by} for {minutes}m (#4091)")
    }))
}


/// #4091 AC4 — rows against rules. `want` is each landable Alert row (one with
/// a metricQuery) and the rule its maps build; `have` is every rule Grafana
/// holds in a chorus-* folder. Missing: a row with no rule. Stale: a rule that
/// differs from what its row builds (a hand edit in Grafana), or a rule no row
/// claims any more.
#[derive(Debug, Default, PartialEq)]
pub struct Drift {
    pub tree: usize,
    pub graph: usize,
    pub missing: Vec<String>,
    pub stale: Vec<String>,
}

const COMPARED: [&str; 8] = [
    "/title", "/for", "/labels/role", "/labels/severity", "/labels/domain",
    "/annotations/summary", "/data/0/model/expr", "/data/0/datasourceUid",
];

/// #4690 — a Grafana duration ("0s", "5m", "1h30m") in seconds. Every part
/// needs its unit; a bare number, an empty string or an unknown unit is None,
/// so it never reads as the same wait as anything.
pub fn for_secs(d: &str) -> Option<u64> {
    if d.is_empty() {
        return None;
    }
    let (mut total, mut num) = (0u64, String::new());
    for ch in d.chars() {
        if ch.is_ascii_digit() {
            num.push(ch);
            continue;
        }
        let n: u64 = num.parse().ok()?;
        total += n * match ch { 's' => 1, 'm' => 60, 'h' => 3600, 'd' => 86400, _ => return None };
        num.clear();
    }
    num.is_empty().then_some(total)
}

/// One compared field the same on both sides. `/for` by its value in seconds
/// (#4690: rows hold "0m", Grafana answers "0s"); the rest as JSON.
fn same(p: &str, built: &Value, have: &Value) -> bool {
    let (b, h) = (built.pointer(p).filter(|v| !v.is_null()), have.pointer(p).filter(|v| !v.is_null()));
    if p == "/for" {
        // absent on both sides is the same (Silas); unreadable text never is
        if b.is_none() && h.is_none() {
            return true;
        }
        let s = |v: Option<&Value>| v.and_then(Value::as_str).and_then(for_secs);
        return matches!((s(b), s(h)), (Some(x), Some(y)) if x == y);
    }
    b == h
}

pub fn drift(want: &[(String, Value)], have: &[Value]) -> Drift {
    let uid = |r: &Value| r.get("uid").and_then(Value::as_str).unwrap_or("").to_string();
    let mut d = Drift { tree: want.len(), ..Default::default() };
    for (name, built) in want {
        match have.iter().find(|h| uid(h) == uid(built)) {
            None => d.missing.push(name.clone()),
            Some(h) => {
                d.graph += 1;
                let off: Vec<&str> = COMPARED.iter().copied()
                    .filter(|p| !same(p, built, h))
                    .collect();
                if !off.is_empty() {
                    d.stale.push(format!("{name} ({})", off.iter().map(|p| p.trim_start_matches('/')).collect::<Vec<_>>().join(", ")));
                }
            }
        }
    }
    for h in have {
        if !want.iter().any(|(_, b)| uid(b) == uid(h)) {
            d.stale.push(format!("{} (no Alert row claims it)", uid(h)));
        }
    }
    d
}

/// #4091 reopen 2 — whether Grafana's folder listing (GET /api/folders) holds
/// `uid`. Grafana 12 answers a GET of a folder that does not exist with 403
/// "folders:read", not 404, so a folder is looked up in the listing it lets
/// the token read. None: the listing is not a folder list (never "absent").
pub fn folder_present(listing: &str, uid: &str) -> Option<bool> {
    let v: Value = serde_json::from_str(listing).ok()?;
    let a = v.as_array()?;
    Some(a.iter().any(|f| f.get("uid").and_then(Value::as_str) == Some(uid)))
}

/// A 0600 file holding one `Authorization:` header line for `curl -H @file`, so
/// a token never rides argv; removed on drop. #4091 reopen 3: the name was the
/// pid alone, so the second header in one run (Grafana's, still held, then the
/// door's for the uid write-back) failed "File exists" after the rule landed.
/// Each one now takes the next number of a per-process counter.
pub struct HeaderFile(pub String);
impl Drop for HeaderFile {
    fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); }
}
pub fn header_file(line: &str) -> Result<HeaderFile, String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, Ordering::SeqCst);
    let p = format!("{}/convergence-land-{}-{n}.h", std::env::temp_dir().display(), std::process::id());
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&p).map_err(|e| format!("header file: {e}"))?;
    f.write_all(line.as_bytes()).map_err(|e| format!("header file: {e}"))?;
    Ok(HeaderFile(p))
}

/// #4714 — the structured event every convergence-land call ends with, as
/// `chorus-log` args: `convergence.call <role> op= target= outcome= [counts=] [reason=]`.
/// Before this the tool printed only text, so the 10-22/10-23 sync runs that
/// could not measure left no reason anywhere. `outcome` is ok, unmeasured
/// (a sync that printed `unmeasured <why>`) or refused (an Err); anything not
/// ok always carries a non-empty reason.
pub fn call_event(role: &str, args: &[String], result: &Result<String, String>) -> Vec<String> {
    let op = args.first().cloned().unwrap_or_else(|| "none".into());
    let mut ev = vec!["convergence.call".to_string(), if role.is_empty() { "system".into() } else { role.to_string() }, format!("op={op}")];
    if let Some(t) = args.get(1) {
        ev.push(format!("target={t}"));
    }
    let cap = |s: &str| -> String { let s = s.trim(); if s.is_empty() { "no reason given".into() } else { s.chars().take(400).collect() } };
    match result {
        Ok(s) => {
            let head = s.lines().next().unwrap_or("");
            if let Some(why) = head.strip_prefix("unmeasured") {
                ev.push("outcome=unmeasured".into());
                ev.push(format!("reason={}", cap(why)));
            } else {
                ev.push("outcome=ok".into());
                if let Some(c) = head.strip_prefix("measured ") {
                    ev.push(format!("counts={}", c.trim()));
                }
            }
        }
        Err(e) => {
            ev.push("outcome=refused".into());
            ev.push(format!("reason={}", cap(e)));
        }
    }
    ev
}
