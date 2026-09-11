//! #4145 — the nightly run, whole, in the runner. Jeff, 2026-09-11 16:53:
//! "delete the nightly script, change the launchd". Until this module the 03:00
//! job was an 1,833-line bash wrapper (`nightly-suites.sh --run-all`) around
//! `werk-test --nightly`: it ran the pre-checks, folded the runner's unit lines
//! into the `SUITE|` rows the /nightly page reads, looked owners up by
//! re-reading the registry ONCE PER ROW (385 reads, 11 min on 2026-09-11),
//! walked the whole results ledger for the census (27 pages, 7 min), then
//! nudged, recorded and read out. These are the PURE halves of that work —
//! every fold, classification, message and census is a function of its inputs
//! and proven here; the orchestration (processes, files, clocks) lives in the
//! binary's `nightly_all` module.
//!
//! The output contract is unchanged on purpose: the page, the daily readers
//! and the readout API parse `RUN|start|<ts>|pid=N`, `RUN|complete|<ts>|suites=N`,
//! `RUN|stopped|…`, `RUN|unmeasurable|…` and `SUITE|kind|path|owner|status|summary`.

use std::collections::HashMap;

/// One row of the run as the page reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuiteRow {
    pub kind: String,
    pub path: String,
    pub owner: String,
    pub status: String,
    pub summary: String,
}

impl SuiteRow {
    pub fn new(kind: &str, path: &str, owner: &str, status: &str, summary: &str) -> Self {
        SuiteRow {
            kind: kind.into(),
            path: path.into(),
            owner: owner.into(),
            status: status.into(),
            summary: summary.into(),
        }
    }
    pub fn line(&self) -> String {
        format!("SUITE|{}|{}|{}|{}|{}", self.kind, self.path, self.owner, self.status, self.summary)
    }
    pub fn suite_name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }
}

/// `SUITE|…` back into a row (the daily readers and the tests round-trip it).
pub fn parse_suite_line(line: &str) -> Option<SuiteRow> {
    let mut it = line.splitn(6, '|');
    if it.next()? != "SUITE" {
        return None;
    }
    Some(SuiteRow::new(it.next()?, it.next()?, it.next()?, it.next()?, it.next().unwrap_or("")))
}

fn first_count(summary: &str, words: &[&str]) -> Option<usize> {
    let toks: Vec<&str> = summary.split(|c: char| !c.is_ascii_alphanumeric()).filter(|t| !t.is_empty()).collect();
    for w in toks.windows(2) {
        if words.contains(&w[1]) {
            if let Ok(n) = w[0].parse::<usize>() {
                return Some(n);
            }
        }
    }
    None
}

/// The wrapper's `_classify_verdict`, ported. A row that says pass while its
/// summary counts failures is a REPORTER CONTRADICTION and records fail
/// (#3753 AC4). A fail whose summary names an environment that never ran the
/// suite (missing node module, command not found, DID NOT RUN) is unmeasurable;
/// a suite timeout under load is unmeasurable, over an idle box it is a fail.
pub fn classify_verdict(verdict: &str, summary: &str, box_over_load: bool) -> (String, bool) {
    if verdict == "pass" {
        if let Some(f) = first_count(summary, &["failed", "fail"]) {
            if f > 0 {
                return ("fail".into(), true);
            }
        }
    }
    if verdict != "fail" {
        return (verdict.to_string(), false);
    }
    let env_never_ran = ["NODE_MODULE_VERSION", "Cannot find module", "command not found", "ERR_DLOPEN_FAILED", "DID NOT RUN"]
        .iter()
        .any(|m| summary.contains(m))
        || (summary.contains("spawn ") && summary.contains("ENOENT"));
    if env_never_ran {
        return ("unmeasurable".into(), false);
    }
    if (summary.contains("SUITE TIMEOUT") || summary.contains("rc=124")) && box_over_load {
        return ("unmeasurable".into(), false);
    }
    ("fail".into(), false)
}

