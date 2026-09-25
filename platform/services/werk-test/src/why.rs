//! #4155 — why a case failed, kept at the moment it fails.
//!
//! Jeff's spec line (2026-09-12): "all failures and exceptions caught and
//! logged as tests run". On 2026-09-12 two 500s, five TypeErrors and two 503s
//! sat in raw jest text in the flat log; none became a line of its own, and
//! the readout said "exceptions 0". Every runner already prints the reason —
//! jest in `failureMessages`, nextest in the panic block, bats in the TAP
//! comment lines, a shell suite in its last lines. This module reads it out,
//! per case, and names its kind, so the log, Loki, the graph and the page all
//! carry the same sentence.

/// The kinds a failure reason is counted under, in the order they are tested.
pub const KINDS: [&str; 4] = ["http", "assertion", "exception", "error"];

/// Longest reason kept. Enough for an exception line plus its message; a
/// stack trace is not a reason.
const MAX_REASON: usize = 300;

/// What kind of failure a reason describes. `http` wins when a service
/// answered 4xx/5xx (an `expect(status)` that received 500 is the harness
/// answering 500, not a wrong assertion); then an assertion; then an
/// exception; anything else is `error`.
pub fn failure_kind(reason: &str) -> &'static str {
    if is_http(reason) {
        "http"
    } else if is_assertion(reason) {
        "assertion"
    } else if is_exception(reason) {
        "exception"
    } else {
        "error"
    }
}

fn status_after(text: &str, marker: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let m = marker.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(&m) {
        let rest = &text[from + i + marker.len()..];
        let digits: String = rest
            .trim_start_matches([' ', ':', '=', '"'])
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if digits.len() == 3 && (digits.starts_with('4') || digits.starts_with('5')) {
            return true;
        }
        from += i + m.len();
    }
    false
}

fn is_http(r: &str) -> bool {
    ["HTTP", "status", "statusCode", "Received"].iter().any(|m| status_after(r, m))
        || [
            " 500 Internal", " 502 Bad Gateway", " 503 Service Unavailable", " 504 Gateway",
            " 401 Unauthorized", " 403 Forbidden", " 404 Not Found", " 422 Unprocessable",
        ]
        .iter()
        .any(|p| r.contains(p))
}

fn is_assertion(r: &str) -> bool {
    ["AssertionError", "assertion `", "assertion failed", "expect(", "Expected:", "AssertionError:", "assert_eq!", "' failed"]
        .iter()
        .any(|p| r.contains(p))
}

fn is_exception(r: &str) -> bool {
    if r.contains("panicked at") || r.contains("Traceback") || r.contains("called `Result::unwrap()`")
        || r.contains("called `Option::unwrap()`")
    {
        return true;
    }
    // `TypeError:`, `ReferenceError:`, `KeyError:`, `IllegalStateException:` —
    // a named error type followed by its message. A bare `Error:` is too
    // general to call an exception on its own.
    r.split(|c: char| !(c.is_ascii_alphanumeric() || c == ':'))
        .any(|w| {
            let w = w.trim_end_matches(':');
            (w.ends_with("Error") || w.ends_with("Exception"))
                && w.len() > "Error".len()
                && w.starts_with(|c: char| c.is_ascii_uppercase())
        })
}

/// A reason made safe for one pipe-delimited log line: ANSI colour codes
/// stripped, stack frames (`at …`) and blank lines dropped, the first few
/// lines joined with " · ", `|` replaced, and capped.
pub fn clean_reason(text: &str) -> String {
    let no_ansi = strip_ansi(text);
    let lines: Vec<&str> = no_ansi
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter(|l| !l.starts_with("at ") && !l.starts_with("note: run with `RUST_BACKTRACE"))
        // jest's code frame (`> 1 | src`, `2 | src`, `| ^`) and its bullet
        // header are location, not reason
        .filter(|l| !is_code_frame(l) && !l.starts_with("● Test suite failed to run"))
        .take(4)
        .collect();
    let mut s = lines.join(" · ").replace('|', "¦").replace('\t', " ");
    if s.chars().count() > MAX_REASON {
        s = s.chars().take(MAX_REASON).collect::<String>() + "…";
    }
    s
}

