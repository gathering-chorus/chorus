//! Pure-helper unit tests (RED first). These cover the verb-contract helpers
//! (AC1): branch/card handling, the jsonl witness line shape, and commit-message
//! assembly. No git, no env — pure functions, deterministic.

use werk_commit::{branch_name, commit_message, jsonl_line, resolve_trace_in, spine_args};

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

// #3162 — the spine helpers (the #3045 verb-contract observability).

#[test]
fn spine_args_is_the_chorus_log_contract() {
    let args = spine_args("commit.failed", "kade", 3162, "trace-xyz", &[("reason", "pre-commit-gate")]);
    assert_eq!(args, vec!["commit.failed", "kade", "card=3162", "trace=trace-xyz", "reason=pre-commit-gate"]);
}

#[test]
fn resolve_trace_inherits_the_env_trace() {
    let dir = std::env::temp_dir();
    // a present env trace is returned verbatim — the inheritance contract (not a mint).
    assert_eq!(resolve_trace_in(3162, Some("inherited-abc"), &dir), "inherited-abc");
    // blank env is NOT a trace — falls through to file/mint.
    assert_ne!(resolve_trace_in(3162, Some("   "), &dir), "   ");
}

#[test]
fn resolve_trace_mints_then_persists_so_one_trace_threads() {
    use std::fs;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("wc-trace-{}-{}", std::process::id(), nanos));
    fs::create_dir_all(&dir).unwrap();
    let card = 4242;
    // no env + no file → mint AND persist the carrier (downstream verbs inherit it).
    let minted = resolve_trace_in(card, None, &dir);
    assert!(!minted.is_empty());
    assert!(dir.join(format!("{}-trace", card)).exists(), "the mint persists the carrier file");
    // second resolve (no env) reads the SAME persisted trace → ONE trace threads through.
    assert_eq!(resolve_trace_in(card, None, &dir), minted);
}
