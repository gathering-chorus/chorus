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

/// `nightly-case|filePath|testName|result` — one per case the runner joined
/// and posted this run. The census below is the registry minus these, computed
/// here from the run's own record instead of a 27-page walk of the ledger.
///
/// #4247 — the result is the fourth field. Without it the log recorded a
/// result per SUITE and an identity per TEST, so "31 red" and "8,537 ran" were
/// different nouns: not comparable, not addable, not trendable. A line written
/// before this carries three fields; it parses with an empty result, which the
/// census reports as unmeasured rather than passing.
pub fn parse_case_line(line: &str) -> Option<(String, String, String, String)> {
    let rest = line.strip_prefix("nightly-case|")?;
    // A test name may itself contain '|' (jest it-names do). So: the file is
    // the first field, and the result is the LAST field only when it is a
    // verdict word. Anything else is part of the name, with no result.
    let (f, rest) = rest.split_once('|')?;
    if f.is_empty() || rest.is_empty() {
        return None;
    }
    let verdict = |w: &str| matches!(w, "pass" | "fail" | "skip");
    // #4271 — the four-field shape: <name>|<verdict>|<regLen>. Digits cannot be
    // swallowed by the split the way a name can, and the verdict check in the
    // middle rules out a name that merely ends in "|123". A three-field line
    // (every line written before this) still parses, with the case answering
    // for the registered row of its own name.
    if let Some((head, tail)) = rest.rsplit_once('|') {
        if !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) {
            if let Some((name, r)) = head.rsplit_once('|') {
                if verdict(r) && !name.is_empty() {
                    let len: usize = tail.parse().unwrap_or(0);
                    // refuse, never guess: a length that does not land on a
                    // boundary of the name yields NO join key, which counts as
                    // a case that answers for no registered row — a visible
                    // state, not a phantom never-ran against the registry.
                    let reg = registered_from_suffix_len(name, len).unwrap_or_default();
                    return Some((f.to_string(), name.to_string(), r.to_string(), reg));
                }
            }
        }
    }
    match rest.rsplit_once('|') {
        Some((n, r)) if verdict(r) && !n.is_empty() => {
            Some((f.to_string(), n.to_string(), r.to_string(), n.to_string()))
        }
        _ => Some((f.to_string(), rest.to_string(), String::new(), rest.to_string())),
    }
}

/// #4247 — every registered test with NO result in this run, by name.
///
/// A registered test that silently does not run reads exactly like one that
/// passed. On 2026-09-20 the gap was 154 of 8,691 and nothing in the run named
/// one of them; the number had to be inferred by matching two lists on
/// file-and-name, so a renamed test looked like a test that never ran.
///
/// `registered` is (filePath, testName) from the registry; `ran` is what the
/// run's own `nightly-case` lines reported. The answer is in the same unit as
/// both inputs: the registered test.
pub fn tests_with_no_result(
    registered: &[(String, String)],
    ran: &[(String, String, String, String)],
) -> Vec<(String, String)> {
    // #4271 — joined on the REGISTERED name a case answers for, not on the
    // case's own name. The two differ for every case a `describe.each` block
    // generates; matching on the case name would read all 8,749 registered
    // tests as never-ran.
    let seen: std::collections::HashSet<(&str, &str)> = ran
        .iter()
        .filter(|(_, _, _, reg)| !reg.is_empty())
        .map(|(f, _, _, reg)| (f.as_str(), reg.as_str()))
        .collect();
    let mut out: Vec<(String, String)> = registered
        .iter()
        .filter(|(f, n)| !seen.contains(&(f.as_str(), n.as_str())))
        .cloned()
        .collect();
    out.sort();
    out.dedup();
    out
}

