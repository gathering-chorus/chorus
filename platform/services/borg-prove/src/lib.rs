//! borg-prove (#4456). The pure half: CardAC rows in, a result per AC out, and
//! the route a failed step takes. main.rs does the I/O (store, HTTP, dagu).

/// One acceptance criterion and the check that proves it (a CardAC row).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub ac: String,        // the CardAC row's name, ac-<card>-<n> (e.g. ac-4456-2)
    pub statement: String, // the AC sentence as the card says it
    pub kind: String,      // checkKind: http | sparql | test | none
    pub target: String,    // checkTarget: a URL (http), an ASK query (sparql), a test name (test)
    pub expect: String,    // checkExpect: text the answer must hold; sparql: true|false
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Met(String),
    NotMet(String),
    /// No runnable check, or the system did not answer: never counted as met.
    Unmeasured(String),
}

impl Outcome {
    pub fn word(&self) -> &'static str {
        match self {
            Outcome::Met(_) => "met",
            Outcome::NotMet(_) => "not met",
            Outcome::Unmeasured(_) => "unmeasured",
        }
    }
    pub fn evidence(&self) -> &str {
        match self {
            Outcome::Met(e) | Outcome::NotMet(e) | Outcome::Unmeasured(e) => e,
        }
    }
}

/// What a probe of the running system saw. None = it did not answer.
pub type Seen = Option<String>;

/// Judge one check from what its probe saw. Pure: the probe is done elsewhere.
pub fn judge(c: &Check, seen: &Seen) -> Outcome {
    match c.kind.as_str() {
        "none" | "" => Outcome::Unmeasured("no runnable check on this AC".into()),
        "http" | "sparql" | "test" => {
            let Some(body) = seen else {
                return Outcome::Unmeasured(if c.kind == "test" {
                    format!("test {}: no result yet (no case event in Loki in 30 days)", c.target)
                } else {
                    format!("{} {} did not answer", c.kind, c.target)
                });
            };
            let want = c.expect.trim();
            if want.is_empty() {
                return Outcome::Unmeasured("the check names nothing to expect".into());
            }
            let got = if c.kind == "sparql" { ask_value(body) } else { None };
            let ok = match got {
                Some(b) => b.to_string() == want,
                // a status or a test result is the whole answer, not a substring
                None if is_status(want) || c.kind == "test" => body.trim() == want,
                None => body.contains(want),
            };
            let line = first_line_with(body, want);
            if ok {
                Outcome::Met(format!("{} {} → {}", c.kind, c.target, line))
            } else {
                Outcome::NotMet(format!("{} {} → expected {:?}, saw {}", c.kind, c.target, want, short(body)))
            }
        }
        other => Outcome::Unmeasured(format!("unknown check kind '{other}'")),
    }
}

/// An ASK result's boolean (`{"boolean": true}`), if the body is one.
fn ask_value(body: &str) -> Option<bool> {
    let i = body.find("\"boolean\"")?;
    let rest = body[i + 9..].trim_start().strip_prefix(':')?.trim_start();
    if rest.starts_with("true") { Some(true) } else if rest.starts_with("false") { Some(false) } else { None }
}

fn first_line_with(body: &str, want: &str) -> String {
    body.lines().find(|l| l.contains(want)).map(|l| short(l.trim())).unwrap_or_else(|| short(body))
}

fn short(s: &str) -> String {
    let s = s.replace('\n', " ");
    if s.chars().count() > 160 { s.chars().take(160).collect::<String>() + "…" } else { s }
}

/// Intake (Proving step 1): a card is Rejected when it has no AC, or an AC
/// has no runnable check. Every gap is named.
pub fn intake(checks: &[Check]) -> Result<(), Vec<String>> {
    if checks.is_empty() {
        return Err(vec!["the card has no CardAC rows: every AC needs a check that can fail".into()]);
    }
    let errs: Vec<String> = checks
        .iter()
        .filter(|c| matches!(c.kind.as_str(), "none" | "") || c.target.trim().is_empty() || c.expect.trim().is_empty())
        .map(|c| format!("{}: no runnable check ({:?})", c.ac, c.statement))
        .collect();
    if errs.is_empty() { Ok(()) } else { Err(errs) }
}

/// #4683 — a card with no CardAC rows has nothing to prove: it lands as
/// before, and the record says so. Never "Proven".
pub const NO_ROWS: &str = "unproven: no CardAC rows";

