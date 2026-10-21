//! convergence-land — #4091 AC1: land an Alert row in Grafana as a rule, or
//! silence one. The I/O half; every rule it applies is in lib.rs.
//!
//!   convergence-land alert <alert-name> [--dry-run [true|false]]
//!   convergence-land silence <alert-name> <minutes>
//!   convergence-land sync        (AC4: rows against rules, read by the sync sensor)
//!
//! GRAFANA_URL (default http://localhost:3100, loopback only), CHORUS_DOOR
//! (default http://localhost:3360/v1), CHORUS_ICD_QUERY (default the pods
//! dataset's query endpoint). The token is never put on a command line: curl
//! reads the Authorization header from a 0600 file that is removed after.
use convergence_land::*;
use serde_json::{json, Value};
use std::process::Command;

const PROVIDER: &str = "https://jeffbridwell.com/icd/provider/grafana/alert-rules";

fn env_or(k: &str, d: &str) -> String {
    std::env::var(k).ok().filter(|v| !v.is_empty()).unwrap_or_else(|| d.to_string())
}

/// curl with an optional header file; returns (status, body).
fn http(method: &str, url: &str, body: Option<&Value>, header_file: Option<&str>, extra: &[&str]) -> Result<(u16, String), String> {
    let mut c = Command::new("curl");
    c.args(["-s", "-m", "20", "-o", "-", "-w", "\n%{http_code}", "-X", method, url, "-H", "Content-Type: application/json", "-H", "Accept: application/json"]);
    if let Some(h) = header_file { c.args(["-H", &format!("@{h}")]); }
    for e in extra { c.args(["-H", e]); }
    if let Some(b) = body { c.args(["--data", &b.to_string()]); }
    let o = c.output().map_err(|e| format!("curl: {e}"))?;
    let out = String::from_utf8_lossy(&o.stdout).to_string();
    let (b, code) = out.rsplit_once('\n').unwrap_or(("", "0"));
    Ok((code.trim().parse().unwrap_or(0), b.to_string()))
}

struct HeaderFile(String);
impl Drop for HeaderFile {
    fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); }
}
fn header_file(line: &str) -> Result<HeaderFile, String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let p = format!("{}/convergence-land-{}.h", std::env::temp_dir().display(), std::process::id());
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&p).map_err(|e| format!("header file: {e}"))?;
    f.write_all(line.as_bytes()).map_err(|e| format!("header file: {e}"))?;
    Ok(HeaderFile(p))
}

fn grafana_auth() -> Result<(String, HeaderFile), String> {
    let url = env_or("GRAFANA_URL", "http://localhost:3100");
    loopback(&url)?;
    let home = env_or("HOME", "");
    let path = format!("{home}/{TOKEN_FILE}");
    let file = std::fs::metadata(&path).ok().and_then(|m| {
        use std::os::unix::fs::PermissionsExt;
        std::fs::read_to_string(&path).ok().map(|t| (t, m.permissions().mode()))
    });
    let token = credential(std::env::var(TOKEN_ENV).ok(), file, &home)?;
    Ok((url.trim_end_matches('/').to_string(), header_file(&format!("Authorization: Bearer {token}"))?))
}

fn alert_row(name: &str) -> Result<Value, String> {
    let door = env_or("CHORUS_DOOR", "http://localhost:3360/v1");
    let (code, body) = http("GET", &format!("{door}/alerts/alerts/{name}"), None, None, &[])?;
    if code != 200 { return Err(format!("refused: no Alert row {name} (door {code})")); }
    serde_json::from_str(&body).map_err(|e| format!("Alert row {name}: {e}"))
}