/// The wrapper's `_remap_unmeasured` (#4009/#4013): "0 pass, 0 fail" with no
/// parenthesised state of its own was never measured.
pub fn remap_unmeasured(verdict: &str, summary: &str) -> (String, String) {
    let bare = summary == "0 pass, 0 fail" || summary.starts_with("0 pass, 0 fail ");
    if bare && !(summary.contains('(') && summary.contains(')')) {
        return ("unmeasured".into(), "0 pass, 0 fail (UNMEASURED — suite produced no parseable output)".into());
    }
    (verdict.to_string(), summary.to_string())
}

/// The wrapper's `_owner_path_rule` — the fallback when the model has no owner
/// for a path. Kept exactly.
pub fn owner_path_rule(path: &str, chorus_root: &str, app_root: &str) -> &'static str {
    let rel = path.strip_prefix(&format!("{}/", chorus_root)).unwrap_or(path);
    if path == app_root || path.starts_with(&format!("{}/", app_root)) {
        return "kade";
    }
    if rel.starts_with("directing/") || rel.starts_with("roles/kade/") {
        return "kade";
    }
    if rel.starts_with("roles/wren/") || rel.starts_with("platform/services/athena-") {
        return "wren";
    }
    if rel.starts_with("roles/silas/")
        || rel.starts_with("platform/services/werk-")
        || rel.starts_with("platform/services/chorus-")
        || rel.starts_with("platform/scripts/")
        || rel.starts_with("proving/")
    {
        return "silas";
    }
    "unowned"
}

/// The owner map, built ONCE per run: registry rows give file → covers (a
/// domain name); domains give name → ownedBy ("role-silas" → "silas"). The
/// wrapper rebuilt this from two HTTP reads on every row because its cache
/// variable lived inside a `$(…)` subshell (385 reads on 2026-09-11).
pub fn owner_map<'a>(
    tests: impl Iterator<Item = (&'a str, &'a str)>,
    domains: impl Iterator<Item = (&'a str, &'a str)>,
) -> HashMap<String, String> {
    let own: HashMap<&str, &str> = domains
        .filter(|(n, o)| !n.is_empty() && !o.is_empty())
        .map(|(n, o)| (n, o.strip_prefix("role-").unwrap_or(o)))
        .collect();
    let mut m = HashMap::new();
    for (fp, cov) in tests {
        if let Some(o) = own.get(cov) {
            m.insert(fp.to_string(), (*o).to_string());
        }
    }
    m
}

pub fn owner_for(path: &str, map: &HashMap<String, String>, chorus_root: &str, app_root: &str) -> String {
    let rel = path.strip_prefix(&format!("{}/", chorus_root)).unwrap_or(path);
    map.get(rel).cloned().unwrap_or_else(|| owner_path_rule(path, chorus_root, app_root).to_string())
}

/// A runner `nightly-unit|kind|unit|verdict|summary` line folded to the page's
/// row. A cargo unit's path is its crate directory; a perf row that fails is
/// SLOW, not red (#4136).
pub fn fold_unit_line(line: &str, owner: &dyn Fn(&str) -> String, box_over_load: bool) -> Option<(SuiteRow, bool)> {
    let mut it = line.splitn(5, '|');
    if it.next()? != "nightly-unit" {
        return None;
    }
    let kind = it.next()?;
    let unit = it.next()?;
    let verdict = it.next()?;
    let summary = it.next().unwrap_or("");
    if unit.is_empty() {
        return None;
    }
    let path = if kind == "cargo" { format!("platform/services/{}", unit) } else { unit.to_string() };
    let (v, contradiction) = classify_verdict(verdict, summary, box_over_load);
    let (mut v, s) = remap_unmeasured(&v, summary);
    if kind == "perf" && v == "fail" {
        v = "slow".into();
    }
    Some((SuiteRow::new(kind, &path, &owner(&path), &v, &s), contradiction))
}

/// `nightly-case|filePath|testName` — one per case the runner joined and
/// posted this run. The census below is the registry minus these, computed
/// here from the run's own record instead of a 27-page walk of the ledger.
pub fn parse_case_line(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("nightly-case|")?;
    let (f, n) = rest.split_once('|')?;
    if f.is_empty() || n.is_empty() {
        return None;
    }
    Some((f.to_string(), n.to_string()))
}

