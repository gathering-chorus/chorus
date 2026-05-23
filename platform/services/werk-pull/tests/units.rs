// Test-first (DEC-1674): these reference werk_pull lib functions that don't exist
// yet — RED until src/lib.rs is written.
use werk_pull::{branch_name, jsonl_line, trace_id};

#[test]
fn branch_is_role_slash_card() {
    assert_eq!(branch_name("kade", 3045), "kade/3045");
}

#[test]
fn trace_id_nonempty_and_shaped() {
    let t = trace_id();
    assert!(!t.is_empty(), "trace id must not be empty");
    assert!(t.contains('-'), "trace id must contain the time-pid separator");
}

#[test]
fn jsonl_line_is_valid_shape_with_card_and_trace() {
    let line = jsonl_line(1234, "pull.completed", "kade", 3045, "abc-1", ",\"branch\":\"kade/3045\"");
    assert!(line.starts_with('{') && line.trim_end().ends_with('}'));
    assert!(line.contains("\"card_id\":3045"));
    assert!(line.contains("\"trace_id\":\"abc-1\""));
    assert!(line.contains("\"event\":\"pull.completed\""));
    assert!(line.contains("\"branch\":\"kade/3045\""));
}