fn is_code_frame(l: &str) -> bool {
    let t = l.trim_start_matches('>').trim_start();
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    t.starts_with('|') || (digits > 0 && t[digits..].trim_start().starts_with('|'))
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for d in chars.by_ref() {
                    if d.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// nextest's panic blocks, keyed by the test path it names:
/// `thread 't::eq_fails' (9217274) panicked at src/lib.rs:5:9:` followed by
/// the message lines, up to the backtrace note or a blank line.
pub fn nextest_case_reasons(out: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = out.lines().collect();
    let mut found = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let t = l.trim();
        let Some(rest) = t.strip_prefix("thread '") else { continue };
        let Some((name, after)) = rest.split_once('\'') else { continue };
        let Some(at) = after.find("panicked at ") else { continue };
        let loc = after[at + "panicked at ".len()..].trim_end_matches(':');
        let mut msg: Vec<&str> = Vec::new();
        for m in lines.iter().skip(i + 1) {
            let m = m.trim();
            if m.is_empty() || m.starts_with("note:") || m.starts_with("thread '") {
                break;
            }
            msg.push(m);
        }
        let text = format!("{}\npanicked at {}", msg.join("\n"), loc);
        found.push((name.to_string(), clean_reason(&text)));
    }
    found
}

/// The reason for one nextest case path, matched on the panic block's name.
pub fn reason_for_nextest_path<'a>(reasons: &'a [(String, String)], path: &str) -> Option<&'a str> {
    reasons
        .iter()
        .find(|(n, _)| n == path || path.ends_with(&format!("::{}", n)) || n.ends_with(&format!("::{}", path)))
        .map(|(_, r)| r.as_str())
}

/// bats TAP: the `# ` comment lines after a `not ok N name`, without the
/// `(in test file …)` locator.
pub fn bats_case_reasons(tap: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut current: Option<(String, Vec<String>)> = None;
    let flush = |cur: &mut Option<(String, Vec<String>)>, found: &mut Vec<(String, String)>| {
        if let Some((name, body)) = cur.take() {
            found.push((name, clean_reason(&body.join("\n"))));
        }
    };
    for l in tap.lines() {
        if let Some(rest) = l.strip_prefix("not ok ") {
            flush(&mut current, &mut found);
            let name = rest.split_once(' ').map(|(_, n)| n).unwrap_or("").trim();
            let name = name.split(" # ").next().unwrap_or(name).to_string();
            current = Some((name, Vec::new()));
        } else if l.starts_with("ok ") || l.starts_with("1..") {
            flush(&mut current, &mut found);
        } else if let Some(c) = l.strip_prefix('#') {
            if let Some((_, body)) = current.as_mut() {
                let c = c.trim();
                if !c.starts_with("(in test file") && !c.starts_with("(from function") {
                    body.push(c.to_string());
                }
            }
        }
    }
    flush(&mut current, &mut found);
    found
}

/// The last non-empty lines of a unit's output: the reason a shell suite (no
/// per-case output) or any case the runner could not read a reason for gives.
pub fn tail_reason(out: &str, n: usize) -> String {
    let lines: Vec<&str> = out.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let start = lines.len().saturating_sub(n);
    clean_reason(&lines[start..].join("\n"))
}

/// jest's `--json` report run through `[$f, .fullName, failureMessages]|@tsv`:
/// tab-separated, newlines escaped as `\n`. A file that failed before any
/// case ran (a TypeError at import) comes through with an empty case name.
pub fn parse_jest_reason_tsv(tsv: &str) -> Vec<(String, String, String)> {
    tsv.lines()
        .filter_map(|l| {
            let mut f = l.splitn(3, '\t');
            let file = f.next()?.to_string();
            let name = crate::tsv_unescape(f.next()?);
            let msg = f.next().unwrap_or("").replace("\\n", "\n").replace("\\t", "\t").replace("\\\\", "\\");
            let reason = clean_reason(&msg);
            if reason.is_empty() { None } else { Some((file, name, reason)) }
        })
        .collect()
}

/// jest's `--json` report to `[file, case, failureMessages]` rows as TSV, and
/// a file that failed before any case ran (a TypeError at import) as one row
/// with an empty case name and the suite's message.
pub const JEST_REASON_JQ: &str = r#".testResults[] | .name as $f | ((.assertionResults[] | select(.status=="failed") | [$f, .fullName, ((.failureMessages // []) | join("\n"))]), (select((.assertionResults | length) == 0 and .status == "failed") | [$f, "", (.message // "")])) | @tsv"#;

