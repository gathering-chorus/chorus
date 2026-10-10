// @domain: services — the services crawl: launchd units → ServiceInstance / ScheduledJob rows (#4472)
//! #4472 — the services crawl (the #3870 harvest, ported to Rust and moved
//! behind the door). Pure decisions only: what a launchd line
//! is, whether a unit is ours, which class it becomes, which binary it runs.
//! main does the I/O (launchctl, plutil, ssh, the door).
//!
//! Parity first (Jeff 2026-10-09): every rule here is the bash generator's
//! rule, with the same evidence order, so the Rust crawl writes the same rows
//! before it writes any new field.

/// One line of `launchctl list`: pid (or none), last exit code, label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedUnit {
    pub pid: Option<String>,
    pub last_exit_code: Option<i64>,
    pub label: String,
}

impl ListedUnit {
    /// running when launchd holds a pid, loaded otherwise (the bash rule).
    pub fn run_state(&self) -> &'static str {
        if self.pid.is_some() { "running" } else { "loaded" }
    }
}

/// The walk's scope: every non-Apple `com.*` unit. No allow-list — an
/// allow-list only finds what someone remembered to add (#3870, Wren).
pub fn keep(label: &str) -> bool {
    label.starts_with("com.") && !label.starts_with("com.apple")
}

/// `launchctl list` output → the units in scope. The header line and any
/// line that is not three tab-separated fields are skipped.
pub fn parse_launchctl_list(text: &str) -> Vec<ListedUnit> {
    text.lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() != 3 || !keep(parts[2]) {
                return None;
            }
            let pid = (parts[0] != "-").then(|| parts[0].to_string());
            let code = parts[1].trim();
            let last_exit_code = code.parse::<i64>().ok();
            Some(ListedUnit { pid, last_exit_code, label: parts[2].to_string() })
        })
        .collect()
}

/// What the unit's plist says about how it runs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnitMeta {
    /// a StartInterval or StartCalendarInterval makes it a scheduled job
    pub scheduled: bool,
    pub argv: Vec<String>,
    pub bundle_id: Option<String>,
}

/// The trees that are ours: a path in one of them is evidence of ownership.
pub const OUR_TREES: &[&str] = &["/Users/jeffbridwell/CascadeProjects/", "/Users/jeffbridwell/.chorus/"];
/// Label namespaces we minted: ours even when the binary is an app.
pub const OUR_LABELS: &[&str] = &["com.gathering.", "com.chorus.", "com.security."];

/// Ours-wins: our label namespace, or any argument in our trees. The full
/// argv, not argv0 — most of our units launch as /bin/bash <our script>.
pub fn is_ours(label: &str, meta: &UnitMeta) -> bool {
    OUR_LABELS.iter().any(|p| label.starts_with(p))
        || meta.argv.iter().any(|a| OUR_TREES.iter().any(|t| a.starts_with(t)))
}

/// External only on evidence: not ours, and some path or bundle id to show
/// for it. No evidence at all is unknowable, never external.
pub fn is_external(label: &str, meta: &UnitMeta) -> bool {
    !is_ours(label, meta) && (!meta.argv.is_empty() || meta.bundle_id.is_some())
}

/// The binary to record: the first argument in our trees (the script a
/// /bin/bash wrapper runs), else argv0.
pub fn binary_path(meta: &UnitMeta) -> Option<&str> {
    meta.argv
        .iter()
        .find(|a| OUR_TREES.iter().any(|t| a.starts_with(t)))
        .or_else(|| meta.argv.first())
        .map(|s| s.as_str())
}

/// The class a unit becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitClass {
    ServiceInstance,
    ScheduledJob,
}

pub fn class_of(meta: &UnitMeta) -> UnitClass {
    if meta.scheduled { UnitClass::ScheduledJob } else { UnitClass::ServiceInstance }
}

/// The row's subject, unchanged from #3870 so the port rewrites the same rows.
pub fn instance_iri(machine: &str, label: &str) -> String {
    format!("urn:chorus:instance-{machine}-{label}")
}

