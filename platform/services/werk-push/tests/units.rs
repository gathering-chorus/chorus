//! Pure-helper unit tests (RED first) — the verb-contract helpers werk-push shares
//! with the blueprint: branch/card handling + the jsonl witness line shape, plus the
//! #3163 spine helpers (resolve_trace inheritance + the chorus-log args contract).

use werk_push::{branch_name, jsonl_line, parse_push_args, resolve_trace_in, spine_args};

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

// #3163 — the spine helpers (the #3045 verb-contract observability). Mirror werk-commit:
// werk-push must surface push.failed/refused on the ONE spine, keyed by card + the
// INHERITED trace (not a fresh mint), so a rejected push shows on the card's thread.

#[test]
fn spine_args_is_the_chorus_log_contract() {
    let args = spine_args("push.failed", "kade", 3163, "trace-xyz", &[("reason", "non-fast-forward")]);
    assert_eq!(args, vec!["push.failed", "kade", "card=3163", "trace=trace-xyz", "reason=non-fast-forward"]);
}

#[test]
fn resolve_trace_inherits_the_env_trace() {
    let dir = std::env::temp_dir();
    // a present env trace is returned verbatim — the inheritance contract (not a mint).
    assert_eq!(resolve_trace_in(3163, Some("inherited-abc"), &dir), "inherited-abc");
    // blank env is NOT a trace — falls through to file/mint.
    assert_ne!(resolve_trace_in(3163, Some("   "), &dir), "   ");
}

#[test]
fn resolve_trace_mints_then_persists_so_one_trace_threads() {
    use std::fs;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("wp-trace-{}-{}", std::process::id(), nanos));
    fs::create_dir_all(&dir).unwrap();
    let card = 4243;
    // no env + no file → mint AND persist the carrier (downstream verbs inherit it).
    let minted = resolve_trace_in(card, None, &dir);
    assert!(!minted.is_empty());
    assert!(dir.join(format!("{}-trace", card)).exists(), "the mint persists the carrier file");
    // second resolve (no env) reads the SAME persisted trace → ONE trace threads through.
    assert_eq!(resolve_trace_in(card, None, &dir), minted);
}

// #3296 — `push --atomic`: the flag must be RECOGNIZED at the CLI seam, not silently
// dropped (run_push ignored nth(3+)) or mis-parsed as the role (`werk-push 3296 --atomic`
// read --atomic as role). push is in the ADR-037 --atomic-FREE group (reversible, no
// approval), so --atomic does not change behavior — but it MUST parse cleanly: card +
// role correct, atomic=true, regardless of flag position.
#[test]
fn parse_push_args_recognizes_atomic_anywhere() {
    // flag trailing
    let (card, role, atomic) =
        parse_push_args(&["3296".into(), "kade".into(), "--atomic".into()], None).unwrap();
    assert_eq!((card, role.as_str(), atomic), (3296, "kade", true));

    // flag BETWEEN card and role — must NOT be mistaken for the role
    let (card, role, atomic) =
        parse_push_args(&["3296".into(), "--atomic".into(), "kade".into()], None).unwrap();
    assert_eq!((card, role.as_str(), atomic), (3296, "kade", true), "--atomic not mistaken for role");

    // no flag → atomic=false, role from the DEPLOY_ROLE fallback
    let (card, role, atomic) = parse_push_args(&["3296".into()], Some("kade".into())).unwrap();
    assert_eq!((card, role.as_str(), atomic), (3296, "kade", false));

    // non-numeric card → Err (the contract guard)
    assert!(parse_push_args(&["notanum".into(), "kade".into()], None).is_err());
}