/// Each failed jest case's reason from its `--json` report, read with jq (the
/// runner's zero-dep JSON reader). A jq that cannot run yields nothing, and
/// the caller's unit-tail reason stands in.
pub fn jest_reasons(json: &[u8]) -> Vec<(String, String, String)> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let Ok(mut jq) = Command::new("jq").args(["-r", JEST_REASON_JQ]).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn() else {
        return Vec::new();
    };
    if let Some(mut stdin) = jq.stdin.take() {
        if stdin.write_all(json).is_err() {
            return Vec::new();
        }
    }
    match jq.wait_with_output() {
        Ok(o) if o.status.success() => parse_jest_reason_tsv(&String::from_utf8_lossy(&o.stdout)),
        _ => Vec::new(),
    }
}

/// `nightly-why|file|case|kind|reason` — one line per failed case, printed as
/// it fails. The reason is cleaned, so it holds no `|`; the case may, so the
/// line is read from both ends.
pub fn why_line(file: &str, case: &str, reason: &str) -> String {
    let r = if reason.is_empty() { "no reason captured — the runner printed nothing it could read".to_string() } else { clean_reason(reason) };
    format!("nightly-why|{}|{}|{}|{}", file, case, failure_kind(&r), r)
}

/// Read a `nightly-why` line back: (file, case, kind, reason).
pub fn parse_why_line(line: &str) -> Option<(String, String, String, String)> {
    let rest = line.strip_prefix("nightly-why|")?;
    let (file, rest) = rest.split_once('|')?;
    let (rest, reason) = rest.rsplit_once('|')?;
    let (case, kind) = rest.rsplit_once('|')?;
    if !KINDS.contains(&kind) {
        return None;
    }
    Some((file.to_string(), case.to_string(), kind.to_string(), reason.to_string()))
}

/// The run's error counts, from its own `nightly-why` lines:
/// `failed cases 9 · exceptions 5 · http 3 · assertions 1 · other 0`.
/// A case counts once: nextest prints a failed case's FAIL line twice (live
/// and in its summary), so one failure reaches the log as two why lines.
pub fn error_counts_line(whys: &[(String, String, String, String)]) -> String {
    error_counts(whys).line()
}

/// #4156 — the counts as numbers, for the run record in the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ErrorCounts {
    pub failed_cases: usize,
    pub exceptions: usize,
    pub http: usize,
    pub assertions: usize,
    pub other: usize,
}

impl ErrorCounts {
    pub fn line(&self) -> String {
        format!(
            "failed cases {} · exceptions {} · http {} · assertions {} · other {}",
            self.failed_cases, self.exceptions, self.http, self.assertions, self.other
        )
    }
}