/// coverage-floors.yml: `ts:` / `rust:` sections, `  <rel>: <floor>` entries,
/// `#` comments anywhere. Returns (lang, rel, floor).
pub fn parse_floors(yaml: &str) -> Vec<(String, String, u32)> {
    let mut out = Vec::new();
    let mut section = String::new();
    for raw in yaml.lines() {
        let line = raw.split('#').next().unwrap_or("").trim_end();
        if line.trim().is_empty() {
            continue;
        }
        if !line.starts_with(' ') {
            if let Some(s) = line.strip_suffix(':') {
                section = s.trim().to_string();
            }
            continue;
        }
        if section.is_empty() {
            continue;
        }
        if let Some((rel, floor)) = line.trim().split_once(':') {
            if let Ok(f) = floor.trim().parse::<u32>() {
                out.push((section.clone(), rel.trim().to_string(), f));
            }
        }
    }
    out
}

/// The coverage row, the wrapper's four outcomes kept verbatim.
pub fn coverage_row(rel: &str, owner: &str, floor: u32, rc: i32, pct: Option<f64>) -> SuiteRow {
    let (status, summary) = match (rc, pct) {
        (0, Some(p)) if p >= floor as f64 => ("pass", format!("1 pass, 0 fail (coverage {}% >= floor {}%)", trim_pct(p), floor)),
        (0, Some(p)) => ("fail", format!("0 pass, 1 fail (coverage {}% < floor {}%)", trim_pct(p), floor)),
        (0, None) => ("fail", format!("0 pass, 1 fail (coverage ran but produced NO summary artifact — expected floor {}%, got nothing)", floor)),
        (rc, _) => ("fail", format!("0 pass, 1 fail (coverage run errored rc={} — floor {}%, no clean measurement)", rc, floor)),
    };
    SuiteRow::new("coverage", rel, owner, status, &summary)
}

fn trim_pct(p: f64) -> String {
    let s = format!("{}", p);
    s
}

/// The coverage-floor ratchet (#4012): crates without a floor may only shrink.
/// Returns the row and the baseline to persist when the count went down.
pub fn denominator_row(configured: usize, present: usize, unconfigured: &[String], baseline: usize) -> (SuiteRow, Option<usize>) {
    let n = unconfigured.len();
    if n > baseline {
        let row = SuiteRow::new(
            "coverage-denominator",
            "platform/services",
            "kade",
            "fail",
            &format!(
                "0 pass, 1 fail (coverage-floor RATCHET DRIFTED: {} unconfigured vs baseline {} — a crate shipped without a coverage floor: {})",
                n, baseline, unconfigured.join(" ")
            ),
        );
        return (row, None);
    }
    let row = SuiteRow::new(
        "coverage-denominator",
        "platform/services",
        "kade",
        "pass",
        &format!(
            "1 pass, 0 fail ({} of {} crates have a floor; {} unconfigured vs baseline {} — standing gap, not drift)",
            configured, present, n, baseline
        ),
    );
    (row, if n < baseline { Some(n) } else { None })
}

/// The wrapper's `notify_results`, as data: (recipient, message) in the order
/// the wrapper sent them. All green → one line to the nightly owner. Reds →
/// one grouped line per owner, the security lane's own line when it has reds,
/// then the TOTAL line with slow rows named as speed, not breakage (#4136).
pub fn notify_messages(rows: &[SuiteRow], security_owner: &str) -> Vec<(String, String)> {
    let skipped = rows.iter().filter(|r| r.status == "skip").count();
    let skipmsg = if skipped > 0 { format!(" — {} skipped (no live stack, #3557)", skipped) } else { String::new() };
    let reds: Vec<&SuiteRow> = rows.iter().filter(|r| r.status == "fail").collect();
    let mut out = Vec::new();
    if reds.is_empty() {
        out.push(("kade".to_string(), format!("nightly: all hermetic suites green ✅{}", skipmsg)));
        return out;
    }
    let sec: Vec<&str> = reds.iter().filter(|r| r.kind == "security").map(|r| r.suite_name()).collect();
    if !sec.is_empty() {
        out.push((
            security_owner.to_string(),
            format!("SECURITY lane: {} red — {} (#3922 — own cadence, own signal)", sec.len(), sec.join(", ")),
        ));
    }
    let mut owners: Vec<&str> = reds.iter().map(|r| r.owner.as_str()).collect();
    owners.sort_unstable();
    owners.dedup();
    for o in &owners {
        let names: Vec<&str> = reds.iter().filter(|r| r.owner == *o).map(|r| r.suite_name()).collect();
        out.push((o.to_string(), format!("nightly: {} suite(s) red — {}", names.len(), names.join(", "))));
    }
    let per_owner: Vec<String> = owners.iter().map(|o| format!("{} {}", o, reds.iter().filter(|r| r.owner == *o).count())).collect();
    let slow: Vec<&str> = rows.iter().filter(|r| r.status == "slow").map(|r| r.suite_name()).collect();
    let slowmsg = if slow.is_empty() {
        String::new()
    } else {
        format!(" — {} slow (speed, not breakage: {})", slow.len(), slow.join(", "))
    };
    out.push((
        "kade".to_string(),
        format!("nightly TOTAL: {} red across the board ({}) — bar is zero{}{}", reds.len(), per_owner.join(", "), skipmsg, slowmsg),
    ));
    out
}