/// A JSON value, just enough to read plutil output and the mapping file.
/// The crate stays zero-dependency (#4173), so this is the whole parser.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(String),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(kv) => kv.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        if let Json::Str(s) = self { Some(s) } else { None }
    }
    /// Python truthiness, which is what the bash generator's `p.get(..) or`
    /// tested: empty string, empty list, false and null are all absent.
    pub fn truthy(&self) -> bool {
        match self {
            Json::Null => false,
            Json::Bool(b) => *b,
            Json::Num(n) => n.trim_start_matches('-').trim_start_matches('0').trim_start_matches('.').trim_start_matches('0') != "",
            Json::Str(s) => !s.is_empty(),
            Json::Arr(a) => !a.is_empty(),
            Json::Obj(o) => !o.is_empty(),
        }
    }
}

pub fn parse_json(text: &str) -> Result<Json, String> {
    let b: Vec<char> = text.chars().collect();
    let mut i = 0;
    let v = json_value(&b, &mut i)?;
    skip_ws(&b, &mut i);
    if i != b.len() {
        return Err(format!("trailing data at {i}"));
    }
    Ok(v)
}

fn skip_ws(b: &[char], i: &mut usize) {
    while *i < b.len() && b[*i].is_whitespace() {
        *i += 1;
    }
}

fn json_value(b: &[char], i: &mut usize) -> Result<Json, String> {
    skip_ws(b, i);
    match b.get(*i) {
        None => Err("unexpected end".into()),
        Some('{') => {
            *i += 1;
            let mut kv = Vec::new();
            skip_ws(b, i);
            if b.get(*i) == Some(&'}') {
                *i += 1;
                return Ok(Json::Obj(kv));
            }
            loop {
                skip_ws(b, i);
                let Json::Str(k) = json_value(b, i)? else { return Err(format!("object key at {i}")) };
                skip_ws(b, i);
                if b.get(*i) != Some(&':') {
                    return Err(format!("expected : at {i}"));
                }
                *i += 1;
                kv.push((k, json_value(b, i)?));
                skip_ws(b, i);
                match b.get(*i) {
                    Some(',') => *i += 1,
                    Some('}') => { *i += 1; return Ok(Json::Obj(kv)); }
                    _ => return Err(format!("expected , or }} at {i}")),
                }
            }
        }
        Some('[') => {
            *i += 1;
            let mut a = Vec::new();
            skip_ws(b, i);
            if b.get(*i) == Some(&']') {
                *i += 1;
                return Ok(Json::Arr(a));
            }
            loop {
                a.push(json_value(b, i)?);
                skip_ws(b, i);
                match b.get(*i) {
                    Some(',') => *i += 1,
                    Some(']') => { *i += 1; return Ok(Json::Arr(a)); }
                    _ => return Err(format!("expected , or ] at {i}")),
                }
            }
        }
        Some('"') => {
            *i += 1;
            let start = *i;
            while *i < b.len() && b[*i] != '"' {
                if b[*i] == '\\' {
                    *i += 1;
                }
                *i += 1;
            }
            if *i >= b.len() {
                return Err("unterminated string".into());
            }
            let raw: String = b[start..*i].iter().collect();
            *i += 1;
            Ok(Json::Str(crate::json_unescape(&raw)))
        }
        Some(_) => {
            let start = *i;
            while *i < b.len() && !matches!(b[*i], ',' | ']' | '}') && !b[*i].is_whitespace() {
                *i += 1;
            }
            let word: String = b[start..*i].iter().collect();
            match word.as_str() {
                "null" => Ok(Json::Null),
                "true" => Ok(Json::Bool(true)),
                "false" => Ok(Json::Bool(false)),
                w if w.parse::<f64>().is_ok() => Ok(Json::Num(w.to_string())),
                w => Err(format!("bad token {w:?}")),
            }
        }
    }
}

/// A plist (as `plutil -convert json` prints it) → how the unit runs. The
/// bash walk's classify(): ProgramArguments, else [Program]; a StartInterval
/// or StartCalendarInterval key makes it a scheduled job.
pub fn plist_meta(p: &Json) -> UnitMeta {
    let argv: Vec<String> = match p.get("ProgramArguments") {
        Some(Json::Arr(a)) if !a.is_empty() => a.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
        _ => p.get("Program").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(|s| vec![s.to_string()]).unwrap_or_default(),
    };
    UnitMeta {
        scheduled: p.get("StartCalendarInterval").is_some() || p.get("StartInterval").is_some(),
        argv,
        bundle_id: None,
    }
}

