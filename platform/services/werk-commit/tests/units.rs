//! Pure-helper unit tests (RED first). These cover the verb-contract helpers
//! (AC1): branch/card handling, the jsonl witness line shape, and commit-message
//! assembly. No git, no env — pure functions, deterministic.

use werk_commit::{branch_name, commit_message, jsonl_line};

#[test]
fn branch_name_is_role_slash_card() {
    assert_eq!(branch_name("kade", 3056), "kade/3056");
}

#[test]
fn commit_message_prefixes_role_and_card() {
    // Convention across the team: "<role>: #<card> — <summary>".
    assert_eq!(
        commit_message("kade", 3056, "commit+push v2"),
        "kade: #3056 — commit+push v2"
    );
}

#[test]
fn commit_message_trims_and_handles_empty_summary() {
    assert_eq!(commit_message("kade", 7, "  spaced  "), "kade: #7 — spaced");
    // empty summary still yields a valid, parseable header (no trailing em-dash junk).
    assert_eq!(commit_message("kade", 7, ""), "kade: #7");
}

#[test]
fn jsonl_line_is_valid_witness_record() {
    let line = jsonl_line(1234, "commit.started", "kade", 3056, "abc-1", "");
    assert!(line.ends_with('\n'));
    assert!(line.contains("\"event\":\"commit.started\""));
    assert!(line.contains("\"card_id\":3056"));
    assert!(line.contains("\"trace_id\":\"abc-1\""));
    assert!(line.starts_with('{'));
}

#[test]
fn jsonl_line_appends_extra_fields_verbatim() {
    let line = jsonl_line(1, "commit.completed", "kade", 1, "t", ",\"sha\":\"deadbeef\"");
    assert!(line.contains("\"sha\":\"deadbeef\""));
}
