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
                    format!("test {}: no recorded result in the store", c.target)
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
