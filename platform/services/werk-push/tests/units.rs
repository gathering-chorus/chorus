//! Pure-helper unit tests (RED first) — the verb-contract helpers werk-push shares
//! with the blueprint: branch/card handling + the jsonl witness line shape.

use werk_push::{branch_name, jsonl_line};

#[test]
fn branch_name_is_role_slash_card() {
    assert_eq!(branch_name("kade", 3056), "kade/3056");
}

#[test]
fn jsonl_line_is_valid_witness_record() {
    let line = jsonl_line(1234, "push.started", "kade", 3056, "abc-1", "");
    assert!(line.ends_with('\n'));
    assert!(line.contains("\"event\":\"push.started\""));
    assert!(line.contains("\"card_id\":3056"));
    assert!(line.contains("\"trace_id\":\"abc-1\""));
    assert!(line.starts_with('{'));
}

#[test]
fn jsonl_line_appends_extra_fields_verbatim() {
    let line = jsonl_line(1, "push.completed", "kade", 1, "t", ",\"sha\":\"deadbeef\"");
    assert!(line.contains("\"sha\":\"deadbeef\""));
}