pub fn error_counts(whys: &[(String, String, String, String)]) -> ErrorCounts {
    let mut seen = std::collections::HashSet::new();
    let whys: Vec<&(String, String, String, String)> = whys.iter().filter(|w| seen.insert((w.0.as_str(), w.1.as_str()))).collect();
    let n = |k: &str| whys.iter().filter(|w| w.2 == k).count();
    ErrorCounts { failed_cases: whys.len(), exceptions: n("exception"), http: n("http"), assertions: n("assertion"), other: n("error") }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Real output captured 2026-09-25 from a throwaway crate / jest file / bats
    // file, not written by hand.
    const NEXTEST: &str = "        FAIL [   0.010s] (3/3) whyfix t::eq_fails\n  stderr ───\n\n    thread 't::eq_fails' (9217274) panicked at src/lib.rs:5:9:\n    assertion `left == right` failed: sum is wrong\n      left: 2\n     right: 3\n    note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\n\n  stderr ───\n\n    thread 't::unwrap_fails' (9217276) panicked at src/lib.rs:10:11:\n    called `Result::unwrap()` on an `Err` value: \"HTTP 500 from /tests\"\n    note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\n";
    const JEST_TSV: &str = "/w/why.test.js\tthrows a TypeError\tTypeError: Cannot read properties of undefined (reading 'foo')\\n    at Object.foo (/w/why.test.js:1:59)\\n    at Promise.finally.completed (/x/jestAdapterInit.js:1557:28)\n/w/why.test.js\tharness answers 500\tError: expect(received).toBe(expected) // Object.is equality\\n\\nExpected: 200\\nReceived: 500\\n    at Object.toBe (/w/why.test.js:2:85)\n/w/why.test.js\tpasses\t\n";
    const TAP: &str = "1..2\nok 1 passes\nnot ok 2 status is 200\n# (in test file why.bats, line 2)\n#   `@test \"status is 200\" { status=503; echo \"curl said 503 Service Unavailable\"; [ \"$status\" -eq 200 ]; }' failed\n# curl said 503 Service Unavailable\n";

    /// AC4 — a case that throws a TypeError and one whose harness answers
    /// 500 each produce a line carrying that text, under its own kind.
    #[test]
    fn a_type_error_and_a_500_each_become_their_own_line() {
        let jest = parse_jest_reason_tsv(JEST_TSV);
        assert_eq!(jest.len(), 2, "{jest:?}");
        let te = why_line("why.test.js", &jest[0].1, &jest[0].2);
        assert!(te.contains("TypeError: Cannot read properties of undefined"), "{te}");
        assert_eq!(parse_why_line(&te).unwrap().2, "exception");
        let h = why_line("why.test.js", &jest[1].1, &jest[1].2);
        assert!(h.contains("Received: 500"), "{h}");
        assert_eq!(parse_why_line(&h).unwrap().2, "http");
        // stack frames never ride along
        assert!(!te.contains("jestAdapterInit"), "{te}");
    }

    /// NEGATIVE PROOF — without the reason the line says so; it never reads
    /// as a TypeError or a 500 that was not there.
    #[test]
    fn no_reason_is_named_never_invented() {
        let l = why_line("a.rs", "t", "");
        assert!(l.contains("no reason captured"), "{l}");
        assert!(!l.contains("TypeError") && !l.contains("500"));
        assert_eq!(parse_why_line(&l).unwrap().2, "error");
    }

    #[test]
    fn nextest_panics_are_read_per_case() {
        let r = nextest_case_reasons(NEXTEST);
        assert_eq!(r.len(), 2, "{r:?}");
        let eq = reason_for_nextest_path(&r, "t::eq_fails").unwrap();
        assert!(eq.contains("sum is wrong") && eq.contains("left: 2"), "{eq}");
        assert_eq!(failure_kind(eq), "assertion");
        let un = reason_for_nextest_path(&r, "t::unwrap_fails").unwrap();
        assert_eq!(failure_kind(un), "http", "{un}");
        assert!(reason_for_nextest_path(&r, "t::ok").is_none());
    }

    #[test]
    fn bats_keeps_the_comment_lines_not_the_locator() {
        let r = bats_case_reasons(TAP);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].0, "status is 200");
        assert!(r[0].1.contains("503 Service Unavailable"), "{}", r[0].1);
        assert!(!r[0].1.contains("in test file"));
        assert_eq!(failure_kind(&r[0].1), "http");
    }

    #[test]
    fn a_line_round_trips_even_when_the_case_holds_a_pipe() {
        let l = why_line("a.bats", "reads a|b pair", "boom | here");
        let (f, c, _, r) = parse_why_line(&l).unwrap();
        assert_eq!((f.as_str(), c.as_str()), ("a.bats", "reads a|b pair"));
        assert!(!r.contains('|'));
    }

    #[test]
    fn kinds_are_told_apart() {
        assert_eq!(failure_kind("ReferenceError: x is not defined"), "exception");
        assert_eq!(failure_kind("Traceback (most recent call last): KeyError: 'a'"), "exception");
        assert_eq!(failure_kind("AssertionError: 1 != 2"), "assertion");
        assert_eq!(failure_kind("POST /tests/tests -> HTTP 403"), "http");
        assert_eq!(failure_kind("Error: something broke"), "error");
        // NEGATIVE PROOF: a status of 200 is not an http failure
        assert_eq!(failure_kind("HTTP 200 but the body was empty"), "error");
    }

    #[test]
    fn counts_come_from_the_lines() {
        let whys: Vec<_> = [
            why_line("a", "1", "TypeError: x"),
            why_line("a", "2", "Expected: 200 Received: 500"),
            why_line("a", "3", "assertion `left == right` failed"),
        ]
        .iter()
        .filter_map(|l| parse_why_line(l))
        .collect();
        assert_eq!(error_counts_line(&whys), "failed cases 3 · exceptions 1 · http 1 · assertions 1 · other 0");
    }

    /// Live 2026-09-25: one failing nextest case printed two why lines. It is
    /// one failure; a second, different case in the same file still counts.
    #[test]
    fn a_case_printed_twice_counts_once() {
        let one = why_line("a.rs", "t", "HTTP 500");
        let whys: Vec<_> = [one.clone(), one, why_line("a.rs", "u", "HTTP 500")]
            .iter().filter_map(|l| parse_why_line(l)).collect();
        assert_eq!(error_counts_line(&whys), "failed cases 2 · exceptions 0 · http 2 · assertions 0 · other 0");
    }

    #[test]
    fn shell_tail_is_the_last_lines() {
        let t = tail_reason("one\ntwo\n\nthree\nFAIL: four\n", 2);
        assert_eq!(t, "three · FAIL: four");
    }
}
