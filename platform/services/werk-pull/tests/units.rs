// Test-first (DEC-1674): these reference werk_pull lib functions that don't exist
// yet — RED until src/lib.rs is written.
use werk_pull::{branch_name, jsonl_line, trace_id, resolve_trace_in, spine_args};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

// Hermetic temp dir per test — the trace dir is injected (testable-core blueprint),
// so resolve_trace never touches the real /tmp during unit tests.
fn tmpdir(tag: &str) -> PathBuf {
    let ns = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let p = std::env::temp_dir().join(format!("wp-trace-{}-{}-{}", tag, std::process::id(), ns));
    fs::create_dir_all(&p).unwrap();
    p
}

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

// --- resolve_trace (CONSISTENT, #3135): env -> /tmp/<card>-trace -> mint+persist.
// The persisted file is the cross-process carrier that threads ONE trace across
// pull -> demo -> acp. Replaces werk-pull's #3063 fresh-mint drift.

#[test]
fn resolve_trace_env_wins_over_file() {
    let d = tmpdir("env");
    fs::write(d.join("7-trace"), "file-trace").unwrap(); // present, but env must win
    assert_eq!(resolve_trace_in(7, Some("env-trace"), &d), "env-trace");
}

#[test]
fn resolve_trace_file_fallback_when_no_env() {
    let d = tmpdir("file");
    fs::write(d.join("8-trace"), "file-trace\n").unwrap();
    assert_eq!(resolve_trace_in(8, None, &d), "file-trace");
}

#[test]
fn resolve_trace_mints_and_persists_when_neither() {
    let d = tmpdir("mint");
    let t = resolve_trace_in(9, None, &d);
    assert!(!t.is_empty(), "minted trace must be non-empty");
    assert!(t.contains('-'), "minted trace keeps the time-pid shape");
    let persisted = fs::read_to_string(d.join("9-trace")).expect("trace file must be written");
    assert_eq!(persisted.trim(), t, "persisted file must equal the returned trace (downstream inherits it)");
}

#[test]
fn resolve_trace_blank_env_falls_through_to_mint() {
    let d = tmpdir("blank");
    let t = resolve_trace_in(10, Some("   "), &d);
    assert!(!t.is_empty(), "blank env must not be used as the trace");
    assert!(fs::read_to_string(d.join("10-trace")).is_ok(), "must persist the minted trace");
}

// --- spine (AUDITABLE, #3135): the lifecycle event reaches the ONE spine
// (~/.chorus/chorus.log via chorus-log) so a pull is queryable in Loki, carrying
// the shared trace + card. spine_args is the pure contract (testable); emit_spine
// shells chorus-log best-effort. Replaces werk-pull's jsonl-witness-only (Loki-invisible).

#[test]
fn spine_args_carry_event_role_card_trace() {
    let a = spine_args("card.pulled", "kade", 3135, "abc-1", &[]);
    assert_eq!(a[0], "card.pulled", "event is first");
    assert_eq!(a[1], "kade", "role is second");
    assert!(a.contains(&"card=3135".to_string()), "card stamped for chorus_logs_for_card join");
    assert!(a.contains(&"trace=abc-1".to_string()), "trace stamped so pull correlates with demo+acp");
}

// #3161: failure emits carry disposition + reason so the #3165 rollup counts them
// and the trace reader shows WHY a pull was refused/rolled-back, not just that it was.
#[test]
fn spine_args_carry_disposition_and_reason_extras() {
    let a = spine_args(
        "pull.refused",
        "kade",
        3161,
        "abc-1",
        &[("disposition", "refuse"), ("reason", "wrong-status")],
    );
    assert_eq!(a[0], "pull.refused", "event is first");
    assert!(a.contains(&"card=3161".to_string()), "card still stamped");
    assert!(a.contains(&"trace=abc-1".to_string()), "trace still stamped");
    assert!(a.contains(&"disposition=refuse".to_string()), "rollup keys on disposition (#3165)");
    assert!(a.contains(&"reason=wrong-status".to_string()), "reason explains the refusal");
}