fn icd_maps() -> Result<Vec<Mapping>, String> {
    let q = format!(r#"PREFIX icd: <urn:gathering:icd#>
SELECT ?field ?target ?kind ?transform WHERE {{ GRAPH <urn:gathering:icd/current> {{
  ?m icd:fromProvider <{PROVIDER}> ; icd:mapsToName ?field ; icd:sourceField ?target .
  OPTIONAL {{ ?m icd:transformType ?kind }} OPTIONAL {{ ?m icd:transform ?transform }} }} }}"#);
    let url = env_or("CHORUS_ICD_QUERY", "http://localhost:3030/pods/query");
    let o = Command::new("curl").args(["-s", "-m", "20", "-H", "Accept: application/sparql-results+json", "--data-urlencode", &format!("query={q}"), &url])
        .output().map_err(|e| format!("curl: {e}"))?;
    let v: Value = serde_json::from_slice(&o.stdout).map_err(|e| format!("ICD maps unreadable ({e}) — UNMEASURED, nothing written"))?;
    Ok(mappings(&v))
}

fn ok(code: u16) -> bool { (200..300).contains(&code) }

fn land_alert(name: &str, dry_run: bool) -> Result<String, String> {
    let row = alert_row(name)?;
    let domain = field(&row, "hasDomain").unwrap_or_default();
    if domain.is_empty() { return Err(format!("refused: Alert row {name} has no domain")); }
    let folder = format!("chorus-{domain}");
    let (mut r, interval) = rule(&row, &icd_maps()?, &folder)?;
    r["ruleGroup"] = json!(domain);
    if dry_run {
        return Ok(format!("dry run — would land in folder {folder}, group {domain} every {interval}:\n{}", serde_json::to_string_pretty(&r).unwrap_or_default()));
    }
    let (g, auth) = grafana_auth()?;
    let h = Some(auth.0.as_str());
    // reopen 2: a missing folder reads 403 on its own GET (Grafana 12), so look it up in the listing
    let (c, b) = http("GET", &format!("{g}/api/folders?limit=1000"), None, h, &[])?;
    match (ok(c), folder_present(&b, &folder)) {
        (true, Some(true)) => {}
        (true, Some(false)) => {
            let (c, b) = http("POST", &format!("{g}/api/folders"), Some(&json!({"uid": folder, "title": format!("Chorus {domain}")})), h, &[])?;
            if !ok(c) { return Err(format!("Grafana refused the folder {folder} ({c}): {b}")); }
        }
        _ => return Err(format!("Grafana folders unreadable ({c}) — nothing written")),
    }
    let uid = r["uid"].as_str().unwrap_or(name).to_string();
    // X-Disable-Provenance: the rule stays editable, so a hand edit is possible and the sync sensor (AC4) reads it stale
    let np = ["X-Disable-Provenance: true"];
    let (mut c, mut b) = http("PUT", &format!("{g}/api/v1/provisioning/alert-rules/{uid}"), Some(&r), h, &np)?;
    if c == 404 {
        (c, b) = http("POST", &format!("{g}/api/v1/provisioning/alert-rules"), Some(&r), h, &np)?;
    }
    if !ok(c) { return Err(format!("Grafana refused rule {uid} ({c}): {b}")); }
    let grp = format!("{g}/api/v1/provisioning/folder/{folder}/rule-groups/{domain}");
    if let Ok((200, gb)) = http("GET", &grp, None, h, &[]) {
        if let Ok(mut gv) = serde_json::from_str::<Value>(&gb) {
            if gv["interval"] != json!(interval_secs(&interval)) {
                gv["interval"] = json!(interval_secs(&interval));
                let _ = http("PUT", &grp, Some(&gv), h, &np);
            }
        }
    }
    write_back(name, &uid)?;
    Ok(format!("landed {name} as Grafana rule {uid} in {folder}/{domain}, every {interval}; grafanaRuleUid written"))
}

fn interval_secs(d: &str) -> u64 {
    let (n, u) = d.split_at(d.len().saturating_sub(1));
    let n: u64 = n.parse().unwrap_or(60);
    match u { "h" => n * 3600, "m" => n * 60, _ => n }
}

fn write_back(name: &str, uid: &str) -> Result<(), String> {
    let door = env_or("CHORUS_DOOR", "http://localhost:3360/v1");
    let role = env_or("CHORUS_ROLE", "");
    let root = env_or("CHORUS_HOME", &format!("{}/CascadeProjects/chorus", env_or("HOME", "")));
    let t = match std::env::var("CHORUS_IDENTITY_TOKEN").ok().filter(|t| !t.is_empty()) {
        Some(t) => t,
        None => {
            let o = Command::new(format!("{root}/platform/scripts/chorus-identity-token")).arg(&role).output().map_err(|e| format!("identity: {e}"))?;
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        }
    };
    if t.is_empty() { return Err(format!("rule landed but grafanaRuleUid NOT written: no identity for {role:?}")); }
    let h = header_file(&format!("Authorization: Bearer {t}"))?;
    let (c, b) = http("PATCH", &format!("{door}/alerts/alerts/{name}"), Some(&json!({"grafanaRuleUid": uid})), Some(&h.0), &[])?;
    if !ok(c) { return Err(format!("rule landed but grafanaRuleUid NOT written ({c}): {b}")); }
    Ok(())
}

fn utc_in(minutes: u64) -> String {
    let o = Command::new("date").args(["-u", &format!("-v+{minutes}M"), "+%Y-%m-%dT%H:%M:%SZ"]).output();
    o.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
}

fn land_silence(name: &str, minutes: &str) -> Result<String, String> {
    let m: u64 = minutes.parse().map_err(|_| format!("refused: minutes must be a number, not {minutes:?}"))?;
    let row = alert_row(name)?;
    let uid = field(&row, "grafanaRuleUid")
        .ok_or_else(|| format!("refused: {name} has no grafanaRuleUid — land it first (convergence-land alert {name})"))?.to_string();
    let s = silence(&uid, m, &utc_in(0), &utc_in(m), &env_or("CHORUS_ROLE", "unknown"))?;
    let (g, auth) = grafana_auth()?;
    let (c, b) = http("POST", &format!("{g}/api/alertmanager/grafana/api/v2/silences"), Some(&s), Some(&auth.0), &[])?;
    if !ok(c) { return Err(format!("Grafana refused the silence ({c}): {b}")); }
    Ok(format!("silenced {name} (rule {uid}) for {m}m: {}", b.trim()))
}

/// AC4 — the grafana sync reading, as lines chorus-crawl's validate reads:
/// `measured tree=N graph=N` (or `unmeasured <why>`), then `missing <row>` and
/// `stale <rule (what)>` lines. Read-only. A store or Grafana it cannot read is
/// unmeasured, never zero.
fn sync() -> String {
    match sync_drift() {
        Ok(d) => {
            let mut out = vec![format!("measured tree={} graph={}", d.tree, d.graph)];
            out.extend(d.missing.iter().map(|m| format!("missing {m}")));
            out.extend(d.stale.iter().map(|m| format!("stale {m}")));
            out.join("\n")
        }
        Err(e) => format!("unmeasured {e}"),
    }
}

fn sync_drift() -> Result<Drift, String> {
    let door = env_or("CHORUS_DOOR", "http://localhost:3360/v1");
    let (c, b) = http("GET", &format!("{door}/alerts/alerts?limit=1000"), None, None, &[])?;
    if c != 200 { return Err(format!("Alert rows unreadable (door {c})")); }
    let list: Value = serde_json::from_str(&b).map_err(|e| format!("Alert rows: {e}"))?;
    let maps = icd_maps()?;
    let mut want = Vec::new();
    for r in list["data"].as_array().into_iter().flatten() {
        if field(r, "metricQuery").is_none() { continue; }
        let name = field(r, "name").unwrap_or_default();
        let row = alert_row(&name)?;
        let domain = field(&row, "hasDomain").unwrap_or_default();
        let (built, _) = rule(&row, &maps, &format!("chorus-{domain}"))?;
        want.push((name, built));
    }
    let (g, auth) = grafana_auth()?;
    let (c, b) = http("GET", &format!("{g}/api/v1/provisioning/alert-rules"), None, Some(&auth.0), &[])?;
    if c != 200 { return Err(format!("Grafana rules unreadable ({c})")); }
    let rules: Value = serde_json::from_str(&b).map_err(|e| format!("Grafana rules: {e}"))?;
    let have: Vec<Value> = rules.as_array().into_iter().flatten()
        .filter(|r| r.get("folderUID").and_then(Value::as_str).is_some_and(|f| f.starts_with("chorus-")))
        .cloned().collect();
    Ok(drift(&want, &have))
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let r = match a.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["alert", n] => land_alert(n, false),
        // the runner passes the flag's value (`[--dry-run {dry_run}]`)
        ["alert", n, "--dry-run"] | ["alert", n, "--dry-run", "true"] => land_alert(n, true),
        ["alert", n, "--dry-run", "false"] => land_alert(n, false),
        ["silence", n, m] => land_silence(n, m),
        ["sync"] => Ok(sync()),
        _ => Err("usage: convergence-land alert <alert-name> [--dry-run] | silence <alert-name> <minutes> | sync".into()),
    };
    std::process::exit(match r {
        Ok(s) => { println!("{s}"); 0 }
        Err(e) => { eprintln!("convergence-land: {e}"); 1 }
    });
}