/// The verdict over every AC: Proven only when every one is met; Not met when
/// any is not met; Unproven when none failed but some could not be measured
/// (an unmeasured AC is never a pass, and never a regression either).
pub fn verdict(results: &[(Check, Outcome)]) -> &'static str {
    if results.iter().any(|(_, o)| matches!(o, Outcome::NotMet(_))) {
        "Not met"
    } else if !results.is_empty() && results.iter().all(|(_, o)| matches!(o, Outcome::Met(_))) {
        "Proven"
    } else {
        "Unproven"
    }
}

/// `step=route,…` (what chorus-make writes) → pairs.
pub fn parse_routes(s: &str) -> Vec<(String, String)> {
    s.split(',')
        .filter_map(|p| p.split_once('='))
        .map(|(a, b)| (a.trim().to_string(), b.trim().to_string()))
        .filter(|(a, b)| !a.is_empty() && !b.is_empty())
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// before the land: the card goes back to the step that owns the fix
    Reopen { step: String },
    /// after the land, routed into Operating: a ticket card, keyed so a repeat
    /// raises its count instead of opening another
    Ticket { step: String, key: String },
    /// the failed step names no route: say so, never guess
    Unrouted,
}

/// Operating's steps (value-stream-operating). A route into one opens a ticket.
pub const OPERATING: [&str; 6] = [
    "value-stream-step-borg-plant", "value-stream-step-borg-tend", "value-stream-step-borg-read",
    "value-stream-step-borg-surface", "value-stream-step-borg-prune", "value-stream-step-borg-feed",
];

pub fn route_for(failed_step: &str, routes: &[(String, String)], role: &str) -> Route {
    match routes.iter().find(|(s, _)| s == failed_step) {
        None => Route::Unrouted,
        Some((s, to)) if OPERATING.contains(&to.as_str()) => {
            Route::Ticket { step: to.clone(), key: format!("{role}|{s}|failed") }
        }
        Some((_, to)) => Route::Reopen { step: to.clone() },
    }
}

/// A card's CardAC rows (Wren's shape, #4456): acFor · acIndex · acStatement ·
/// checkKind · checkTarget · checkExpect, one row per AC, in AC order.
pub const CHECKS_QUERY: &str = r#"PREFIX chorus: <https://jeffbridwell.com/chorus#>
SELECT ?ac ?statement ?kind ?target ?expect WHERE {
  GRAPH ?g { ?ac a chorus:CardAC ; chorus:acFor chorus:card-{CARD} ; chorus:acIndex ?i .
             OPTIONAL { ?ac chorus:acStatement ?statement }
             OPTIONAL { ?ac chorus:checkKind ?kind }
             OPTIONAL { ?ac chorus:checkTarget ?target }
             OPTIONAL { ?ac chorus:checkExpect ?expect } }
}
ORDER BY xsd:integer(?i)
"#;

pub fn checks_query(card: u32) -> String {
    format!("PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>\n{}", CHECKS_QUERY.replace("{CARD}", &card.to_string()))
}

/// CardAC rows from SPARQL TSV: ?ac ?statement ?kind ?target ?expect.
pub fn checks_from_tsv(tsv: &str) -> Result<Vec<Check>, String> {
    let mut lines = tsv.lines();
    let header = lines.next().unwrap_or("");
    if !header.starts_with("?ac") {
        return Err(format!("not a CardAC result: header {header:?}"));
    }
    Ok(lines
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let mut f: Vec<String> = l.split('\t').map(term).collect();
            f.resize(5, String::new());
            Check { ac: local(&f[0]), statement: f[1].clone(), kind: f[2].clone(), target: f[3].clone(), expect: f[4].clone() }
        })
        .collect())
}

fn local(s: &str) -> String {
    s.rsplit(['#', '/']).next().unwrap_or(s).to_string()
}

fn term(t: &str) -> String {
    let t = t.trim();
    if let Some(iri) = t.strip_prefix('<').and_then(|x| x.strip_suffix('>')) {
        return iri.to_string();
    }
    if let Some(rest) = t.strip_prefix('"') {
        if let Some(end) = rest.rfind('"') {
            return rest[..end].replace("\\\"", "\"").replace("\\\\", "\\").replace("\\t", " ").replace("\\n", "\n");
        }
    }
    t.to_string()
}

