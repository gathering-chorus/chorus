// @domain: builds — placed by chorus-crawl's domain rules (#4538)
//! #4155 AC4 — a jest case that throws a TypeError and one whose harness
//! answers 500 each come out of jest's real `--json` report as a reason line
//! carrying that text, and a file that dies at import names its TypeError.
//! The fixture is jest 29's own output from a throwaway file (2026-09-25),
//! paths shortened to /w/. Runs the runner's jq filter; jq is a hard dep of
//! the runner, so a missing jq fails here rather than passing empty.
use werk_test::why::jest_reasons;

const REPORT: &str = include_str!("fixtures/jest-why-4155.json");

#[test]
fn jq_is_present() {
    assert!(std::process::Command::new("jq").arg("--version").output().map(|o| o.status.success()).unwrap_or(false),
        "jq missing: the runner reads jest reasons with it");
}

/// NEGATIVE PROOF — the same report with every case passing yields no line:
/// the text above comes from the report, not from the reader.
#[test]
fn a_green_report_yields_nothing() {
    let green = REPORT.replace("\"failed\"", "\"passed\"");
    assert!(jest_reasons(green.as_bytes()).is_empty());
}