/// The `nightly.run.summary` spine fields, the wrapper's `emit_run_summary`.
pub fn run_summary_fields(rows: &[SuiteRow]) -> Vec<(String, String)> {
    let count = |s: &str| rows.iter().filter(|r| r.status == s).count();
    let failed = count("fail");
    let mut owners: Vec<&str> = rows.iter().filter(|r| r.status == "fail").map(|r| r.owner.as_str()).collect();
    owners.sort_unstable();
    owners.dedup();
    let csv: Vec<String> = owners.iter().map(|o| format!("{}={}", o, rows.iter().filter(|r| r.status == "fail" && r.owner == *o).count())).collect();
    vec![
        ("suites".into(), rows.len().to_string()),
        ("passed".into(), count("pass").to_string()),
        ("failed".into(), failed.to_string()),
        ("skipped".into(), count("skip").to_string()),
        ("unmeasurable".into(), count("unmeasurable").to_string()),
        ("red_by_owner".into(), if csv.is_empty() { "none".into() } else { csv.join(";") }),
        ("zero_red".into(), (failed == 0).to_string()),
    ]
}

/// The pipeline-run record body (`emit_pipeline_run`): outcome and counts.
pub fn pipeline_run_body(rows: &[SuiteRow], name: &str, trace: &str, duration_ms: u128) -> String {
    let failed = rows.iter().filter(|r| r.status == "fail").count();
    let passed = rows.iter().filter(|r| r.status == "pass").count();
    let outcome = if failed == 0 { "green" } else { "red" };
    format!(
        "{{\"name\":\"{}\",\"forPipeline\":\"pipeline-cicd\",\"traceId\":\"{}\",\"runOutcome\":\"{}\",\"runDurationMs\":\"{}\",\"testsRun\":\"{}\",\"testsFailed\":\"{}\",\"testsStored\":\"{}\"}}",
        name,
        trace,
        outcome,
        duration_ms,
        passed + failed,
        failed,
        rows.len()
    )
}

/// The per-row `test.suite.result` fields (`emit_suite_results`), with the
/// reporter-contradiction repair (#3753 AC4) applied on the way out.
pub fn suite_result_fields(row: &SuiteRow, reason: Option<&str>) -> (Vec<(String, String)>, bool) {
    let passed = first_count(&row.summary, &["passed", "pass", "ok"]).unwrap_or(0);
    let failed = first_count(&row.summary, &["failed", "fail"]).unwrap_or(0);
    let mut status = row.status.clone();
    let mut contradiction = false;
    if status == "pass" && failed > 0 {
        status = "fail".into();
        contradiction = true;
    }
    let mut f = vec![
        ("suite".to_string(), row.suite_name().to_string()),
        ("kind".to_string(), row.kind.clone()),
        ("status".to_string(), status),
        ("passed".to_string(), passed.to_string()),
        ("failed".to_string(), failed.to_string()),
        ("owner".to_string(), row.owner.clone()),
    ];
    if let Some(r) = reason {
        f.push(("reason".to_string(), r.to_string()));
    }
    (f, contradiction)
}

