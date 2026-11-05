// @test-type: unit — pure: the built-in reviews' verdict from their own reply; no model.
// @domain: proving
// @card: #4889
//! Jeff 19:04 ("fine do that"): the code review is Claude Code's /code-review, with
//! /security-review beside it. Kade 19:19: the verdict comes from the findings.
use werk_demo::{builtin_review_verdict, BUILTIN_CODE_REVIEWS};

#[test]
fn the_code_step_runs_the_existing_reviews_by_name() {
    assert_eq!(BUILTIN_CODE_REVIEWS, ["code-review", "security-review"]);
}

#[test]
fn findings_in_the_reply_fail_with_file_line_and_summary() {
    let out = "The review is done. I found 1 issue.\n\n```json\n[\n  {\n    \"file\": \"a/b.rs\",\n    \"line\": 42,\n    \"summary\": \"race between read and truncate drops bytes\",\n    \"failure_scenario\": \"x\"\n  }\n]\n```\n";
    let (r, f) = builtin_review_verdict(out);
    assert_eq!(r, "fail");
    assert_eq!(f, "a/b.rs:42 — race between read and truncate drops bytes");
}

#[test]
fn an_empty_findings_list_passes() {
    assert_eq!(builtin_review_verdict("No issues found.\n```json\n[]\n```\n"), ("pass".to_string(), "no findings".to_string()));
}

#[test]
fn negative_proof_a_reply_with_no_findings_list_is_error_never_pass() {
    assert_eq!(builtin_review_verdict("Looks good to me, ship it.").0, "error");
    assert_eq!(builtin_review_verdict("```json\n[ {\"file\": \"a\"").0, "error", "a list cut off mid-way");
}

#[test]
fn the_last_json_block_is_the_findings_not_an_earlier_example() {
    let out = "e.g. ```json\n[]\n```\nthen\n```json\n[{\"file\":\"x.rs\",\"line\":1,\"summary\":\"bug\"}]\n```";
    assert_eq!(builtin_review_verdict(out).0, "fail");
}

// #4889 — Semgrep in the quality review (Jeff 19:04)
use werk_demo::semgrep_verdict;

#[test]
fn a_semgrep_error_fails_and_a_warning_is_a_note() {
    let j = r#"{"results":[{"check_id":"a.empty-catch","path":"x.ts","start":{"line":2,"col":1},"extra":{"message":"empty catch","severity":"ERROR"}},{"check_id":"a.or-true","path":"y.sh","start":{"line":9},"extra":{"message":"|| true","severity":"WARNING"}}],"errors":[]}"#;
    let (r, f) = semgrep_verdict(j);
    assert_eq!(r, "fail");
    assert!(f.starts_with("x.ts:2 — empty catch") && f.contains("note: y.sh:9"), "{f}");
    assert_eq!(semgrep_verdict(r#"{"results":[{"path":"y.sh","start":{"line":1},"extra":{"message":"m","severity":"WARNING"}}]}"#).0, "pass");
    assert_eq!(semgrep_verdict(r#"{"results":[],"errors":[]}"#), ("pass".to_string(), "no findings".to_string()));
}

#[test]
fn negative_proof_no_semgrep_report_is_error_never_pass() {
    assert_eq!(semgrep_verdict("Fatal error: exception Failure(\"no trust anchor\")").0, "error");
}

// #4889 — dependency-cruiser in the architecture review (Jeff 19:04)
use werk_demo::depcruise_verdict;

#[test]
fn a_boundary_break_fails_naming_the_rule_and_the_edge() {
    let out = "\n  error no-reach-into-another-package: directing/clearing/src/a.ts → platform/api/src/b.ts\n\nx 1 dependency violations (1 errors, 0 warnings).\n";
    let (r, f) = depcruise_verdict(out, Some(1));
    assert_eq!(r, "fail");
    assert_eq!(f, "no-reach-into-another-package: directing/clearing/src/a.ts → platform/api/src/b.ts");
    assert_eq!(depcruise_verdict("✔ no dependency violations found (909 modules)", Some(0)).0, "pass");
}

#[test]
fn negative_proof_no_report_is_error_never_pass() {
    assert_eq!(depcruise_verdict("npm ERR! could not determine executable to run", Some(1)).0, "error");
    assert_eq!(depcruise_verdict("", Some(0)).0, "error", "exit 0 with no summary is not a pass");
}