/// `launchctl print gui/<uid>/<label>` → (arguments, parent bundle id). App
/// bundle units (docker, ollama) have no plist in the three dirs, but print
/// still shows what they run — evidence, not guesswork.
pub fn parse_launchctl_print(text: &str) -> (Vec<String>, Option<String>) {
    let (mut argv, mut in_args, mut bundle) = (Vec::new(), false, None);
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("arguments = {") {
            in_args = true;
            continue;
        }
        if in_args {
            if t == "}" {
                in_args = false;
            } else if !t.is_empty() {
                argv.push(t.to_string());
            }
        }
        if let Some(rest) = t.strip_prefix("parent bundle identifier = ") {
            if let Some(id) = rest.split_whitespace().next() {
                bundle = Some(id.to_string());
            }
        }
    }
    (argv, bundle)
}

/// One observed unit, ready to become a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    pub label: String,
    pub run_state: String,
    pub meta: UnitMeta,
    /// remote unit whose plist we could not read: "we could not look"
    pub evidence_unavailable: bool,
}

/// What one machine's walk returned: its units, or why it could not be walked.
pub type MachineWalk = Result<Vec<Unit>, String>;

/// The label→Service mapping file, `_`-keys (comments) dropped.
pub fn parse_mapping(text: &str) -> Result<Vec<(String, String)>, String> {
    match parse_json(text)? {
        Json::Obj(kv) => Ok(kv
            .into_iter()
            .filter(|(k, _)| !k.starts_with('_'))
            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
            .collect()),
        _ => Err("mapping is not an object".into()),
    }
}