/// The census row from the run's OWN record: registered (file, name) pairs
/// minus the cases the runner posted this run. `never_ran` carries the names
/// so the log keeps them (#4140), `attributed` the lane-silence split.
pub fn census_row(registered_total: usize, never_ran: &[(String, String)], by_state: &str, attributed: &str) -> SuiteRow {
    if never_ran.is_empty() {
        return SuiteRow::new(
            "reconcile",
            "tests-domain",
            "kade",
            "pass",
            &format!("1 pass, 0 fail ({} registered, every one executed — ledger cross-foots)", registered_total),
        );
    }
    SuiteRow::new(
        "reconcile",
        "tests-domain",
        "kade",
        "fail",
        &format!(
            "0 pass, 1 fail ({} registered test(s) never ran of {} — {}; {})",
            never_ran.len(),
            registered_total,
            by_state,
            attributed
        ),
    )
}

/// The fail-log path for a row (`_fail_log_path`): kind + path, separators to
/// underscores, under the failure dir.
pub fn fail_log_name(kind: &str, path: &str) -> String {
    let id: String = format!("{}-{}", kind, path).chars().map(|c| if c == '/' || c == ' ' || c == '.' { '_' } else { c }).collect();
    format!("{}.log", id)
}

/// The slice of the lane output that belongs to one unit (#4004): lines that
/// name it, plus the runner's `!! kind:unit` verdict lines.
pub fn unit_slice<'a>(lane: &'a str, unit: &str) -> Vec<&'a str> {
    lane.lines().filter(|l| l.contains(unit)).collect()
}

/// The wrapper's `_extract_shell_summary`, kept for the smoke/lint/eslint legs
/// and the wrapper tests that exercised it.
pub fn shell_summary(out: &str, rc: i32) -> String {
    for l in out.lines().rev() {
        if l.starts_with("=== Results: ") && l.contains(" passed, ") && l.ends_with(" failed ===") {
            let p = first_count(l, &["passed"]).unwrap_or(0);
            let f = first_count(l, &["failed"]).unwrap_or(0);
            return format!("{} pass, {} fail", p, f);
        }
    }
    let p = out.lines().rev().find_map(|l| l.strip_prefix("Passed: ").and_then(|v| v.trim().parse::<usize>().ok()));
    let f = out.lines().rev().find_map(|l| l.strip_prefix("Failed: ").and_then(|v| v.trim().parse::<usize>().ok()));
    if let (Some(p), Some(f)) = (p, f) {
        return format!("{} pass, {} fail", p, f);
    }
    if let Some(last) = out.lines().last() {
        if first_count(last, &["pass", "ok"]).is_some() && first_count(last, &["fail"]).is_some() {
            return last.to_string();
        }
    }
    match rc {
        0 => "1 ok, 0 fail (synthesized, no parseable line)".into(),
        3 => "0 pass, 0 fail (SELF-REFUSED rc=3 — suite declined to run here)".into(),
        _ => format!("0 pass, 1 fail (synthesized rc={}, no parseable line)", rc),
    }
}


/// #4145 — a failed jest case's WHY, kept. The runner parsed jest's JSON for
/// names and status and dropped `failureMessages`, so a red inside the run
/// had no text anywhere (2026-09-11: 10 api reads failed twice in the run and
/// passed by hand, and nothing said what they saw). One line per failed case:
/// the first non-empty line of the message, ANSI stripped, capped at 200.
pub fn jest_failure_why(json: &str, pkg: &str, rel: &dyn Fn(&str) -> String) -> Vec<String> {
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(i) = json[pos..].find("\"assertionResults\"") {
        let start = pos + i;
        // the file name precedes its assertionResults in jest's shape
        let file = json[..start].rfind("\"name\":\"").map(|k| {
            let rest = &json[k + 8..];
            rest[..rest.find('"').unwrap_or(0)].to_string()
        }).unwrap_or_default();
        let end = json[start..].find("\"endTime\"").map(|e| start + e).unwrap_or(json.len());
        let block = &json[start..end];
        let mut q = 0;
        while let Some(j) = block[q..].find("\"fullName\":\"") {
            let k = q + j + 12;
            let name_end = block[k..].find("\",\"").map(|e| k + e).unwrap_or(block.len());
            let name = unescape_json(&block[k..name_end]);
            let after = &block[name_end..];
            let status = str_after(after, "\"status\":\"").unwrap_or_default();
            let msg = str_after(after, "\"failureMessages\":[\"").unwrap_or_default();
            if status == "failed" {
                let first = unescape_json(&msg)
                    .split('\n')
                    .map(|l| strip_ansi(l).trim().to_string())
                    .find(|l| !l.is_empty())
                    .unwrap_or_else(|| "(no message)".into());
                let first: String = first.chars().take(200).collect();
                out.push(format!("!! jest:{} WHY: {} :: {} :: {}", pkg, rel(&file), name, first));
            }
            q = name_end;
        }
        pos = end;
    }
    out
}

