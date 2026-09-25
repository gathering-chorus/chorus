//! #4155 AC4 — a jest case that throws a TypeError and one whose harness
//! answers 500 each come out of jest's real `--json` report as a reason line
//! carrying that text, and a file that dies at import names its TypeError.
//! The fixture is jest 29's own output from a throwaway file (2026-09-25),
//! paths shortened to /w/. Runs the runner's jq filter; jq is a hard dep of
//! the runner, so a missing jq fails here rather than passing empty.
use werk_test::why::{jest_reasons, parse_why_line, why_line, error_counts_line};

const REPORT: &str = include_str!("fixtures/jest-why-4155.json");

#[test]
fn jq_is_present() {
    assert!(std::process::Command::new("jq").arg("--version").output().map(|o| o.status.success()).unwrap_or(false),
        "jq missing: the runner reads jest reasons with it");
}

#[test]
fn a_type_error_and_a_500_come_out_of_the_real_report() {
    let r = jest_reasons(REPORT.as_bytes());
    let whys: Vec<_> = r.iter().filter_map(|(f, n, t)| parse_why_line(&why_line(f, n, t))).collect();
    let find = |case: &str| whys.iter().find(|w| w.1 == case).unwrap_or_else(|| panic!("no line for {case}: {whys:?}"));
    let te = find("throws a TypeError");
    assert_eq!(te.2, "exception");
    assert!(te.3.contains("TypeError: Cannot read properties of undefined (reading 'foo')"), "{te:?}");
    let h = find("harness answers 500");
    assert_eq!(h.2, "http");
    assert!(h.3.contains("Received: 500"), "{h:?}");
    // the file that died at import: its own line, the TypeError, no code frame
    let imp = whys.iter().find(|w| w.0 == "/w/import.test.js").expect("import failure line");
    assert!(imp.3.starts_with("TypeError: Cannot read properties of undefined (reading 'boom')"), "{imp:?}");
    assert!(!imp.3.contains("x.boom();"), "code frame leaked: {imp:?}");
    assert_eq!(error_counts_line(&whys), "failed cases 3 · exceptions 2 · http 1 · assertions 0 · other 0");
}

/// NEGATIVE PROOF — the same report with every case passing yields no line:
/// the text above comes from the report, not from the reader.
#[test]
fn a_green_report_yields_nothing() {
    let green = REPORT.replace("\"failed\"", "\"passed\"");
    assert!(jest_reasons(green.as_bytes()).is_empty());
}
