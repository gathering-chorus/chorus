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

/// Kinds whose `unit` field IS a repo-relative path, so its absence from disk
/// is a real fact about the registry. `cargo` (a crate name) and `npm` /
/// `coverage` (a package dir that always exists) are deliberately absent.
pub const PATH_SHAPED_KINDS: [&str; 4] = ["bats", "shell", "security", "perf"];

/// A runner `nightly-unit|kind|unit|verdict|summary` line folded to the page's
/// row. A cargo unit's path is its crate directory; a perf row that fails is
/// SLOW, not red (#4136).
pub fn fold_unit_line(
    line: &str,
    owner: &dyn Fn(&str) -> String,
    box_over_load: bool,
    exists: &dyn Fn(&str) -> bool,
) -> Option<(SuiteRow, bool)> {
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
    // #4168 — the registry named a file that is no longer in the repo. That is
    // bookkeeping, not breakage: report it as its own state so the red list
    // means what Jeff needs it to mean. Only kinds whose UNIT IS a repo path
    // are probed — a cargo crate name or an npm package dir is not a file, and
    // probing them would mark every unit stale.
    if PATH_SHAPED_KINDS.contains(&kind) && !exists(&path) {
        let row = SuiteRow::new(
            kind,
            &path,
            &owner(&path),
            "stale",
            &format!("0 pass, 0 fail (STALE REGISTRY — {} is not in the repo; the row outlived its file)", path),
        );
        return Some((row, false));
    }
    let (v, contradiction) = classify_verdict(verdict, summary, box_over_load);
    let (mut v, s) = remap_unmeasured(&v, summary);
    if kind == "perf" && v == "fail" {
        v = "slow".into();
    }
    Some((SuiteRow::new(kind, &path, &owner(&path), &v, &s), contradiction))
}

/// A JSON string field read WITH its escapes honoured. The registry names a
/// case exactly as its source spells it; `has zero ="// occurrences in
/// index.html` arrives as `"has zero =\"// occurrences in index.html"`, and a
/// reader that stops at the first quote registers `has zero =\` — 60 rows
/// stood as NAME MISMATCH on 2026-09-12 for names that were whole in the
/// store. Handles `\"`, `\\`, `\/`, `\n`, `\t`, `\r`, `\b`, `\f` and
/// `\uXXXX` (with surrogate pairs); anything else keeps the character.
pub fn json_str_field(obj: &str, key: &str) -> Option<String> {
    let i = obj.find(key)? + key.len();
    let rest = obj[i..].trim_start().strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                '/' => out.push('/'),
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                'b' => out.push('\u{8}'),
                'f' => out.push('\u{c}'),
                'u' => {
                    let hex: String = chars.by_ref().take(4).collect();
                    let mut cp = u32::from_str_radix(&hex, 16).ok()?;
                    if (0xD800..0xDC00).contains(&cp) {
                        // surrogate pair: expect \uDC00-DFFF next
                        let tail: String = chars.by_ref().take(6).collect();
                        let lo = tail.strip_prefix("\\u").and_then(|h| u32::from_str_radix(h, 16).ok())?;
                        cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                    }
                    out.push(char::from_u32(cp)?);
                }
                other => out.push(other),
            },
            _ => out.push(c),
        }
    }
    None
}