/// The first step `dagu status` marks failed, by name; empty when none is.
pub fn failed_in_status(status: &str) -> String {
    status
        .lines()
        .find(|l| l.contains("[failed]"))
        .map(|l| l.trim_start_matches(|c: char| "├└│─ ".contains(c)))
        .and_then(|l| l.split_whitespace().next())
        .unwrap_or_default()
        .to_string()
}

/// Three digits: an HTTP status code.
pub fn is_status(s: &str) -> bool {
    let s = s.trim();
    s.len() == 3 && s.chars().all(|c| c.is_ascii_digit())
}

/// The single value of a one-column SPARQL TSV result; None when it has no row.
pub fn tsv_value(tsv: &str) -> Option<String> {
    let v = tsv.lines().nth(1)?.split('\t').next()?.trim();
    (!v.is_empty()).then(|| term(v))
}

/// #4536 — a `test` check's query: the test's case events, named by file or
/// by case, from runs of THE CARD BEING PROVED only (Silas 08:11: the latest of
/// any run read Abby's #4530 branch as 3706's result while main had it red).
/// Matched on the exact event after cheap line filters; the target is escaped
/// so it stays one string inside the LogQL.
pub fn test_logql(target: &str, card: u32) -> String {
    let t = target.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        // cheap line filters (card, target) before `| json`: a json stage over
        // every case line in the window does not answer (test-diff, 10-15).
        // card_id is a JSON number on card runs ("card_id":4530); the exact
        // match is the label filter after json, so 45301 never answers for 4530.
        "{{job=\"platform-chorus\", appName=\"chorus-events\"}} |= \"\\\"card_id\\\":{card}\" |= \"{t}\" |~ \"test.case.(passed|failed)\" | json | event=~\"test.case.(passed|failed)\" | card_id=\"{card}\" | file=\"{t}\" or case=\"{t}\""
    )
}

