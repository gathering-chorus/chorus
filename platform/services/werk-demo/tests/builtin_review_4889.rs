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