/// Every object in a JSON array body that carries both `a` and `b` as string
/// fields, as (a, b) pairs, in order. Objects are split on `},` — the bodies
/// the registry serves are flat rows, never nested objects.
pub fn json_rows(json: &str, a: &str, b: &str) -> Vec<(String, String)> {
    let ka = format!("\"{}\"", a);
    let kb = format!("\"{}\"", b);
    json.split("},")
        .filter_map(|obj| Some((json_str_field(obj, &ka)?, json_str_field(obj, &kb)?)))
        .collect()
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
/// #4180 — the crawler's last scheduled pass, as one clause on the TOTAL line.
///
/// The on-land delta deliberately cannot fail a land (`continue-on-error`), and
/// the nightly writes to a log nobody opens. That is how a crawler that never
/// ran shipped as "self-maintaining": nothing a person reads said otherwise.
/// This reads the log the plist writes and says, every morning, one of three
/// things — it never ran, it ran red, or it ran and what it did.
pub fn crawl_line(log: Option<&str>) -> String {
    let Some(text) = log else { return " — crawl: NEVER RAN (no log)".to_string() };
    // last pass = from the last "chorus-crawl: full|delta" header to the end
    let start = text.rmatch_indices("chorus-crawl: full").chain(text.rmatch_indices("chorus-crawl: delta")).map(|(i, _)| i).max().unwrap_or(0);
    let last = &text[start..];
    if last.trim().is_empty() {
        return " — crawl: NEVER RAN (empty log)".to_string();
    }
    let field = |k: &str| last.split_whitespace().find_map(|w| w.strip_prefix(k).and_then(|v| v.parse::<usize>().ok()));
    let (wrote, failed) = (field("wrote=").unwrap_or(0), field("failed=").unwrap_or(0));
    if failed > 0 || last.contains("the run is RED") || last.contains("watermark HELD") {
        format!(" — crawl: RED ({} write(s) failed, watermark held)", failed)
    } else {
        format!(" — crawl: {} written, clean", wrote)
    }
}

pub fn notify_messages(rows: &[SuiteRow], security_owner: &str, crawl: &str) -> Vec<(String, String)> {
    let skipped = rows.iter().filter(|r| r.status == "skip").count();
    let skipmsg = if skipped > 0 { format!(" — {} skipped (no live stack, #3557)", skipped) } else { String::new() };
    let reds: Vec<&SuiteRow> = rows.iter().filter(|r| r.status == "fail").collect();
    let mut out = Vec::new();
    // #4168 — a stale row is bookkeeping, not breakage. It never counts toward
    // the red total (the bar stays reachable) but it is always named, or a row
    // that outlived its file would sit in the registry unseen.
    let stale: Vec<&str> = rows.iter().filter(|r| r.status == "stale").map(|r| r.suite_name()).collect();
    let stalemsg = if stale.is_empty() {
        String::new()
    } else {
        format!(" — {} stale (deleted file, still registered: {})", stale.len(), stale.join(", "))
    };
    if reds.is_empty() {
        out.push(("kade".to_string(), format!("nightly: all hermetic suites green ✅{}{}", skipmsg, stalemsg)));
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
        format!("nightly TOTAL: {} red across the board ({}) — bar is zero{}{}{}{}", reds.len(), per_owner.join(", "), skipmsg, slowmsg, stalemsg, crawl),
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
        // #4168 — counted on its own axis; deliberately NOT folded into failed,
        // so zero_red stays true on a run whose only non-pass rows are stale.
        ("stale".into(), count("stale").to_string()),
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
    // jest's JSON keys are alphabetical: a testResults entry is
    // {"assertionResults":[{"ancestorTitles",…,"failureMessages",…,"fullName",…,"status",…}],"endTime",…,"name",…}
    // so the FILE name follows its assertions, and inside an assertion the
    // messages precede the name. Scope every lookup to its own object.
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(i) = json[pos..].find("\"assertionResults\"") {
        let start = pos + i;
        let end = json[start..].find("\"endTime\"").map(|e| start + e).unwrap_or(json.len());
        let block = &json[start..end];
        let file = str_after(&json[end..], "\"name\":\"").unwrap_or_default();
        let entries: Vec<&str> = block.split("\"ancestorTitles\"").skip(1).collect();
        for e in entries {
            let status = str_after(e, "\"status\":\"").unwrap_or_default();
            if status != "failed" {
                continue;
            }
            let name = str_after(e, "\"fullName\":\"").map(|n| unescape_json(&n)).unwrap_or_default();
            let msg = str_after(e, "\"failureMessages\":[\"").unwrap_or_default();
            let first = unescape_json(&msg)
                .split('\n')
                .map(|l| strip_ansi(l).trim().to_string())
                .find(|l| !l.is_empty())
                .unwrap_or_else(|| "(no message)".into());
            let first: String = first.chars().take(200).collect();
            out.push(format!("!! jest:{} WHY: {} :: {} :: {}", pkg, rel(&unescape_json(&file)), name, first));
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


/// `--last-run` (#3606/#3272): replay the LATEST run's SUITE rows from the log.
/// The block starts at the last `RUN|start|`; a block with no `RUN|complete|`
/// is PARTIAL and says so as a loud meta row first (the 2026-07-04 false wall
/// came from re-running instead of reading). A log with no RUN markers at all
/// (pre-#3709) is cut at the first repeated (kind, path) key walking backward.
pub fn last_run_rows(log: &str) -> Vec<String> {
    let raw: Vec<&str> = log.lines().collect();
    let start = raw.iter().rposition(|l| l.starts_with("RUN|start|"));
    let Some(si) = start else {
        let mut seen = std::collections::HashSet::new();
        let mut run: Vec<&str> = Vec::new();
        for l in raw.iter().rev().filter(|l| l.starts_with("SUITE|")) {
            let p: Vec<&str> = l.split('|').collect();
            let key = (p.get(1).copied().unwrap_or(""), p.get(2).copied().unwrap_or(""));
            if !seen.insert(key) {
                break;
            }
            run.push(l);
        }
        run.reverse();
        return run.into_iter().map(String::from).collect();
    };
    let block = &raw[si..];
    let completed = block.iter().any(|l| l.starts_with("RUN|complete|"));
    let suites: Vec<String> = block.iter().filter(|l| l.starts_with("SUITE|")).map(|l| l.to_string()).collect();
    let mut out = Vec::new();
    if !completed {
        let started = raw[si].split('|').nth(2).unwrap_or("?");
        out.push(format!(
            "SUITE|meta|nightly-run-incomplete|silas|fail|0 pass, 1 fail (run started {} and never wrote RUN|complete — KILLED after {} suite(s); the results below are PARTIAL, not a full night)",
            started,
            suites.len()
        ));
    }
    out.extend(suites);
    out
}

/// The load gate's verdict line and pass/fail, from cores × per-core cap
/// against the 1-minute load (`NIGHTLY_LOAD_STUB` is the test seam).
pub fn load_verdict(load: f64, cores: f64, per_core: f64) -> (bool, String) {
    let max = cores * per_core;
    (load <= max, format!("load={} max={:.1}", load, max))
}

#[cfg(test)]
mod nightly_run_4145 {
    use super::*;

    #[test]
    fn json_str_field_keeps_an_escaped_quote_whole() {
        let obj = r#"{"filePath": "directing/clearing/tests/base-path-3872.test.ts", "testName": "has zero =\"// occurrences in index.html", "validityClass": ""}"#;
        assert_eq!(json_str_field(obj, "\"testName\"").as_deref(), Some(r#"has zero ="// occurrences in index.html"#));
        assert_eq!(json_str_field(obj, "\"filePath\"").as_deref(), Some("directing/clearing/tests/base-path-3872.test.ts"));
    }

    #[test]
    fn json_str_field_decodes_every_json_escape() {
        let obj = r#"{"n": "a\\b\/c\n\t\u00e9\ud83d\ude00 end"}"#;
        assert_eq!(json_str_field(obj, "\"n\"").as_deref(), Some("a\\b/c\n\t\u{e9}\u{1F600} end"));
    }

    #[test]
    fn negative_proof_the_old_first_quote_reader_truncated_at_the_backslash() {
        // The 2026-09-12 census: registry `has zero =\"// …` read as `has zero =\`.
        let obj = r#"{"testName": "has zero =\"// occurrences in index.html"}"#;
        let key = "\"testName\"";
        let i = obj.find(key).unwrap() + key.len();
        let rest = obj[i..].trim_start().strip_prefix(':').unwrap().trim_start().strip_prefix('"').unwrap();
        let old = &rest[..rest.find('"').unwrap()];
        assert_eq!(old, "has zero =\\", "the defect this test pins");
        assert_ne!(json_str_field(obj, key).as_deref(), Some(old));
    }

    #[test]
    fn json_rows_pairs_every_row_and_skips_rows_missing_a_field() {
        let body = r#"{"data": [{"filePath": "a.bats", "testName": "one"}, {"filePath": "a.bats"}, {"filePath": "b.ts", "testName": "say \"hi\""}]}"#;
        let rows = json_rows(body, "filePath", "testName");
        assert_eq!(rows, vec![("a.bats".to_string(), "one".to_string()), ("b.ts".to_string(), "say \"hi\"".to_string())]);
    }


    fn present_stub(_: &str) -> bool {
        true
    }

    fn owner_stub(_: &str) -> String {
        "kade".into()
    }


    #[test]
    fn jest_failure_why_keeps_the_first_assertion_line_and_nothing_for_passes() {
        // jest's real key order: assertions first, the file's "name" AFTER them; messages before fullName
        let json = r#"{"testResults":[{"assertionResults":[{"ancestorTitles":["grp"],"failureMessages":[],"fullName":"grp passes","status":"passed"}],"endTime":1,"name":"/w/platform/api/tests/first.test.ts","status":"passed"},{"assertionResults":[{"ancestorTitles":["grp"],"failureMessages":["\u001b[1mError: \u001b[22mexpect(received).toBe(expected)\n\nExpected: 200\nReceived: 503"],"fullName":"grp fails hard","status":"failed"},{"ancestorTitles":["grp"],"failureMessages":[],"fullName":"grp ok","status":"passed"}],"endTime":2,"name":"/w/platform/api/tests/second.test.ts","status":"failed"}]}"#;
        let lines = jest_failure_why(json, "platform/api", &|f| f.replace("/w/", ""));
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0], "!! jest:platform/api WHY: platform/api/tests/second.test.ts :: grp fails hard :: Error: expect(received).toBe(expected)");
    }


    #[test]
    fn last_run_replays_the_latest_block_and_flags_a_partial_one() {
        let log = "RUN|start|2026-09-11T03:00:01|pid=1\nSUITE|bats|a|kade|pass|1 pass, 0 fail\nRUN|complete|2026-09-11T04:00:00|suites=1\nRUN|start|2026-09-11T15:51:18|pid=2\nSUITE|bats|b|kade|fail|0 pass, 1 fail\n";
        let rows = last_run_rows(log);
        assert_eq!(rows[0].split('|').nth(2), Some("nightly-run-incomplete"), "{rows:?}");
        assert!(rows[0].contains("started 2026-09-11T15:51:18") && rows[0].contains("KILLED after 1 suite(s)"));
        assert_eq!(rows[1], "SUITE|bats|b|kade|fail|0 pass, 1 fail");
        // control: a completed block has no meta row
        let done = "RUN|start|2026-09-11T03:00:01|pid=1\nSUITE|bats|a|kade|pass|1 pass, 0 fail\nRUN|complete|2026-09-11T04:00:00|suites=1\n";
        assert_eq!(last_run_rows(done), vec!["SUITE|bats|a|kade|pass|1 pass, 0 fail".to_string()]);
        // pre-#3709 log: cut at the first repeated key walking back
        let old = "SUITE|bats|a|kade|pass|1 pass, 0 fail\nSUITE|bats|b|kade|pass|1 pass, 0 fail\nSUITE|bats|a|kade|fail|0 pass, 1 fail\n";
        assert_eq!(last_run_rows(old), vec!["SUITE|bats|b|kade|pass|1 pass, 0 fail".to_string(), "SUITE|bats|a|kade|fail|0 pass, 1 fail".to_string()], "walk back until the first repeated (kind,path) key");
    }

    #[test]
    fn load_verdict_holds_over_the_cap_and_passes_under_it() {
        assert!(load_verdict(0.1, 8.0, 1.5).0);
        assert!(!load_verdict(999.0, 8.0, 0.1).0, "negative proof: a loaded box is held");
        assert_eq!(load_verdict(5.0, 8.0, 100.0).1, "load=5 max=800.0");
    }

    #[test]
    fn a_unit_line_folds_to_the_page_row_unchanged_in_shape() {
        let (row, contradiction) = fold_unit_line("nightly-unit|bats|platform/tests/x.bats|pass|3 pass, 0 fail", &owner_stub, false, &present_stub).unwrap();
        assert_eq!(row.line(), "SUITE|bats|platform/tests/x.bats|kade|pass|3 pass, 0 fail");
        assert!(!contradiction);
        assert_eq!(parse_suite_line(&row.line()).unwrap(), row);
    }

    #[test]
    fn a_cargo_unit_gets_its_crate_path_and_a_perf_fail_is_slow() {
        let (row, _) = fold_unit_line("nightly-unit|cargo|werk-test|pass|245 pass, 0 fail", &owner_stub, false, &present_stub).unwrap();
        assert_eq!(row.path, "platform/services/werk-test");
        let (row, _) = fold_unit_line("nightly-unit|perf|platform/tests/werk-phase-budgets.test.sh|fail|0 pass, 1 fail", &owner_stub, false, &present_stub).unwrap();
        assert_eq!(row.status, "slow");
    }

    // #4168 — a test file that left the repo is a STALE REGISTRY row, not a
    // product break. 2026-09-13: three of these in one day (tag-tests-domain-
    // delete-guard.bats and test-tag-tests-validate-first.sh in the 03:00 run,
    // test-security-manifest-3726.sh blocking Silas at 13:58). Jeff's bar is
    // that a red means the product broke; today a red can mean someone deleted
    // a file, and the page cannot tell the two apart.
    #[test]
    fn a_unit_whose_file_left_the_repo_is_stale_not_red() {
        let gone = |_: &str| false;
        let (row, contradiction) = fold_unit_line(
            "nightly-unit|bats|platform/tests/deleted.bats|fail|0 pass, 1 fail",
            &owner_stub,
            false,
            &gone,
        )
        .unwrap();
        assert_eq!(row.status, "stale", "a row whose file is gone is not a failure");
        assert!(row.summary.contains("STALE REGISTRY"), "the row says why in its own words: {}", row.summary);
        assert!(row.summary.contains("platform/tests/deleted.bats"), "and names the path: {}", row.summary);
        assert!(!contradiction);
        assert_eq!(parse_suite_line(&row.line()).unwrap(), row, "still round-trips through the page");
    }

    // The check must separate the two states it exists to tell apart (#3734).
    // A green here with the file PRESENT would mean the check cannot distinguish
    // "someone deleted the test" from "the product broke" — which is the defect.
    #[test]
    fn negative_proof_a_file_that_exists_and_genuinely_fails_is_still_red() {
        let present = |_: &str| true;
        let (row, _) = fold_unit_line(
            "nightly-unit|bats|platform/tests/deleted.bats|fail|0 pass, 1 fail",
            &owner_stub,
            false,
            &present,
        )
        .unwrap();
        assert_eq!(row.status, "fail", "the file is on disk — this is the product breaking");
        assert!(!row.summary.contains("STALE"));
    }

    // A cargo crate or an npm package is not a file path; the existence probe
    // must never be applied to it, or every cargo unit reads stale.
    #[test]
    fn negative_proof_a_cargo_or_npm_unit_is_never_marked_stale() {
        let gone = |_: &str| false;
        let (row, _) = fold_unit_line("nightly-unit|cargo|werk-test|fail|0 pass, 1 fail", &owner_stub, false, &gone).unwrap();
        assert_eq!(row.status, "fail", "a crate name is not a path that can go missing");
        let (row, _) = fold_unit_line("nightly-unit|npm|platform/api|fail|0 pass, 2 fail", &owner_stub, false, &gone).unwrap();
        assert_eq!(row.status, "fail");
    }

    // #4168 AC2 — stale rows are counted and named in their own words, never
    // folded into the red total and never silently dropped either.
    #[test]
    fn stale_rows_are_reported_separately_from_reds_and_counted() {
        let rows = vec![
            SuiteRow::new("bats", "platform/tests/gone.bats", "kade", "stale", "0 pass, 0 fail (STALE REGISTRY — platform/tests/gone.bats is not in the repo; the row outlived its file)"),
            SuiteRow::new("bats", "platform/tests/real.bats", "wren", "fail", "0 pass, 1 fail"),
        ];
        let msgs = notify_messages(&rows, "silas", "");
        let total = msgs.iter().find(|(who, m)| who == "kade" && m.contains("TOTAL")).expect("a total line").1.clone();
        assert!(total.contains("1 red"), "the stale row is not counted as red: {}", total);
        assert!(total.contains("1 stale"), "but it IS named: {}", total);

        let fields = run_summary_fields(&rows);
        let get = |k: &str| fields.iter().find(|(f, _)| f == k).map(|(_, v)| v.clone()).unwrap_or_default();
        assert_eq!(get("failed"), "1");
        assert_eq!(get("stale"), "1");
        assert_eq!(get("zero_red"), "false");
    }

    // The check must be able to say zero. A run with a stale row and no reds is
    // a GREEN run — if stale leaked into the red total, zero_red could never be
    // true while any deleted file remained in the registry.
    #[test]
    fn negative_proof_a_run_with_only_stale_rows_is_still_zero_red() {
        let rows = vec![
            SuiteRow::new("bats", "platform/tests/gone.bats", "kade", "stale", "0 pass, 0 fail (STALE REGISTRY — platform/tests/gone.bats is not in the repo; the row outlived its file)"),
            SuiteRow::new("bats", "platform/tests/real.bats", "wren", "pass", "3 pass, 0 fail"),
        ];
        let fields = run_summary_fields(&rows);
        let get = |k: &str| fields.iter().find(|(f, _)| f == k).map(|(_, v)| v.clone()).unwrap_or_default();
        assert_eq!(get("zero_red"), "true");
        assert_eq!(get("stale"), "1");
        let msgs = notify_messages(&rows, "silas", "");
        assert!(msgs.iter().any(|(_, m)| m.contains("green")), "green run still reads green");
        assert!(msgs.iter().any(|(_, m)| m.contains("1 stale")), "and still names the stale row");
    }

    #[test]
    fn negative_proof_pass_with_failures_is_a_contradiction_recorded_as_fail() {
        let (row, contradiction) = fold_unit_line("nightly-unit|npm|platform/api|pass|10 pass, 2 fail", &owner_stub, false, &present_stub).unwrap();
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
        let m = notify_messages(&rows, "silas", "");
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
        let m = notify_messages(&rows, "silas", "");
        assert_eq!(m.len(), 1);
        assert!(m[0].1.starts_with("nightly: all hermetic suites green"));
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

#[cfg(test)]
mod crawl_line_4180 {
    use super::crawl_line;

    #[test]
    fn a_clean_pass_and_a_missing_log_read_as_what_they_are() {
        let ok = "chorus-crawl: full (no watermark) · tracked=6190 read=Complete\nchorus-crawl: posted=4 replaced=0 unchanged=5547 deleted=0 skipped=639\nchorus-crawl: wrote=4 failed=0\nchorus-crawl: watermark -> ad5b6182b\n";
        assert_eq!(crawl_line(Some(ok)), " — crawl: 4 written, clean");
        assert_eq!(crawl_line(None), " — crawl: NEVER RAN (no log)");
        // only the LAST pass counts — an old red must not haunt a clean morning
        let two = format!("chorus-crawl: full\nchorus-crawl: wrote=0 failed=3\nchorus-crawl: 3 write(s) failed — the run is RED\n{ok}");
        assert_eq!(crawl_line(Some(&two)), " — crawl: 4 written, clean");
    }

    // NEGATIVE PROOF (#3734): the clause exists to make a silent red loud. A red
    // pass — failed writes, a held watermark — must NOT read as clean, and the
    // exact log shape the 2026-09-15 refusal produced is the fixture.
    #[test]
    fn negative_proof_a_red_pass_is_named_red_not_clean() {
        let red = "chorus-crawl: full (--reconcile) · tracked=6190 read=Complete\nchorus-crawl: posted=0 replaced=3 unchanged=5547 deleted=0 skipped=639\nchorus-crawl: wrote=0 failed=3\nchorus-crawl: watermark HELD — a write failed — the graph does not match this commit\nchorus-crawl: 3 write(s) failed — the run is RED, not partially green\n";
        let line = crawl_line(Some(red));
        assert!(line.contains("RED"), "{line}");
        assert!(line.contains("3 write(s) failed"), "{line}");
        assert!(!line.contains("clean"), "{line}");
    }
}
}