/// #4536 — the newest result in a Loki query_range answer: "pass" or "fail".
/// No events (or an unreadable answer) is None — never a pass.
pub fn latest_case_result(body: &str) -> Option<String> {
    let mut best: Option<(u128, &'static str)> = None;
    let mut rest = body;
    while let Some(i) = rest.find("[\"") {
        rest = &rest[i + 2..];
        let Some(end) = rest.find('"') else { break };
        let Ok(ts) = rest[..end].parse::<u128>() else { continue };
        rest = &rest[end + 1..];
        let Some(open) = rest.find('"') else { break };
        rest = &rest[open + 1..];
        // the log line, JSON-escaped inside the answer
        let mut line = String::new();
        let mut close = rest.len();
        let mut chars = rest.char_indices();
        while let Some((k, c)) = chars.next() {
            match c {
                '\\' => { if let Some((_, n)) = chars.next() { line.push(n); } }
                '"' => { close = k; break; }
                _ => line.push(c),
            }
        }
        rest = &rest[close..];
        let result = if line.contains("\"event\":\"test.case.failed\"") { "fail" }
            else if line.contains("\"event\":\"test.case.passed\"") { "pass" }
            else { continue };
        if best.map_or(true, |(t, _)| ts > t) {
            best = Some((ts, result));
        }
    }
    best.map(|(_, r)| r.to_string())
}

/// #4683 AC3 — the ACs a red verdict files children for: every one not met
/// or not measured.
pub fn reds(results: &[(Check, Outcome)]) -> Vec<&(Check, Outcome)> {
    results.iter().filter(|(_, o)| !matches!(o, Outcome::Met(_))).collect()
}

/// The child's title names the parent and the AC, so a rerun finds it.
pub fn child_title(parent: u32, c: &Check) -> String {
    let s: String = c.statement.split_whitespace().collect::<Vec<_>>().join(" ");
    let s = if s.chars().count() > 80 { format!("{}…", s.chars().take(79).collect::<String>().trim_end()) } else { s };
    format!("#{parent} {} not proven in prod: {s}", c.ac)
}

/// The child's description carries the card floor (Experience + AC) and
/// borg-prove's evidence.
pub fn child_desc(parent: u32, c: &Check, o: &Outcome) -> String {
    format!(
        "## Experience\nborg-prove checked #{parent} in prod after its land and {} {} ({} check on {}).\n\nEvidence: {}\n\n## AC\n- [ ] {}: {}\n- [ ] borg-prove reads it met in prod\n",
        c.ac, o.word(), c.kind, c.target, o.evidence(), c.ac, c.statement
    )
}

/// The parent's owner from `cards view` ("  Owner:    Kade"), lowercased.
pub fn owner_of(view: &str) -> Option<String> {
    view.lines().find_map(|l| l.trim().strip_prefix("Owner:")).map(|o| o.trim().to_lowercase()).filter(|o| !o.is_empty())
}

/// Titles of the parent's children from `cards view` ("  Children (2):" then
/// "    #4700 [Later] title" lines). None when the count the view states is not
/// the count read (Silas, Abby at the demo): a format change must refuse,
/// never silently file a second child.
pub fn child_titles(view: &str) -> Option<Vec<String>> {
    let mut lines = view.lines().map(str::trim);
    let Some(head) = lines.by_ref().find(|t| t.starts_with("Children (")) else { return Some(Vec::new()) };
    let n: usize = head.trim_start_matches("Children (").split(')').next()?.parse().ok()?;
    let out: Vec<String> = lines.take(n)
        .filter_map(|t| t.strip_prefix('#').and_then(|r| r.split_once("] ")).map(|(_, title)| title.to_string()))
        .collect();
    (out.len() == n).then_some(out)
}

/// #4683 (Silas, Wren in the demo reviews) — record must judge what prod
/// measured, not measure again: prod writes its results, record reads them.
/// One row per AC, tab-separated, with tabs, newlines and backslashes escaped.
pub fn results_to_tsv(results: &[(Check, Outcome)]) -> String {
    let esc = |s: &str| s.replace('\\', "\\\\").replace('\t', "\\t").replace('\n', "\\n");
    let mut out = String::from("ac\tstatement\tkind\ttarget\texpect\toutcome\tevidence\n");
    for (c, o) in results {
        let f = [&c.ac, &c.statement, &c.kind, &c.target, &c.expect, o.word(), o.evidence()];
        out.push_str(&f.iter().map(|x| esc(x)).collect::<Vec<_>>().join("\t"));
        out.push('\n');
    }
    out
}

pub fn results_from_tsv(tsv: &str) -> Result<Vec<(Check, Outcome)>, String> {
    let unesc = |s: &str| {
        let (mut out, mut it) = (String::new(), s.chars());
        while let Some(ch) = it.next() {
            if ch != '\\' { out.push(ch); continue; }
            match it.next() { Some('t') => out.push('\t'), Some('n') => out.push('\n'), Some(x) => out.push(x), None => {} }
        }
        out
    };
    let mut lines = tsv.lines();
    if lines.next() != Some("ac\tstatement\tkind\ttarget\texpect\toutcome\tevidence") {
        return Err("not a borg-prove results file".into());
    }
    lines.filter(|l| !l.is_empty()).map(|l| {
        let f: Vec<String> = l.split('\t').map(unesc).collect();
        if f.len() != 7 { return Err(format!("a results row has {} fields, not 7", f.len())); }
        let c = Check { ac: f[0].clone(), statement: f[1].clone(), kind: f[2].clone(), target: f[3].clone(), expect: f[4].clone() };
        let o = match f[5].as_str() {
            "met" => Outcome::Met(f[6].clone()),
            "not met" => Outcome::NotMet(f[6].clone()),
            "unmeasured" => Outcome::Unmeasured(f[6].clone()),
            w => return Err(format!("unknown outcome '{w}'")),
        };
        Ok((c, o))
    }).collect()
}

/// #4691 — accept runs only on Borg Queen's own proof: her step signed by
/// borg-queen, and the run's verdict file written by her Mac account (the
/// signature dagu cannot check, the file owner can), saying Proven or, for a
/// card with no CardAC rows, unproven. Anything else holds the land, named.
pub fn signed_verdict(by: &str, owner: Option<&str>, verdict: Option<&str>, queen_account: &str) -> Result<(), String> {
    if by != "borg-queen" {
        return Err(format!("signed by {by}, not borg-queen: accept held"));
    }
    let (Some(owner), Some(v)) = (owner, verdict) else {
        return Err("no verdict recorded for this run: accept held".into());
    };
    if owner != queen_account {
        return Err(format!("the verdict was written by {owner}, not {queen_account}: accept held"));
    }
    match v.trim() {
        "Proven" | "unproven" => Ok(()),
        other => Err(format!("verdict {other}: accept held")),
    }
}