/// #4247 — the run's counts, all in one unit: the registered test.
///
/// Suites stay a separate line labelled as suites. Jeff, 2026-09-20: "rather
/// than stating everything in a different unit of measure can we be consistent".
pub fn registered_test_tally(
    registered: &[(String, String)],
    ran: &[(String, String, String, String)],
) -> String {
    let no_result = tests_with_no_result(registered, ran).len();
    let failed = ran.iter().filter(|(_, _, r, _)| r == "fail").count();
    let passed = ran.iter().filter(|(_, _, r, _)| r == "pass").count();
    let unmeasured = ran.len() - failed - passed;
    format!(
        "registered {} · ran {} · passed {} · failed {} · unmeasured {} · no result {}",
        registered.len(),
        ran.len(),
        passed,
        failed,
        unmeasured,
        no_result
    )
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
        // #4265 — a run that never produced a number is UNMEASURED, not a
        // failure. Scoring it "fail" made a coverage job that crashed
        // indistinguishable from one that measured and came in under floor,
        // and Jeff asked for exactly that distinction today: "76 fail" is not
        // 76 failures if some of them never answered. Below-floor stays fail
        // (the arm above) — that is the state this check exists to catch.
        (0, None) => ("unmeasured", format!("0 pass, 0 fail (UNMEASURED — coverage ran but produced NO summary artifact; floor {}% not evaluated)", floor)),
        (rc, _) => ("unmeasured", format!("0 pass, 0 fail (UNMEASURED — coverage run errored rc={}; floor {}% not evaluated, nothing measured)", rc, floor)),
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
    // #4199 — the pass grades the graph against the project after it writes;
    // the morning line carries that verdict verbatim when the log has it.
    let project = last
        .lines()
        .filter_map(|l| l.split_once("graph vs project · ").map(|(_, v)| v.trim()))
        .last()
        .map(|v| format!(" — graph vs project: {v}"))
        .unwrap_or_default();
    if failed > 0 || last.contains("the run is RED") || last.contains("watermark HELD") {
        format!(" — crawl: RED ({} write(s) failed, watermark held){project}", failed)
    } else {
        format!(" — crawl: {} written, clean{project}", wrote)
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

/// #4271 — the byte length of the registered name inside the case's own name.
/// The registered name is a suffix of the emitted name by construction (an
/// exact join makes them equal; the suffix join requires `ends_with`), so the
/// line carries this number instead of a second copy of the name. 0 means the
/// case answers for no registered row.
pub fn registered_suffix_len(test_name: &str, registered: &str) -> usize {
    if !registered.is_empty() && test_name.ends_with(registered) {
        registered.len()
    } else {
        0
    }
}

/// The registered name a `nightly-case` line's suffix length points at, or
/// None when the length does not land on a boundary of the name. It REFUSES
/// rather than guessing: a length that slices mid-name would hand the census a
/// join key no registry row holds, which reads as a phantom never-ran.
pub fn registered_from_suffix_len(test_name: &str, len: usize) -> Option<String> {
    if len == 0 || len > test_name.len() {
        return None;
    }
    let start = test_name.len() - len;
    if !test_name.is_char_boundary(start) {
        return None;
    }
    Some(test_name[start..].to_string())
}

/// #4271 — the PipelineRun row's name, built from the run's START stamp so it
/// carries the same id the log brackets the run with (`RUN|start|<stamp>`).
///
/// It used to be built at emit time, which is the moment the run FINISHED: the
/// 2026-09-22 run is `RUN|start|2026-09-22T03:00:03` in the log and
/// `nightly-2026-09-22t03-49-08` in the graph. Two rows about one run with no
/// shared key, so no reader could check the graph's counts against the log's.
pub fn pipeline_run_name(started_at: &str) -> String {
    format!("nightly-{}", started_at.replace(':', "-"))
}

/// The inverse: the log runId a PipelineRun name points at, or None when the
/// name is not one this runner minted. Case-insensitive on the date separator
/// because the store lowercases names on the way in.
pub fn run_id_from_pipeline_run_name(name: &str) -> Option<String> {
    let rest = name.strip_prefix("nightly-")?;
    let (date, time) = rest.split_once(['t', 'T'])?;
    if date.len() != 10 || time.len() != 8 {
        return None;
    }
    Some(format!("{}T{}", date, time.replace('-', ":")))
}

/// #4271 — rows the summary's per-status fields did not account for. Suites
/// minus the sum of every emitted count. Zero is the only honest answer once
/// the buckets are derived from the rows; a non-zero means a row's verdict
/// reached the log and reached no field, which is how the 2026-09-22 03:00 run
/// reported 430 suites across fields summing to 422.
pub fn uncounted_rows(suites: usize, counted: usize) -> usize {
    suites.saturating_sub(counted)
}

/// The `nightly.run.summary` spine fields, the wrapper's `emit_run_summary`.
pub fn run_summary_fields(rows: &[SuiteRow]) -> Vec<(String, String)> {
    let count = |s: &str| rows.iter().filter(|r| r.status == s).count();
    let failed = count("fail");
    let mut owners: Vec<&str> = rows.iter().filter(|r| r.status == "fail").map(|r| r.owner.as_str()).collect();
    owners.sort_unstable();
    owners.dedup();
    let csv: Vec<String> = owners.iter().map(|o| format!("{}={}", o, rows.iter().filter(|r| r.status == "fail" && r.owner == *o).count())).collect();
    // The three fields Loki and the readout have always keyed on keep their
    // names and their places. Everything else is DERIVED from the rows (#4271):
    // a hand-written list named two statuses the rows never carry and left the
    // two they do ("unmeasured", "slow") in no field at all, so the summary
    // read 430 suites across fields summing to 422 and nothing looked wrong.
    let mut f = vec![
        ("suites".to_string(), rows.len().to_string()),
        ("passed".to_string(), count("pass").to_string()),
        ("failed".to_string(), failed.to_string()),
        ("skipped".to_string(), count("skip").to_string()),
    ];
    let mut counted = count("pass") + failed + count("skip");
    // #4168 — stale is counted on its own axis and deliberately NOT folded into
    // failed, so zero_red stays true on a run whose only non-pass rows are
    // stale. It now arrives through the same derivation as every other status.
    let mut rest: Vec<&str> = rows
        .iter()
        .map(|r| r.status.as_str())
        .filter(|s| !matches!(*s, "pass" | "fail" | "skip"))
        .collect();
    rest.sort_unstable();
    rest.dedup();
    for status in rest {
        let n = count(status);
        counted += n;
        f.push((status.to_string(), n.to_string()));
    }
    f.push(("uncounted".to_string(), uncounted_rows(rows.len(), counted).to_string()));
    f.push(("red_by_owner".to_string(), if csv.is_empty() { "none".into() } else { csv.join(";") }));
    f.push(("zero_red".to_string(), (failed == 0).to_string()));
    f
}

/// The pipeline-run record body (`emit_pipeline_run`): outcome and counts.
/// #4271 — the lane's own count of results it wrote: `nightly-stored|run|<n> of <m>`.
/// The `run` roll-up only; a per-unit line is one unit's slice, and summing
/// those plus the roll-up double-counts (18,834 for a 9,417-case night).
pub fn parse_run_stored_line(line: &str) -> Option<usize> {
    let rest = line.strip_prefix("nightly-stored|run|")?;
    rest.split(" of ").next()?.trim().parse().ok()
}

/// The test grain for a run's PipelineRun row, or None when there is no
/// reading to report. Absence is not zero (#3734): a night that measured
/// nothing must omit the fields, not claim 0 failures.
pub fn run_test_counts(
    cases: &[(String, String, String, String)],
    stored: Option<usize>,
) -> Option<RunTestCounts> {
    if cases.is_empty() {
        return None;
    }
    let stored = stored?;
    Some(RunTestCounts {
        run: cases.len(),
        failed: cases.iter().filter(|(_, _, r, _)| r == "fail").count(),
        stored,
    })
}

/// #4271 — the test-grain numbers a run measured, if it measured any. `None`
/// means no reading: the fields are OMITTED, never sent as zero. A zero is a
/// measurement and "0 tests failed" on an unmeasured night reads as green.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunTestCounts {
    pub run: usize,
    pub failed: usize,
    pub stored: usize,
}

/// The pipeline-run record body (`emit_pipeline_run`): outcome and counts.
///
/// #4271 — suites and tests are two units and now have two sets of names. The
/// row used to send `testsRun` / `testsFailed` / `testsStored` and put SUITE
/// counts in all three: the 2026-09-22 03:00 row reads testsRun 421,
/// testsFailed 17, testsStored 430 for a night that ran 9,417 cases, failed 51
/// and stored 9,417 results. `testsStored` is RETIRED rather than repaired
/// (Kade, 07:28) — 134 rows already carry it meaning suites and no reader can
/// date the change, so the old name keeps its old meaning as history.
pub fn pipeline_run_body(
    rows: &[SuiteRow],
    name: &str,
    trace: &str,
    duration_ms: u128,
    tests: Option<RunTestCounts>,
) -> String {
    let failed = rows.iter().filter(|r| r.status == "fail").count();
    let passed = rows.iter().filter(|r| r.status == "pass").count();
    let outcome = if failed == 0 { "green" } else { "red" };
    let mut body = format!(
        "{{\"name\":\"{}\",\"forPipeline\":\"pipeline-cicd\",\"traceId\":\"{}\",\"runOutcome\":\"{}\",\"runDurationMs\":\"{}\",\"suitesRun\":\"{}\",\"suitesFailed\":\"{}\",\"suitesTotal\":\"{}\"",
        name,
        trace,
        outcome,
        duration_ms,
        passed + failed,
        failed,
        rows.len()
    );
    if let Some(t) = tests {
        body.push_str(&format!(
            ",\"testsRun\":\"{}\",\"testsFailed\":\"{}\",\"resultsStored\":\"{}\"",
            t.run, t.failed, t.stored
        ));
    }
    body.push('}');
    body
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

    // #4271 — the graph row and the log run must share one id. A reader who
    // has the PipelineRun must be able to find the run in the log, and vice
    // versa, without guessing which of two timestamps was meant.
    #[test]
    fn the_pipeline_run_name_carries_the_log_runid() {
        let log = "RUN|start|2026-09-22T03:00:03|pid=10432\nSUITE|bats|a.bats|kade|pass|1 pass, 0 fail\nRUN|complete|2026-09-22T03:49:08|suites=430\n";
        let started = log.lines().next().unwrap().split('|').nth(2).unwrap();
        let name = super::pipeline_run_name(started);
        assert_eq!(name, "nightly-2026-09-22T03-00-03");
        assert_eq!(
            super::run_id_from_pipeline_run_name(&name).as_deref(),
            Some("2026-09-22T03:00:03"),
            "the name round-trips back to the id the log brackets the run with"
        );
        // the store lowercases names on the way in; the join must survive that
        assert_eq!(
            super::run_id_from_pipeline_run_name(&name.to_lowercase()).as_deref(),
            Some("2026-09-22T03:00:03")
        );
    }

    // NEGATIVE PROOF (#3734): the name the runner actually emitted on
    // 2026-09-22 — built at completion — must FAIL to join. Without this the
    // round-trip above passes for any self-consistent pair of stamps and says
    // nothing about whether the right stamp was chosen.
    #[test]
    fn negative_proof_a_completion_stamped_name_does_not_join_the_log_run() {
        let log_run_id = "2026-09-22T03:00:03";
        let as_shipped = format!("nightly-{}", "2026-09-22T03:49:08".replace(':', "-"));
        assert_eq!(as_shipped, "nightly-2026-09-22T03-49-08", "this is the row in the graph today");
        assert_ne!(
            super::run_id_from_pipeline_run_name(&as_shipped).as_deref(),
            Some(log_run_id),
            "a completion-stamped name must not resolve to the run's log id"
        );
        assert_eq!(super::pipeline_run_name(log_run_id), "nightly-2026-09-22T03-00-03");
    }

    // A name this runner did not mint must say so rather than inventing an id.
    #[test]
    fn negative_proof_a_foreign_name_yields_no_runid() {
        assert_eq!(super::run_id_from_pipeline_run_name("werk-4271-run-2"), None);
        assert_eq!(super::run_id_from_pipeline_run_name("nightly-nonsense"), None);
        assert_eq!(super::run_id_from_pipeline_run_name("nightly-2026-09-22T03-00"), None);
    }

    // #4271 — every row the run produced must reach a counted field. The
    // 2026-09-22 03:00 run reported 430 suites while nightly.run.summary's
    // fields summed to 422: run_summary_fields hand-listed "unmeasurable" and
    // "stale", which no row that night carried, and the 7 rows verdicted
    // "unmeasured" plus the 1 verdicted "slow" landed in no field at all. Both
    // hand-listed fields read 0, so nothing looked wrong.
    //
    // This is the run's own status set, verbatim.
    #[test]
    fn every_status_the_run_produced_reaches_a_counted_field() {
        let rows = vec![
            SuiteRow::new("bats", "a.bats", "kade", "pass", "1 pass, 0 fail"),
            SuiteRow::new("bats", "b.bats", "kade", "fail", "0 pass, 1 fail"),
            SuiteRow::new("bats", "c.bats", "kade", "skip", "skipped"),
            SuiteRow::new("perf", "d.sh", "silas", "slow", "0 pass, 1 fail"),
            SuiteRow::new("coverage", "e", "wren", "unmeasured", "0 pass, 0 fail (UNMEASURED)"),
        ];
        let fields = run_summary_fields(&rows);
        let get = |k: &str| fields.iter().find(|(f, _)| f == k).map(|(_, v)| v.clone()).unwrap_or_default();
        assert_eq!(get("suites"), "5");
        assert_eq!(get("slow"), "1", "a slow row must be counted, not vanish: {:?}", fields);
        assert_eq!(get("unmeasured"), "1", "an unmeasured row must be counted, not vanish: {:?}", fields);
        assert_eq!(get("uncounted"), "0", "every row reached a field: {:?}", fields);
    }

    // NEGATIVE PROOF (#3734) for the check above: a status the code has never
    // seen must still land in a field and still leave uncounted at 0. A field
    // list that is hand-written cannot satisfy this — which is exactly the
    // state that produced the 430-vs-422 gap.
    #[test]
    fn negative_proof_a_status_no_one_hand_listed_is_still_counted() {
        let rows = vec![
            SuiteRow::new("bats", "a.bats", "kade", "pass", "1 pass, 0 fail"),
            SuiteRow::new("bats", "z.bats", "kade", "wedged", "0 pass, 0 fail (a verdict word nobody listed)"),
        ];
        let fields = run_summary_fields(&rows);
        let get = |k: &str| fields.iter().find(|(f, _)| f == k).map(|(_, v)| v.clone()).unwrap_or_default();
        assert_eq!(get("wedged"), "1", "an unlisted status must get its own field: {:?}", fields);
        assert_eq!(get("uncounted"), "0", "and must not leave rows uncounted: {:?}", fields);
    }

    // The uncounted field must be able to say something other than 0, or it is
    // a hollow gate. Summing the emitted per-status counts against the suite
    // total is the arithmetic it performs; this pins that arithmetic.
    #[test]
    fn negative_proof_uncounted_is_reachable() {
        let counted = 422usize;
        let suites = 430usize;
        assert_eq!(super::uncounted_rows(suites, counted), 8, "the 2026-09-22 03:00 gap, in the arithmetic uncounted uses");
        assert_eq!(super::uncounted_rows(430, 430), 0);
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

    /// #4286 — the chorus-principal floor sits at the measured LOW, not 0.2 pt
    /// under a measurement that moves with the box (18.20 / 16.96 / 18.20 on
    /// unchanged code, 2026-09-23/24). Reads the real floors file so a hand
    /// edit back up re-fails here before it re-fails the nightly.
    #[test]
    fn chorus_principal_floor_covers_the_measured_low() {
        // #4292 — run-time, not compile-time: the 4030 guard refuses env!() (a shared
        // nightly target dir reuses binaries across werks).
        let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo test sets CARGO_MANIFEST_DIR");
        let yaml = std::fs::read_to_string(format!("{dir}/../../../coverage-floors.yml")).unwrap();
        let floor = parse_floors(&yaml).into_iter().find(|(_, rel, _)| rel == "platform/services/chorus-principal").map(|(_, _, f)| f).expect("chorus-principal has a floor");
        let measured_low = 16.959669079627716;
        assert_eq!(coverage_row("platform/services/chorus-principal", "silas", floor, 0, Some(measured_low)).status, "pass", "floor {} must cover the measured low", floor);
        // NEGATIVE PROOF: one point above the low, the same measurement is red.
        assert_eq!(coverage_row("platform/services/chorus-principal", "silas", floor + 1, 0, Some(measured_low)).status, "fail");
        assert_eq!(coverage_row("platform/services/chorus-principal", "silas", 18, 0, Some(measured_low)).status, "fail", "the 03:04 red reproduces at the old floor");
    }
    #[test]
    fn coverage_rows_keep_the_four_outcomes() {
        assert!(coverage_row("a", "kade", 80, 0, Some(80.5)).line().ends_with("pass|1 pass, 0 fail (coverage 80.5% >= floor 80%)"));
        // NEGATIVE PROOF: a real measurement below the floor is still a FAIL.
        // If this ever reads unmeasured, the check can no longer catch the one
        // state it exists for and #4265's change went too far.
        assert!(coverage_row("a", "kade", 80, 0, Some(79.0)).status == "fail");
        // #4265 — never measured is not the same as measured-and-bad.
        assert!(coverage_row("a", "kade", 80, 0, None).status == "unmeasured");
        assert!(coverage_row("a", "kade", 80, 0, None).summary.contains("NO summary artifact"));
        assert!(coverage_row("a", "kade", 80, 124, None).status == "unmeasured");
        assert!(coverage_row("a", "kade", 80, 124, None).summary.contains("rc=124"));
        // and an unmeasured row must not claim a pass either
        assert!(coverage_row("a", "kade", 80, 124, None).summary.contains("0 pass, 0 fail"));
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
        let body = pipeline_run_body(&rows, "nightly-x", "t", 5, None);
        assert!(body.contains("\"runOutcome\":\"red\""));
        // #4271 — suite counts live under SUITE names now
        assert!(body.contains("\"suitesRun\":\"2\""), "{body}");
        assert!(body.contains("\"suitesFailed\":\"1\""), "{body}");
        assert!(body.contains("\"suitesTotal\":\"3\""), "{body}");
    }

    // #4271 — the row lied in one unit. testsRun/testsFailed/testsStored all
    // carried SUITE counts: the 2026-09-22 03:00 row says testsRun 421 and
    // testsFailed 17 for a night that ran 9,417 cases and failed 51, and
    // testsStored said 430 when 9,417 results were stored.
    //
    // Kade's ruling, 07:28: retire testsStored rather than repair its meaning.
    // 134 rows already carry it meaning suites and no reader can date the
    // change, so the old name keeps its old meaning as history.
    #[test]
    fn the_body_states_suites_and_tests_under_their_own_names() {
        let rows = vec![
            SuiteRow::new("bats", "a", "wren", "fail", "0 pass, 1 fail"),
            SuiteRow::new("bats", "b", "kade", "pass", "1 pass, 0 fail"),
        ];
        let body = pipeline_run_body(&rows, "n", "t", 5, Some(super::RunTestCounts { run: 9417, failed: 51, stored: 9417 }));
        for want in [
            "\"suitesRun\":\"2\"",
            "\"suitesFailed\":\"1\"",
            "\"suitesTotal\":\"2\"",
            "\"testsRun\":\"9417\"",
            "\"testsFailed\":\"51\"",
            "\"resultsStored\":\"9417\"",
        ] {
            assert!(body.contains(want), "missing {want} in {body}");
        }
        assert!(!body.contains("testsStored"), "the retired name must not be written again: {body}");
    }

    // NEGATIVE PROOF (#3734) — a run with no test-grain reading must OMIT the
    // test fields, never send zero. Zero is a measurement; absence is not, and
    // "0 tests failed" on a night nothing was measured reads as green.
    #[test]
    fn negative_proof_no_tally_omits_the_test_fields_rather_than_sending_zero() {
        let rows = vec![SuiteRow::new("bats", "a", "kade", "pass", "1 pass, 0 fail")];
        let body = pipeline_run_body(&rows, "n", "t", 5, None);
        assert!(body.contains("\"suitesRun\":\"1\""), "the suite grain is still there: {body}");
        assert!(!body.contains("testsRun"), "no tests reading, no testsRun field: {body}");
        assert!(!body.contains("testsFailed"), "{body}");
        assert!(!body.contains("resultsStored"), "{body}");
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

    // #4199 — the morning line rides along when the pass printed it, verbatim;
    // NEGATIVE PROOF: a LOSSY verdict is not hidden behind "clean".
    #[test]
    fn the_graph_vs_project_verdict_rides_the_crawl_clause() {
        let log = "chorus-crawl: full · tracked=6214 read=Complete\nchorus-crawl: wrote=0 failed=0\nchorus-crawl: graph vs project · complete files=5575/6214 no-kind=642 (png 233) cases=8236 no-case=9 logs=90 rows/40 files · current lag=0 · consistent files=clean cases=clean logs=DRIFT 12 · LOSSY lands=3 uncovered=1 (abc1234)\nchorus-crawl: watermark -> 8f8ade821\n";
        let line = crawl_line(Some(log));
        assert!(line.starts_with(" — crawl: 0 written, clean — graph vs project: complete files=5575/6214"), "{line}");
        assert!(line.contains("LOSSY lands=3 uncovered=1 (abc1234)"), "{line}");
        assert!(line.contains("logs=DRIFT 12"), "{line}");
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

/// #4247 — one unit for test reporting: the registered test.
#[cfg(test)]
mod one_unit_4247 {
    use super::{parse_case_line, registered_test_tally, tests_with_no_result};

    fn reg(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(f, n)| (f.to_string(), n.to_string())).collect()
    }

    /// A case answering for the registered row of its own name — the ordinary
    /// shape. `ran_as` below is the `describe.each` shape, where the two differ.
    fn ran(triples: &[(&str, &str, &str)]) -> Vec<(String, String, String, String)> {
        triples
            .iter()
            .map(|(f, n, r)| (f.to_string(), n.to_string(), r.to_string(), n.to_string()))
            .collect()
    }

    fn ran_as(quads: &[(&str, &str, &str, &str)]) -> Vec<(String, String, String, String)> {
        quads
            .iter()
            .map(|(f, n, r, g)| (f.to_string(), n.to_string(), r.to_string(), g.to_string()))
            .collect()
    }

    /// NEGATIVE PROOF — the 2026-09-20 case. A registered test that silently
    /// does not run read exactly like one that passed; the gap (154 of 8,691)
    /// was nameless and had to be inferred by matching two lists.
    #[test]
    fn a_registered_test_that_never_ran_is_named() {
        let registered = reg(&[("a.rs", "one"), ("a.rs", "two"), ("b.bats", "three")]);
        let ran = ran(&[("a.rs", "one", "pass"), ("b.bats", "three", "fail")]);
        assert_eq!(
            tests_with_no_result(&registered, &ran),
            reg(&[("a.rs", "two")])
        );
    }

    /// Nothing missing is an empty list, not a silence.
    #[test]
    fn a_full_run_names_nothing() {
        let registered = reg(&[("a.rs", "one")]);
        let ran = ran(&[("a.rs", "one", "pass")]);
        assert!(tests_with_no_result(&registered, &ran).is_empty());
    }

    /// NEGATIVE PROOF — a three-field line (written before this card) parses
    /// with an EMPTY result and is counted unmeasured, never as a pass.
    #[test]
    fn a_line_without_a_result_is_unmeasured_not_passed() {
        assert_eq!(
            parse_case_line("nightly-case|a.rs|one"),
            Some(("a.rs".into(), "one".into(), String::new(), "one".into()))
        );
        let line = registered_test_tally(&reg(&[("a.rs", "one")]), &ran(&[("a.rs", "one", "")]));
        assert!(line.contains("unmeasured 1"), "{line}");
        assert!(line.contains("passed 0"), "{line}");
    }

    /// The four-field line carries its verdict.
    #[test]
    fn a_line_with_a_result_carries_it() {
        assert_eq!(
            parse_case_line("nightly-case|a.rs|one|fail"),
            Some(("a.rs".into(), "one".into(), "fail".into(), "one".into()))
        );
    }

    /// NEGATIVE PROOF — a jest it-name may contain '|'. Splitting left-to-right
    /// cut the name in half and called the rest the result ("b pair|pass").
    /// The result is the last field ONLY when it is a verdict word.
    #[test]
    fn a_pipe_in_the_test_name_keeps_the_name_and_the_result() {
        assert_eq!(
            parse_case_line("nightly-case|a.rs|reads a|b pair|pass"),
            Some(("a.rs".into(), "reads a|b pair".into(), "pass".into(), "reads a|b pair".into()))
        );
        assert_eq!(
            parse_case_line("nightly-case|a.rs|reads a|b pair"),
            Some(("a.rs".into(), "reads a|b pair".into(), String::new(), "reads a|b pair".into()))
        );
    }

    /// #4271 — the four-field line: identity, verdict, and the BYTE LENGTH of
    /// the registered name inside that identity. 42 cases generated by one
    /// `describe.each` block all answer for the one registered row, and each
    /// keeps the name it ran under.
    #[test]
    fn a_dynamic_case_line_carries_its_identity_and_its_join_key() {
        let line = "nightly-case|cards.test.ts|matrix add P1 runs without throwing|pass|21";
        assert_eq!(
            parse_case_line(line),
            Some((
                "cards.test.ts".into(),
                "matrix add P1 runs without throwing".into(),
                "pass".into(),
                "runs without throwing".into()
            ))
        );
        // and the census joins on the registered row, so ONE registration is
        // answered for by all of its generated cases
        let registered = reg(&[("cards.test.ts", "runs without throwing")]);
        let ran = ran_as(&[
            ("cards.test.ts", "matrix add P1 runs without throwing", "pass", "runs without throwing"),
            ("cards.test.ts", "matrix add P2 runs without throwing", "pass", "runs without throwing"),
        ]);
        assert!(tests_with_no_result(&registered, &ran).is_empty(), "the registered row DID run");
        assert!(registered_test_tally(&registered, &ran).contains("ran 2"), "and two cases ran");
    }

    /// NEGATIVE PROOF (#3734) — matching the census on the case's OWN name is
    /// the phantom Kade flagged: every generated case would answer for nothing
    /// and the registered row would read as never-ran. This pins that the join
    /// uses the fourth field, by showing the wrong join's result is different.
    #[test]
    fn negative_proof_joining_on_the_case_name_would_report_the_row_as_never_ran() {
        let registered = reg(&[("cards.test.ts", "runs without throwing")]);
        let as_emitted = ran_as(&[("cards.test.ts", "matrix add P1 runs without throwing", "pass", "runs without throwing")]);
        let joined_on_case_name = ran(&[("cards.test.ts", "matrix add P1 runs without throwing", "pass")]);
        assert!(tests_with_no_result(&registered, &as_emitted).is_empty());
        assert_eq!(
            tests_with_no_result(&registered, &joined_on_case_name),
            reg(&[("cards.test.ts", "runs without throwing")]),
            "joining on the case name reports the row as never-ran — the state this field exists to prevent"
        );
    }

    /// NEGATIVE PROOF — a length that does not land on a character boundary of
    /// the name must REFUSE, yielding no join key, never a sliced one. A sliced
    /// key names no registry row and reads as a phantom never-ran.
    #[test]
    fn negative_proof_a_suffix_length_that_misses_a_boundary_refuses() {
        // "é" is two bytes; a length of 1 lands inside it
        let line = "nightly-case|a.rs|caf\u{e9}|pass|1";
        let (_, name, result, reg_name) = parse_case_line(line).expect("the line still parses");
        assert_eq!(result, "pass");
        assert_eq!(name, "caf\u{e9}");
        assert_eq!(reg_name, "", "no join key rather than a sliced one");
        // control: a length that DOES land on a boundary resolves
        let ok = "nightly-case|a.rs|suite saves the file|pass|14";
        assert_eq!(parse_case_line(ok).unwrap().3, "saves the file");
    }

    /// A name that merely ENDS in a pipe-digit run is not a suffix length.
    #[test]
    fn negative_proof_a_name_ending_in_pipe_digits_is_not_read_as_a_length() {
        assert_eq!(
            parse_case_line("nightly-case|a.rs|exits with code|127"),
            Some(("a.rs".into(), "exits with code|127".into(), String::new(), "exits with code|127".into())),
            "no verdict before the digits, so the digits are part of the name"
        );
    }

    /// Every count in the tally is the same unit — the registered test.
    #[test]
    fn the_tally_speaks_one_unit() {
        let registered = reg(&[("a.rs", "one"), ("a.rs", "two"), ("b.bats", "three")]);
        let ran = ran(&[("a.rs", "one", "pass"), ("b.bats", "three", "fail")]);
        assert_eq!(
            registered_test_tally(&registered, &ran),
            "registered 3 · ran 2 · passed 1 · failed 1 · unmeasured 0 · no result 1"
        );
    }
}

/// #4251 — the most recent scheduled slot at or before `now`, as an ISO stamp.
///
/// Both are `YYYY-MM-DDTHH:MM:SS` local, which compares lexicographically. If
/// today's slot has not arrived yet, the answer is yesterday's — so a readout
/// at 02:00 is measured against the 03:00 slot of the night before, not one
/// that has not happened.
///
/// Slots are (hour, minute) from the job's own schedule. Deliberately NOT an
/// hours threshold: Wren, 2026-09-21, "an hours threshold is another
/// uncommented 86400 — the schedule already answers it".
pub fn last_slot_before(now: &str, slots: &[(u32, u32)]) -> Option<String> {
    let (date, time) = now.split_once('T')?;
    let mut today: Vec<String> = slots
        .iter()
        .map(|(h, m)| format!("{date}T{h:02}:{m:02}:00"))
        .collect();
    today.sort();
    if let Some(s) = today.iter().rev().find(|s| s.as_str() <= now) {
        return Some(s.clone());
    }
    // none today yet — the last one is the latest slot of the previous day
    let prev = previous_day(date)?;
    let mut y: Vec<String> = slots
        .iter()
        .map(|(h, m)| format!("{prev}T{h:02}:{m:02}:00"))
        .collect();
    y.sort();
    let _ = time;
    y.pop()
}

/// `YYYY-MM-DD` minus one day. Local dates only; no timezone maths, because
/// the stamps compared here are all written by the same `date` call.
fn previous_day(date: &str) -> Option<String> {
    let mut p = date.split('-');
    let (y, m, d) = (
        p.next()?.parse::<i32>().ok()?,
        p.next()?.parse::<u32>().ok()?,
        p.next()?.parse::<u32>().ok()?,
    );
    let leap = |y: i32| (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = |y: i32, m: u32| match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap(y) => 29,
        2 => 28,
        _ => 0,
    };
    let (y, m, d) = if d > 1 {
        (y, m, d - 1)
    } else if m > 1 {
        (y, m - 1, days(y, m - 1))
    } else {
        (y - 1, 12, 31)
    };
    if d == 0 {
        return None;
    }
    Some(format!("{y:04}-{m:02}-{d:02}"))
}

/// #4251 — the line that REPLACES the counts when no run has finished since
/// the last scheduled slot.
///
/// On 2026-09-21 the 03:00 job never launched (launchd held a stale code
/// signature after a 21:10 deploy) and the 06:05 readout replayed the 21:11
/// run as if it were the night's. Jeff read four reds as today's. A stale
/// readout that looks fresh is worse than no readout.
///
/// `last_complete` is the stamp of the most recent `RUN|complete|`, if any.
/// Returns None when a run HAS completed since the slot — the counts stand.
pub fn unmeasured_since_slot(
    now: &str,
    slots: &[(u32, u32)],
    last_complete: Option<&str>,
    last_summary: &str,
    reason: Option<&str>,
) -> Option<String> {
    let slot = last_slot_before(now, slots)?;
    if let Some(c) = last_complete {
        if c >= slot.as_str() {
            return None;
        }
    }
    let slot_hm = slot.split('T').nth(1).unwrap_or("").get(..5).unwrap_or("");
    let mut line = format!("UNMEASURED — no run since the {slot_hm} slot");
    match last_complete {
        Some(c) => line.push_str(&format!("\nlast completed: {c}, {last_summary}")),
        None => line.push_str("\nlast completed: never"),
    }
    if let Some(r) = reason {
        line.push_str(&format!("\nreason: {r}"));
    }
    Some(line)
}

/// #4251 — a readout with no run since its slot says so, in place of counts.
#[cfg(test)]
mod unmeasured_since_slot_4251 {
    use super::{last_slot_before, unmeasured_since_slot};

    const NIGHTLY: &[(u32, u32)] = &[(3, 0)];

    /// NEGATIVE PROOF — 2026-09-21 exactly. The 03:00 job never launched and
    /// the 06:05 readout replayed the 21:11 run as the night's; Jeff read four
    /// reds as today's.
    #[test]
    fn a_morning_with_no_run_since_the_slot_is_unmeasured() {
        let line = unmeasured_since_slot(
            "2026-09-21T06:05:00",
            NIGHTLY,
            Some("2026-09-20T22:00:55"),
            "4 red",
            Some("launchd OS_REASON_CODESIGNING"),
        )
        .expect("must be unmeasured");
        assert!(line.starts_with("UNMEASURED — no run since the 03:00 slot"), "{line}");
        assert!(line.contains("last completed: 2026-09-20T22:00:55, 4 red"), "{line}");
        assert!(line.contains("reason: launchd OS_REASON_CODESIGNING"), "{line}");
    }

    /// A run that finished after the slot is measured — the counts stand and
    /// no line is produced.
    #[test]
    fn a_run_after_the_slot_is_measured() {
        assert_eq!(
            unmeasured_since_slot(
                "2026-09-21T06:05:00",
                NIGHTLY,
                Some("2026-09-21T04:12:00"),
                "0 red",
                None
            ),
            None
        );
    }

    /// Before today's slot, the measure is LAST night's slot — a 02:00 reader
    /// is not told a 03:00 run is missing when 03:00 has not arrived.
    #[test]
    fn before_todays_slot_the_measure_is_last_nights() {
        assert_eq!(
            last_slot_before("2026-09-21T02:00:00", NIGHTLY).as_deref(),
            Some("2026-09-20T03:00:00")
        );
        assert_eq!(
            unmeasured_since_slot(
                "2026-09-21T02:00:00",
                NIGHTLY,
                Some("2026-09-20T04:00:00"),
                "0 red",
                None
            ),
            None
        );
    }

    /// Never having run is its own state, not a missing line.
    #[test]
    fn a_log_with_no_completed_run_says_never() {
        let line = unmeasured_since_slot("2026-09-21T06:05:00", NIGHTLY, None, "", None)
            .expect("must be unmeasured");
        assert!(line.contains("last completed: never"), "{line}");
    }

    /// Month and year boundaries: the previous day is real arithmetic, not a
    /// subtraction on the day field.
    #[test]
    fn the_previous_day_crosses_month_and_year() {
        assert_eq!(
            last_slot_before("2026-03-01T02:00:00", NIGHTLY).as_deref(),
            Some("2026-02-28T03:00:00")
        );
        assert_eq!(
            last_slot_before("2024-03-01T02:00:00", NIGHTLY).as_deref(),
            Some("2024-02-29T03:00:00")
        );
        assert_eq!(
            last_slot_before("2026-01-01T02:00:00", NIGHTLY).as_deref(),
            Some("2025-12-31T03:00:00")
        );
    }

    /// Two slots a day: the measure is the most recent one passed.
    #[test]
    fn two_slots_measure_against_the_later_one() {
        let slots = &[(6, 0), (13, 30)];
        assert_eq!(
            last_slot_before("2026-09-21T14:00:00", slots).as_deref(),
            Some("2026-09-21T13:30:00")
        );
        assert_eq!(
            last_slot_before("2026-09-21T07:00:00", slots).as_deref(),
            Some("2026-09-21T06:00:00")
        );
    }
}

/// #4251 — the job's own schedule, read from its launchd plist.
///
/// The schedule is the source of truth for "when should a run have happened".
/// Hardcoding it here would be the same uncommented constant in a second
/// place, and it would drift the first time the slot moves.
///
/// Handles both shapes launchd accepts: a single `StartCalendarInterval` dict,
/// and an array of them. A plist with no Hour key yields no slots, and the
/// caller then makes no claim about staleness.
pub fn slots_from_plist(xml: &str) -> Vec<(u32, u32)> {
    // Tag scan, not line parsing: launchd plists are written both one-key-per
    // -line and all on one line, and the first version of this read only the
    // first shape — it returned no slots for the array form and would have
    // made the readout silently claim nothing.
    #[derive(Clone, Copy)]
    enum Tok {
        Hour(u32),
        Minute(u32),
    }
    let mut toks: Vec<Tok> = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find("<key>") {
        let after = &rest[i + 5..];
        let Some(j) = after.find("</key>") else { break };
        let key = &after[..j];
        let tail = &after[j + 6..];
        if key == "Hour" || key == "Minute" {
            if let Some(a) = tail.find("<integer>") {
                let v = &tail[a + 9..];
                if let Some(b) = v.find("</integer>") {
                    if let Ok(n) = v[..b].trim().parse::<u32>() {
                        toks.push(if key == "Hour" { Tok::Hour(n) } else { Tok::Minute(n) });
                    }
                }
            }
        }
        rest = tail;
    }
    let mut out: Vec<(u32, u32)> = Vec::new();
    let mut cur: Option<(u32, u32)> = None;
    for t in toks {
        match t {
            Tok::Hour(h) => {
                if let Some(c) = cur.take() {
                    out.push(c);
                }
                cur = Some((h, 0));
            }
            Tok::Minute(m) => {
                if let Some(c) = cur.as_mut() {
                    c.1 = m;
                }
            }
        }
    }
    if let Some(c) = cur {
        out.push(c);
    }
    out.sort();
    out.dedup();
    out
}

/// #4251 — the stamp of the most recent completed run in a nightly log.
pub fn last_complete_stamp(log: &str) -> Option<String> {
    log.lines()
        .rev()
        .find(|l| l.starts_with("RUN|complete|"))
        .and_then(|l| l.split('|').nth(2))
        .map(str::to_string)
}

/// #4251 — the schedule comes from the job's plist, not a constant.
#[cfg(test)]
mod slots_from_plist_4251 {
    use super::{last_complete_stamp, slots_from_plist};

    const ONE: &str = r#"<dict>
  <key>StartCalendarInterval</key>
  <dict>
    <key>Hour</key><integer>3</integer>
    <key>Minute</key><integer>0</integer>
  </dict>
</dict>"#;

    const TWO: &str = r#"<dict>
  <key>StartCalendarInterval</key>
  <array>
    <dict><key>Hour</key><integer>6</integer><key>Minute</key><integer>0</integer></dict>
    <dict><key>Hour</key><integer>13</integer><key>Minute</key><integer>30</integer></dict>
  </array>
</dict>"#;

    #[test]
    fn a_single_interval_reads() {
        assert_eq!(slots_from_plist(ONE), vec![(3, 0)]);
    }

    #[test]
    fn an_array_of_intervals_reads_all_of_them() {
        assert_eq!(slots_from_plist(TWO), vec![(6, 0), (13, 30)]);
    }

    /// NEGATIVE PROOF: a plist with no schedule yields NO slots, so the caller
    /// makes no staleness claim at all. Defaulting to 03:00 here would invent
    /// a schedule the job does not have and call every run late.
    #[test]
    fn no_schedule_yields_no_slots() {
        assert!(slots_from_plist("<dict><key>Label</key><string>x</string></dict>").is_empty());
        assert!(slots_from_plist("").is_empty());
    }

    /// An hour with no minute is on the hour, not dropped.
    #[test]
    fn an_hour_without_a_minute_is_on_the_hour() {
        assert_eq!(
            slots_from_plist("<dict><key>Hour</key><integer>4</integer></dict>"),
            vec![(4, 0)]
        );
    }

    #[test]
    fn the_last_completed_run_is_the_one_read() {
        let log = "RUN|start|2026-09-20T03:00:03|pid=1\nRUN|complete|2026-09-20T04:10:00|suites=2\nRUN|start|2026-09-20T21:11:21|pid=2\nRUN|complete|2026-09-20T22:00:55|suites=424\n";
        assert_eq!(last_complete_stamp(log).as_deref(), Some("2026-09-20T22:00:55"));
        assert_eq!(last_complete_stamp("RUN|start|2026-09-20T03:00:03|pid=1\n"), None);
    }
}