/// Service IRIs authored in the services TTL (`chorus:service-x a chorus:Service`).
pub fn authored_services(ttl: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (pos, _) in ttl.match_indices("chorus:service-") {
        let rest = &ttl[pos + "chorus:".len()..];
        let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
        // the bash rule, regex `chorus:(service-…)\s+a\s+chorus:Service`:
        // whitespace (newlines too) on both sides of `a`
        let tail = &rest[name.len()..];
        let after = tail.trim_start();
        if after.len() == tail.len() || !after.starts_with('a') {
            continue;
        }
        let obj = after[1..].trim_start();
        // whole word: chorus:ServiceInstance is not chorus:Service (the bash
        // regex had no boundary; no authored row hits the difference today)
        let word_ends = !obj["chorus:Service".len().min(obj.len())..].starts_with(|c: char| c.is_ascii_alphanumeric());
        if obj.len() < after.len() - 1 && obj.starts_with("chorus:Service") && word_ends {
            out.push(name);
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Mapping targets that are not authored Services — the generator refuses.
pub fn unknown_targets(mapping: &[(String, String)], authored: &[String]) -> Vec<String> {
    let mut bad: Vec<String> = mapping.iter().map(|(_, s)| s.clone()).filter(|s| !authored.contains(s)).collect();
    bad.sort();
    bad.dedup();
    bad
}

#[cfg(test)]
mod render_4472 {
    use super::*;

    #[test]
    fn json_reads_plutil_output_and_the_mapping_comment_escapes() {
        let p = parse_json(r#"{"Label":"com.x","ProgramArguments":["/bin/bash","/a b"],"StartInterval":300,"KeepAlive":true}"#).unwrap();
        let m = plist_meta(&p);
        assert!(m.scheduled);
        assert_eq!(m.argv, ["/bin/bash", "/a b"]);
        let map = parse_mapping("{\"_comment\": \"a \\u2014 b\", \"com.security.css\": \"service-identity\"}").unwrap();
        assert_eq!(map, [("com.security.css".to_string(), "service-identity".to_string())]);
        // NEGATIVE PROOF: malformed JSON is an error, never an empty mapping
        assert!(parse_mapping("{\"a\": ").is_err());
        assert!(parse_json("[1,2] x").is_err());
    }

    #[test]
    fn program_is_the_argv_when_there_are_no_program_arguments() {
        let p = parse_json(r#"{"Program":"/usr/sbin/sshd","ProgramArguments":[]}"#).unwrap();
        assert_eq!(plist_meta(&p).argv, ["/usr/sbin/sshd"]);
        assert!(!plist_meta(&p).scheduled);
    }

    #[test]
    fn launchctl_print_yields_arguments_and_bundle() {
        let t = "gui/501/com.docker.helper = {\n\tparent bundle identifier = com.docker.docker\n\targuments = {\n\t\t/Applications/Docker.app/x\n\t\t--flag\n\t}\n}\n";
        let (argv, b) = parse_launchctl_print(t);
        assert_eq!(argv, ["/Applications/Docker.app/x", "--flag"]);
        assert_eq!(b.as_deref(), Some("com.docker.docker"));
    }

    #[test]
    fn mapping_targets_must_be_authored_services() {
        let ttl = "chorus:service-identity a chorus:Service ;\nchorus:service-clearing\n    a chorus:Service .\nchorus:service-x a chorus:ServiceInstance .\n";
        let a = authored_services(ttl);
        // a newline between subject and `a` still counts (the bash regex is \s+)
        assert_eq!(a, ["service-clearing", "service-identity"]);
        let m = vec![("l".to_string(), "service-identity".to_string()), ("k".to_string(), "service-nope".to_string())];
        // NEGATIVE PROOF: an unknown target is named, so the run refuses
        assert_eq!(unknown_targets(&m, &a), ["service-nope"]);
    }
}

/// The door's row name for a unit. The door takes `[A-Za-z0-9_-]` only
/// (athena-make is_safe_local), so the #3870 IRI `urn:chorus:instance-…`
/// with its dots cannot be addressed there at all: dots become hyphens.
pub fn door_name(machine: &str, label: &str) -> String {
    let n: String = format!("instance-{machine}-{label}")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    n.chars().take(128).collect()
}

/// One row as the door holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoorRow {
    pub class: UnitClass,
    pub name: String,
    /// door field name → value, `name` excluded
    pub fields: Vec<(String, String)>,
}

impl DoorRow {
    pub fn get(&self, k: &str) -> &str {
        self.fields.iter().find(|(f, _)| f == k).map(|(_, v)| v.as_str()).unwrap_or("")
    }
}

/// The fields this crawl owns. Anything else on a row (deployedCommit,
/// cdhash, lastRunAt…) belongs to another writer and is put back untouched.
pub const OWNED_FIELDS: &[&str] = &["label", "launchdLabel", "onMachine", "runState", "binaryPath", "runsService", "external", "evidenceState"];

/// The row a unit should be: the bash generator's triples, as door fields.
pub fn desired_row(machine: &str, u: &Unit, mapping: &[(String, String)], ts: &str) -> DoorRow {
    let class = class_of(&u.meta);
    let mut f: Vec<(String, String)> = vec![
        ("label".into(), format!("{} ({machine})", u.label)),
        ("launchdLabel".into(), u.label.clone()),
        ("onMachine".into(), machine.into()),
    ];
    if class == UnitClass::ServiceInstance {
        f.push(("runState".into(), u.run_state.clone()));
    }
    if let Some(bp) = binary_path(&u.meta) {
        f.push(("binaryPath".into(), bp.into()));
    }
    if let Some((_, svc)) = mapping.iter().find(|(k, _)| *k == u.label) {
        f.push(("runsService".into(), svc.clone()));
    }
    if is_external(&u.label, &u.meta) {
        f.push(("external".into(), "true".into()));
    } else if u.evidence_unavailable && !is_ours(&u.label, &u.meta) {
        f.push(("evidenceState".into(), "unknown".into()));
    }
    f.push(("lastObserved".into(), ts.into()));
    DoorRow { class, name: door_name(machine, &u.label), fields: f }
}

/// What one run writes. Each list is named in the report.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub create: Vec<DoorRow>,
    /// full rows to PUT: the current row with our fields laid over it
    pub update: Vec<DoorRow>,
    /// job rows on a walked machine that no unit backs any more
    pub retire: Vec<DoorRow>,
    /// labels of instance rows no unit backs: kept, runState "absent"
    pub vanished: Vec<String>,
    pub unchanged: usize,
    /// two labels that share one door name — refused, never merged
    pub collisions: Vec<String>,
}

/// lastObserved alone is refreshed only once it is half way to the drift
/// check's 24 h "stale" line, so an unchanged world writes nothing most runs
/// and never reads as stale.
pub const REFRESH_AFTER_SECS: u64 = 12 * 3600;

/// Desired rows vs the rows the door holds → the writes. `walked` names the
/// machines this run actually saw: a machine whose walk failed keeps every
/// row it had (the bash load deleted them all, because its replace was
/// class-wide).
pub fn plan(desired: Vec<DoorRow>, current: &[DoorRow], walked: &[&str], now_secs: u64, iso_secs: &dyn Fn(&str) -> Option<u64>) -> Plan {
    let mut p = Plan::default();
    let mut seen: Vec<(String, String)> = Vec::new();
    for d in desired {
        if let Some((_, other)) = seen.iter().find(|(n, _)| *n == d.name) {
            p.collisions.push(format!("{} and {} both name {}", other, d.get("launchdLabel"), d.name));
            continue;
        }
        seen.push((d.name.clone(), d.get("launchdLabel").to_string()));
        match current.iter().find(|c| c.name == d.name && c.class == d.class) {
            None => p.create.push(d),
            Some(c) => {
                let differs = OWNED_FIELDS.iter().any(|k| c.get(k) != d.get(k));
                let old = iso_secs(c.get("lastObserved")).map_or(true, |t| now_secs.saturating_sub(t) >= REFRESH_AFTER_SECS);
                if differs || old {
                    let mut fields: Vec<(String, String)> = c
                        .fields
                        .iter()
                        .filter(|(k, v)| !v.is_empty() && *k != "lastObserved" && !OWNED_FIELDS.contains(&k.as_str()))
                        .cloned()
                        .collect();
                    fields.extend(d.fields.iter().filter(|(_, v)| !v.is_empty()).cloned());
                    p.update.push(DoorRow { class: d.class, name: d.name, fields });
                } else {
                    p.unchanged += 1;
                }
            }
        }
    }
    for c in current {
        if !walked.contains(&c.get("onMachine")) || seen.iter().any(|(n, _)| *n == c.name) {
            continue;
        }
        // A vanished ServiceInstance keeps its row as runState "absent" (the
        // shape's red line: deleting it would blind the report to
        // disappearances); lastObserved keeps when it was last seen. A job has
        // no runState to say it with, so its row is retired, by name.
        if c.class == UnitClass::ServiceInstance {
            if c.get("runState") != "absent" {
                let mut r = c.clone();
                r.fields.retain(|(k, v)| k != "runState" && !v.is_empty());
                r.fields.push(("runState".into(), "absent".into()));
                p.update.push(r);
            }
            p.vanished.push(c.get("label").to_string());
        } else {
            p.retire.push(c.clone());
        }
    }
    p
}

/// `YYYY-MM-DDTHH:MM:SSZ` → unix seconds (the inverse of iso_from_secs).
/// Anything else is None, and the planner treats None as "refresh".
pub fn secs_from_iso(s: &str) -> Option<u64> {
    let b = s.as_bytes();
    if b.len() != 20 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' || b[19] != b'Z' {
        return None;
    }
    let n = |r: std::ops::Range<usize>| s.get(r).and_then(|x| x.parse::<i64>().ok());
    let (y, m, d, hh, mm, ss) = (n(0..4)?, n(5..7)?, n(8..10)?, n(11..13)?, n(14..16)?, n(17..19)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || hh > 23 || mm > 59 || ss > 60 {
        return None;
    }
    // days from civil (Howard Hinnant)
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    u64::try_from(days * 86400 + hh * 3600 + mm * 60 + ss).ok()
}

/// What the report calls a finding: each one makes the run exit 1 (AC5).
pub fn findings(desired: &[DoorRow], plan: &Plan, mapping: &[(String, String)], unwalked: &[(String, String)]) -> Vec<String> {
    let mut f = Vec::new();
    for (m, why) in unwalked {
        f.push(format!("unwalked: {m} could not be read ({why}) — its rows were left as they were"));
    }
    for c in &plan.collisions {
        f.push(format!("collision: {c}"));
    }
    for d in desired {
        if d.class == UnitClass::ServiceInstance && d.get("runsService").is_empty() && d.get("external").is_empty() && d.get("evidenceState").is_empty() {
            f.push(format!("unmapped: {} — ours, and no Service claims it", d.get("label")));
        }
    }
    let observed: Vec<&str> = desired.iter().map(|d| d.get("launchdLabel")).collect();
    let mut stale: Vec<&(String, String)> = mapping.iter().filter(|(k, _)| !observed.contains(&k.as_str())).collect();
    stale.sort();
    for (label, svc) in stale {
        f.push(format!("stale-mapping: {label} maps to {svc} but no instance was observed"));
    }
    for r in &plan.retire {
        f.push(format!("retired: {} — no unit backs it any more", r.get("label")));
    }
    for v in &plan.vanished {
        f.push(format!("vanished: {v} — was observed, now absent"));
    }
    f
}

/// Rows the door holds whose lastObserved is older than the drift line
/// (24 h) — a crawl that stopped running shows up here, not as quiet.
pub fn stale_rows(current: &[DoorRow], now_secs: u64, iso_secs: &dyn Fn(&str) -> Option<u64>) -> Vec<String> {
    current
        .iter()
        .filter(|c| c.get("runState") != "absent")
        .filter(|c| iso_secs(c.get("lastObserved")).map_or(true, |t| now_secs.saturating_sub(t) > 24 * 3600))
        .map(|c| format!("stale: {} last observed {}", c.get("label"), c.get("lastObserved")))
        .collect()
}

#[cfg(test)]
mod parity_4472 {
    use super::*;

    const LIST: &str = "PID\tStatus\tLabel\n\
        4045\t0\tcom.chorus.athena-make\n\
        -\t0\tcom.chorus.nightly-suites\n\
        -\t-9\tcom.gathering.fuseki\n\
        512\t0\tcom.apple.Finder\n\
        77\t0\tcom.microsoft.update.agent\n\
        garbage line\n";

    #[test]
    fn launchctl_list_keeps_non_apple_com_units_with_their_state() {
        let u = parse_launchctl_list(LIST);
        let labels: Vec<&str> = u.iter().map(|x| x.label.as_str()).collect();
        assert_eq!(labels, ["com.chorus.athena-make", "com.chorus.nightly-suites", "com.gathering.fuseki", "com.microsoft.update.agent"]);
        assert_eq!(u[0].run_state(), "running");
        assert_eq!(u[1].run_state(), "loaded");
        assert_eq!(u[2].last_exit_code, Some(-9));
        // NEGATIVE PROOF: Apple units and malformed lines never become rows
        assert!(!labels.iter().any(|l| l.starts_with("com.apple")));
    }

    fn meta(argv: &[&str]) -> UnitMeta {
        UnitMeta { scheduled: false, argv: argv.iter().map(|s| s.to_string()).collect(), bundle_id: None }
    }

    #[test]
    fn ownership_reads_the_whole_argv_and_our_label_namespaces() {
        // a /bin/bash wrapper around our script is ours (argv0-only got this wrong in 2026-08)
        let wrapped = meta(&["/bin/bash", "/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/x.sh"]);
        assert!(is_ours("com.example.thing", &wrapped));
        assert!(!is_external("com.example.thing", &wrapped));
        // our label namespace wins even for an app binary
        assert!(is_ours("com.gathering.ollama", &meta(&["/Applications/Ollama.app/Contents/MacOS/ollama"])));
        // NEGATIVE PROOF: a foreign path with no claim of ours is external
        assert!(is_external("com.microsoft.update.agent", &meta(&["/Library/Application Support/Microsoft/agent"])));
        // and no evidence at all is unknowable, never external
        assert!(!is_external("com.unknown.thing", &UnitMeta::default()));
    }

    #[test]
    fn binary_is_our_script_not_the_shell_that_runs_it() {
        let m = meta(&["/bin/bash", "/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/tmp-reaper.sh"]);
        assert_eq!(binary_path(&m), Some("/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/tmp-reaper.sh"));
        assert_eq!(binary_path(&meta(&["/opt/homebrew/bin/dagu", "start-all"])), Some("/opt/homebrew/bin/dagu"));
        assert_eq!(binary_path(&UnitMeta::default()), None);
    }

    #[test]
    fn a_scheduled_unit_is_a_job_and_the_iri_is_unchanged_from_3870() {
        let mut m = UnitMeta::default();
        assert_eq!(class_of(&m), UnitClass::ServiceInstance);
        m.scheduled = true;
        assert_eq!(class_of(&m), UnitClass::ScheduledJob);
        assert_eq!(instance_iri("library", "com.chorus.dagu"), "urn:chorus:instance-library-com.chorus.dagu");
    }
}

#[cfg(test)]
mod plan_4472 {
    use super::*;

    fn row(class: UnitClass, name: &str, f: &[(&str, &str)]) -> DoorRow {
        DoorRow { class, name: name.into(), fields: f.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect() }
    }
    fn iso(s: &str) -> Option<u64> {
        s.strip_prefix("T").and_then(|n| n.parse().ok())
    }
    fn unit(label: &str) -> Unit {
        Unit { label: label.into(), run_state: "running".into(), meta: UnitMeta { scheduled: false, argv: vec!["/Users/jeffbridwell/.chorus/bin/x".into()], bundle_id: None }, evidence_unavailable: false }
    }

    #[test]
    fn door_names_have_no_dots_and_fit_the_door_rule() {
        assert_eq!(door_name("library", "com.chorus.api"), "instance-library-com-chorus-api");
        let long = door_name("library", &"x".repeat(300));
        assert_eq!(long.len(), 128);
    }

    #[test]
    fn an_unchanged_world_writes_nothing() {
        let d = desired_row("library", &unit("com.chorus.api"), &[], "T1000");
        let cur = vec![DoorRow { fields: d.fields.iter().map(|(k, v)| (k.clone(), if k == "lastObserved" { "T990".into() } else { v.clone() })).collect(), ..d.clone() }];
        let p = plan(vec![d], &cur, &["library"], 1000, &iso);
        assert_eq!((p.create.len(), p.update.len(), p.retire.len(), p.unchanged), (0, 0, 0, 1));
    }

    #[test]
    fn a_changed_field_puts_the_whole_row_back_with_other_writers_fields() {
        let d = desired_row("library", &unit("com.chorus.api"), &[], "T1000");
        let mut cur = d.clone();
        cur.fields.retain(|(k, _)| k != "runState");
        cur.fields.push(("runState".into(), "loaded".into()));
        cur.fields.push(("deployedCommit".into(), "abc".into()));
        cur.fields.push(("cdhash".into(), "".into()));
        let p = plan(vec![d], &[cur], &["library"], 1000, &iso);
        assert_eq!(p.update.len(), 1);
        let u = &p.update[0];
        assert_eq!(u.get("runState"), "running");
        // NEGATIVE PROOF: the full-replace PUT must not drop another writer's field
        assert_eq!(u.get("deployedCommit"), "abc");
        assert!(!u.fields.iter().any(|(k, _)| k == "cdhash"), "empty fields are not written back");
    }

    #[test]
    fn last_observed_alone_refreshes_only_after_twelve_hours() {
        let d = desired_row("library", &unit("com.chorus.api"), &[], "T50000");
        let mut cur = d.clone();
        for (k, v) in cur.fields.iter_mut() {
            if k == "lastObserved" { *v = "T0".into(); }
        }
        assert_eq!(plan(vec![d.clone()], &[cur.clone()], &["library"], 50000, &iso).update.len(), 1);
        assert_eq!(plan(vec![d], &[cur], &["library"], 3600, &iso).unchanged, 1);
    }

    #[test]
    fn a_vanished_instance_turns_absent_a_vanished_job_is_retired() {
        let gone = row(UnitClass::ServiceInstance, "instance-library-com-old", &[("onMachine", "library"), ("label", "com.old (library)"), ("runState", "running"), ("lastObserved", "T5")]);
        let job = row(UnitClass::ScheduledJob, "instance-library-com-job", &[("onMachine", "library")]);
        let bed = row(UnitClass::ServiceInstance, "instance-bedroom-com-x", &[("onMachine", "bedroom"), ("runState", "running")]);
        let p = plan(vec![], &[gone, job, bed], &["library"], 0, &iso);
        assert_eq!(p.update.len(), 1);
        assert_eq!(p.update[0].get("runState"), "absent");
        assert_eq!(p.update[0].get("lastObserved"), "T5", "keeps when it was last seen");
        assert_eq!(p.vanished, ["com.old (library)"]);
        assert_eq!(p.retire.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["instance-library-com-job"]);
        // NEGATIVE PROOF: bedroom was not walked, so its row is not touched
        assert!(!p.update.iter().chain(p.retire.iter()).any(|r| r.name.contains("bedroom")));
        // and an already-absent row is not rewritten every run
        let absent = row(UnitClass::ServiceInstance, "instance-library-com-old", &[("onMachine", "library"), ("runState", "absent")]);
        assert!(plan(vec![], &[absent], &["library"], 0, &iso).update.is_empty());
    }

    #[test]
    fn a_row_unseen_for_a_day_is_stale_an_absent_one_is_not() {
        let fresh = row(UnitClass::ServiceInstance, "a", &[("label", "a"), ("lastObserved", "T90000")]);
        let old = row(UnitClass::ServiceInstance, "b", &[("label", "b"), ("lastObserved", "T0")]);
        let gone = row(UnitClass::ServiceInstance, "c", &[("label", "c"), ("lastObserved", "T0"), ("runState", "absent")]);
        assert_eq!(stale_rows(&[fresh, old, gone], 100000, &iso), ["stale: b last observed T0"]);
    }

    #[test]
    fn evidence_rules_carry_into_rows() {
        let bundle_only = Unit { label: "com.docker.helper".into(), run_state: "running".into(), meta: UnitMeta { scheduled: false, argv: vec![], bundle_id: Some("com.docker.docker".into()) }, evidence_unavailable: false };
        assert_eq!(desired_row("library", &bundle_only, &[], "T").get("external"), "true");
        let unseen = Unit { label: "com.x.y".into(), run_state: "loaded".into(), meta: UnitMeta::default(), evidence_unavailable: true };
        let r = desired_row("bedroom", &unseen, &[], "T");
        assert_eq!(r.get("evidenceState"), "unknown");
        assert_eq!(r.get("external"), "");
        // NEGATIVE PROOF: a wrapper around OUR script is ours (the css case)
        let css = Unit { label: "com.security.css".into(), run_state: "running".into(), meta: UnitMeta { scheduled: false, argv: vec!["/bin/bash".into(), "/Users/jeffbridwell/CascadeProjects/x/start.sh".into()], bundle_id: None }, evidence_unavailable: false };
        assert_eq!(desired_row("library", &css, &[], "T").get("external"), "");
    }

    #[test]
    fn two_labels_that_share_a_door_name_are_refused() {
        let a = desired_row("library", &unit("com.a-b"), &[], "T0");
        let b = desired_row("library", &unit("com.a.b"), &[], "T0");
        let p = plan(vec![a, b], &[], &["library"], 0, &iso);
        assert_eq!(p.create.len(), 1);
        assert_eq!(p.collisions.len(), 1, "{:?}", p.collisions);
    }
}

#[cfg(test)]
mod report_4472 {
    use super::*;

    #[test]
    fn iso_round_trips_with_the_crate_clock() {
        for t in [0u64, 951782400, 1760054400, 4102444799] {
            assert_eq!(secs_from_iso(&crate::iso_from_secs(t)), Some(t));
        }
        // NEGATIVE PROOF: a malformed stamp is None (refresh), never epoch 0
        assert_eq!(secs_from_iso("2026-10-09 21:00:00"), None);
        assert_eq!(secs_from_iso("2026-13-09T21:00:00Z"), None);
    }

    #[test]
    fn findings_name_unmapped_stale_retired_and_unwalked() {
        let u = |l: &str, ext: bool| Unit { label: l.into(), run_state: "running".into(), meta: UnitMeta { scheduled: false, argv: vec![if ext { "/opt/x".into() } else { "/Users/jeffbridwell/.chorus/bin/x".into() }], bundle_id: None }, evidence_unavailable: false };
        let mapping = vec![("com.chorus.mapped".to_string(), "service-a".to_string()), ("com.gone".to_string(), "service-b".to_string())];
        let desired = vec![
            desired_row("library", &u("com.chorus.mapped", false), &mapping, "T"),
            desired_row("library", &u("com.chorus.orphan", false), &mapping, "T"),
            desired_row("library", &u("com.other.app", true), &mapping, "T"),
        ];
        let mut p = Plan::default();
        p.retire.push(DoorRow { class: UnitClass::ServiceInstance, name: "n".into(), fields: vec![("label".into(), "com.old (library)".into())] });
        let f = findings(&desired, &p, &mapping, &[("bedroom".into(), "ssh timeout".into())]);
        assert!(f.iter().any(|x| x == "unmapped: com.chorus.orphan (library) — ours, and no Service claims it"), "{f:?}");
        assert!(f.iter().any(|x| x.starts_with("stale-mapping: com.gone")), "{f:?}");
        assert!(f.iter().any(|x| x.starts_with("retired: com.old")), "{f:?}");
        assert!(f.iter().any(|x| x.starts_with("unwalked: bedroom")), "{f:?}");
        // NEGATIVE PROOF: mapped and external units are not findings
        assert!(!f.iter().any(|x| x.contains("com.chorus.mapped") || x.contains("com.other.app")), "{f:?}");
        assert_eq!(f.len(), 4);
    }
}