fn str_after(s: &str, key: &str) -> Option<String> {
    let i = s.find(key)? + key.len();
    let rest = &s[i..];
    let mut end = 0;
    let b = rest.as_bytes();
    while end < b.len() {
        if b[end] == b'\\' { end += 2; continue; }
        if b[end] == b'"' { break; }
        end += 1;
    }
    Some(rest[..end.min(rest.len())].to_string())
}

fn unescape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' { out.push(c); continue; }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('u') => { let h: String = it.by_ref().take(4).collect(); if let Ok(v) = u32::from_str_radix(&h, 16) { if let Some(ch) = char::from_u32(v) { out.push(ch); } } }
            Some(o) => { out.push('\\'); out.push(o); }
            None => out.push('\\'),
        }
    }
    out
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\u{1b}' {
            if it.peek() == Some(&'[') {
                it.next();
                while let Some(&d) = it.peek() { it.next(); if d.is_ascii_alphabetic() { break; } }
            }
            continue;
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod nightly_run_4145 {
    use super::*;

    fn owner_stub(_: &str) -> String {
        "kade".into()
    }


    #[test]
    fn jest_failure_why_keeps_the_first_assertion_line_and_nothing_for_passes() {
        let json = r#"{"testResults":[{"name":"/w/platform/api/tests/a.test.ts","assertionResults":[{"fullName":"grp passes","status":"passed","failureMessages":[]},{"fullName":"grp fails hard","status":"failed","failureMessages":["\u001b[1mError: \u001b[22mexpect(received).toBe(expected)\n\nExpected: 200\nReceived: 503"]}],"endTime":1}]}"#;
        let lines = jest_failure_why(json, "platform/api", &|f| f.replace("/w/", ""));
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0], "!! jest:platform/api WHY: platform/api/tests/a.test.ts :: grp fails hard :: Error: expect(received).toBe(expected)");
    }

    #[test]
    fn a_unit_line_folds_to_the_page_row_unchanged_in_shape() {
        let (row, contradiction) = fold_unit_line("nightly-unit|bats|platform/tests/x.bats|pass|3 pass, 0 fail", &owner_stub, false).unwrap();
        assert_eq!(row.line(), "SUITE|bats|platform/tests/x.bats|kade|pass|3 pass, 0 fail");
        assert!(!contradiction);
        assert_eq!(parse_suite_line(&row.line()).unwrap(), row);
    }

    #[test]
    fn a_cargo_unit_gets_its_crate_path_and_a_perf_fail_is_slow() {
        let (row, _) = fold_unit_line("nightly-unit|cargo|werk-test|pass|245 pass, 0 fail", &owner_stub, false).unwrap();
        assert_eq!(row.path, "platform/services/werk-test");
        let (row, _) = fold_unit_line("nightly-unit|perf|platform/tests/werk-phase-budgets.test.sh|fail|0 pass, 1 fail", &owner_stub, false).unwrap();
        assert_eq!(row.status, "slow");
    }

    #[test]
    fn negative_proof_pass_with_failures_is_a_contradiction_recorded_as_fail() {
        let (row, contradiction) = fold_unit_line("nightly-unit|npm|platform/api|pass|10 pass, 2 fail", &owner_stub, false).unwrap();
        assert_eq!(row.status, "fail");
        assert!(contradiction);
    }

    #[test]
    fn unmeasured_and_unmeasurable_states_are_named_not_green() {
        let (v, s) = remap_unmeasured("pass", "0 pass, 0 fail");
        assert_eq!(v, "unmeasured");
        assert!(s.contains("UNMEASURED"));
        let (v, _) = remap_unmeasured("pass", "0 pass, 0 fail (SELF-REFUSED rc=3 — suite declined to run here)");
        assert_eq!(v, "pass", "a row that names its own state keeps it");
        assert_eq!(classify_verdict("fail", "Cannot find module 'x'", false).0, "unmeasurable");
        assert_eq!(classify_verdict("fail", "SUITE TIMEOUT rc=124", true).0, "unmeasurable");
        assert_eq!(classify_verdict("fail", "SUITE TIMEOUT rc=124", false).0, "fail", "a timeout on an idle box is a fail");
    }

    #[test]
    fn owner_map_is_a_pure_function_of_registry_and_domains() {
        let tests = vec![("platform/tests/a.bats", "logs"), ("platform/tests/b.bats", "nope")];
        let domains = vec![("logs", "role-silas")];
        let m = owner_map(tests.into_iter(), domains.into_iter());
        assert_eq!(m.get("platform/tests/a.bats").map(String::as_str), Some("silas"));
        assert!(!m.contains_key("platform/tests/b.bats"), "no domain → no model owner");
        assert_eq!(owner_for("platform/tests/b.bats", &m, "/r", "/app"), "unowned", "the path rule has no platform/tests entry, as in the wrapper");
        assert_eq!(owner_for("platform/scripts/x.sh", &m, "/r", "/app"), "silas", "path rule fallback");
        assert_eq!(owner_for("/r/directing/x/tests/y.test.ts", &m, "/r", "/app"), "kade");
        assert_eq!(owner_for("elsewhere/x", &m, "/r", "/app"), "unowned");
    }

    #[test]
    fn floors_parse_sections_and_ignore_comments() {
        let y = "# c\nts:\n  directing/clearing: 85\n  platform/api: 80   # set by #2205\n\nrust:\n  # gone\n  platform/services/chorus-hooks: 45\n";
        assert_eq!(
            parse_floors(y),
            vec![
                ("ts".into(), "directing/clearing".into(), 85),
                ("ts".into(), "platform/api".into(), 80),
                ("rust".into(), "platform/services/chorus-hooks".into(), 45)
            ]
        );
    }

    #[test]
    fn coverage_rows_keep_the_four_outcomes() {
        assert!(coverage_row("a", "kade", 80, 0, Some(80.5)).line().ends_with("pass|1 pass, 0 fail (coverage 80.5% >= floor 80%)"));
        assert!(coverage_row("a", "kade", 80, 0, Some(79.0)).status == "fail");
        assert!(coverage_row("a", "kade", 80, 0, None).summary.contains("NO summary artifact"));
        assert!(coverage_row("a", "kade", 80, 124, None).summary.contains("rc=124"));
    }

    #[test]
    fn denominator_ratchet_fails_on_growth_and_moves_only_down() {
        let (row, nb) = denominator_row(3, 23, &["a".into(), "b".into()], 2);
        assert_eq!(row.status, "pass");
        assert_eq!(nb, None);
        let (row, _) = denominator_row(3, 23, &["a".into(), "b".into(), "c".into()], 2);
        assert_eq!(row.status, "fail", "negative proof: a crate shipped without a floor");
        let (_, nb) = denominator_row(3, 23, &["a".into()], 2);
        assert_eq!(nb, Some(1));
    }

    #[test]
    fn notify_groups_reds_per_owner_names_slow_and_totals_once() {
        let rows = vec![
            SuiteRow::new("bats", "platform/tests/a.bats", "wren", "fail", "0 pass, 1 fail"),
            SuiteRow::new("bats", "platform/tests/b.bats", "wren", "fail", "0 pass, 1 fail"),
            SuiteRow::new("security", "platform/tests/s.bats", "silas", "fail", "0 pass, 1 fail"),
            SuiteRow::new("perf", "platform/tests/p.sh", "silas", "slow", "0 pass, 1 fail"),
            SuiteRow::new("bats", "platform/tests/c.bats", "kade", "pass", "1 pass, 0 fail"),
        ];
        let m = notify_messages(&rows, "silas");
        assert_eq!(m[0].0, "silas");
        assert!(m[0].1.starts_with("SECURITY lane: 1 red — s.bats"));
        assert!(m.iter().any(|(to, msg)| to == "wren" && msg == "nightly: 2 suite(s) red — a.bats, b.bats"));
        let total = &m.last().unwrap().1;
        assert!(total.starts_with("nightly TOTAL: 3 red across the board (silas 1, wren 2) — bar is zero"), "{total}");
        assert!(total.contains("1 slow (speed, not breakage: p.sh)"));
    }

    #[test]
    fn negative_proof_all_green_is_one_line_to_the_owner_never_silence() {
        let rows = vec![SuiteRow::new("bats", "platform/tests/c.bats", "kade", "pass", "1 pass, 0 fail")];
        let m = notify_messages(&rows, "silas");
        assert_eq!(m.len(), 1);
        assert!(m[0].1.starts_with("nightly: all hermetic suites green"));
    }

    #[test]
    fn census_from_the_runs_own_cases_needs_no_ledger_walk() {
        let registered = vec![("f.rs".to_string(), "a".to_string()), ("f.rs".to_string(), "b".to_string())];
        let cases: Vec<(String, String)> = ["nightly-case|f.rs|a", "nightly-case|f.rs|b", "noise"].iter().filter_map(|l| parse_case_line(l)).collect();
        let gap = crate::reconcile_gap(&registered, &cases);
        assert!(gap.is_empty());
        assert_eq!(census_row(2, &gap, "", "").status, "pass");
        // negative proof: one registered case the run never posted is never-ran
        let cases = vec![("f.rs".to_string(), "a".to_string())];
        let gap = crate::reconcile_gap(&registered, &cases);
        assert_eq!(gap, vec![("f.rs".to_string(), "b".to_string())]);
        let row = census_row(2, &gap, "NAME MISMATCH 1", "lane silent 0");
        assert_eq!(row.status, "fail");
        assert!(row.summary.contains("1 registered test(s) never ran of 2"));
    }

    #[test]
    fn suite_result_fields_repair_a_contradiction_and_carry_the_reason() {
        let row = SuiteRow::new("shell", "platform/scripts/x.sh", "silas", "pass", "2 pass, 1 fail");
        let (f, c) = suite_result_fields(&row, Some("boom"));
        assert!(c);
        assert!(f.iter().any(|(k, v)| k == "status" && v == "fail"));
        assert!(f.iter().any(|(k, v)| k == "reason" && v == "boom"));
    }

    #[test]
    fn shell_summary_reads_the_three_forms_and_synthesizes_from_rc() {
        assert_eq!(shell_summary("x\n=== Results: 4 passed, 1 failed ===\n", 1), "4 pass, 1 fail");
        assert_eq!(shell_summary("Passed: 2\nFailed: 0\n", 0), "2 pass, 0 fail");
        assert_eq!(shell_summary("3 ok, 0 fail", 0), "3 ok, 0 fail");
        assert!(shell_summary("nothing", 3).contains("SELF-REFUSED"));
        assert!(shell_summary("nothing", 2).contains("synthesized rc=2"));
    }

    #[test]
    fn fail_log_name_matches_the_wrappers_files() {
        assert_eq!(fail_log_name("bats", "platform/tests/deep-health.bats"), "bats-platform_tests_deep-health_bats.log");
    }

    #[test]
    fn run_summary_and_pipeline_body_count_the_rows() {
        let rows = vec![
            SuiteRow::new("bats", "a", "wren", "fail", "0 pass, 1 fail"),
            SuiteRow::new("bats", "b", "kade", "pass", "1 pass, 0 fail"),
            SuiteRow::new("bats", "c", "kade", "skip", "skipped"),
        ];
        let f = run_summary_fields(&rows);
        assert!(f.contains(&("red_by_owner".to_string(), "wren=1".to_string())));
        assert!(f.contains(&("zero_red".to_string(), "false".to_string())));
        let body = pipeline_run_body(&rows, "nightly-x", "t", 5);
        assert!(body.contains("\"runOutcome\":\"red\"") && body.contains("\"testsRun\":\"2\"") && body.contains("\"testsStored\":\"3\""));
    }
}
