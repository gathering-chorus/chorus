//! Pure-helper unit tests (RED first) — the verb-contract helpers werk-merge shares
//! with the blueprint, plus the KEYSTONE of #3175: `pr_number_for_sha` resolves the
//! PR by HEAD sha, NOT by branch name. That is the exact bug Wren + Kade hit on
//! 2026-06-03: the interim `gh pr merge <branch>` matched the branch NAME and grabbed
//! a STALE already-merged PR (false-green, zero new commits) — or, with no PR, failed
//! loud. Resolving by the current HEAD oid is what makes the merge land the real work.

use werk_merge::{branch_name, classify_merge_error, jsonl_line, pr_number_for_sha};

#[test]
fn branch_name_is_role_slash_card() {
    assert_eq!(branch_name("kade", 3175), "kade/3175");
}

#[test]
fn jsonl_line_is_valid_witness_record() {
    let line = jsonl_line(1234, "merge.started", "kade", 3175, "abc-1", "");
    assert!(line.ends_with('\n'));
    assert!(line.contains("\"event\":\"merge.started\""));
    assert!(line.contains("\"card_id\":3175"));
    assert!(line.contains("\"trace_id\":\"abc-1\""));
    assert!(line.starts_with('{'));
}

#[test]
fn jsonl_line_appends_extra_fields_verbatim() {
    let line = jsonl_line(1, "merge.completed", "kade", 1, "t", ",\"sha\":\"deadbeef\"");
    assert!(line.contains("\"sha\":\"deadbeef\""));
}

// ── KEYSTONE: resolve the OPEN PR for the current HEAD sha, never the branch name ──

#[test]
fn pr_number_for_sha_picks_the_pr_matching_the_head_oid() {
    // gh pr list --json number,headRefOid output (compact, fields in gh's order).
    let json = r#"[{"number":456,"headRefOid":"aaaa1111bbbb2222"}]"#;
    assert_eq!(pr_number_for_sha(json, "aaaa1111bbbb2222"), Some(456));
}

#[test]
fn pr_number_for_sha_ignores_a_pr_at_a_different_sha() {
    // This is Wren's false-green: a PR exists for the branch but at a STALE sha
    // (the already-merged first PR). Matching by name would grab it; matching by
    // the current HEAD oid must NOT — there is no open PR for THIS work yet.
    let json = r#"[{"number":440,"headRefOid":"staaale0000dead"}]"#;
    assert_eq!(pr_number_for_sha(json, "fresh9999headoid"), None);
}

#[test]
fn pr_number_for_sha_selects_the_right_one_among_several() {
    let json = r#"[{"number":10,"headRefOid":"oldsha"},{"number":22,"headRefOid":"target"},{"number":33,"headRefOid":"other"}]"#;
    assert_eq!(pr_number_for_sha(json, "target"), Some(22));
}

#[test]
fn pr_number_for_sha_handles_empty_list() {
    assert_eq!(pr_number_for_sha("[]", "anything"), None);
    assert_eq!(pr_number_for_sha("", "anything"), None);
}

#[test]
fn pr_number_for_sha_tolerates_field_order_and_whitespace() {
    let json = r#"[ { "headRefOid": "xyz", "number": 7 } ]"#;
    assert_eq!(pr_number_for_sha(json, "xyz"), Some(7));
}

// ── merge-error classification: typed refusal taxonomy from gh stderr ──

#[test]
fn classify_merge_error_detects_conflict() {
    assert_eq!(classify_merge_error("merge conflict between base and head"), "merge-conflict");
}

#[test]
fn classify_merge_error_detects_not_mergeable() {
    assert_eq!(classify_merge_error("Pull request is not mergeable"), "not-mergeable");
}

#[test]
fn classify_merge_error_falls_back_to_merge_fail() {
    assert_eq!(classify_merge_error("some unexpected gh error"), "merge-fail");
}
