//! Pure-helper unit tests (RED first). These cover the verb-contract helpers
//! (AC1): branch/card handling, the jsonl witness line shape, and commit-message
//! assembly. No git, no env — pure functions, deterministic.

use werk_commit::{
    branch_name, commit_message, conflict_hold_message, jsonl_line, parse_commit_args,
    resolve_trace_in, spine_args, Mode,
};

fn args(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

// #3304 — the CLI seam (the #3294 pattern: parse_<verb>_args recognizes flags
// anywhere, unit-tested where the contract lives). Four modes, mutually exclusive.
#[test]
fn parse_recognizes_the_four_modes_with_flags_anywhere() {
    let p = parse_commit_args(&args(&["3304", "kade", "a", "summary"])).unwrap();
    assert_eq!((p.card, p.role.as_deref(), p.mode), (3304, Some("kade"), Mode::Flow));
    assert_eq!(p.summary, "a summary");

    let p = parse_commit_args(&args(&["--atomic", "3304", "kade"])).unwrap();
    assert_eq!(p.mode, Mode::Atomic, "--atomic recognized anywhere");

    let p = parse_commit_args(&args(&["3304", "kade", "--continue"])).unwrap();
    assert_eq!(p.mode, Mode::Continue);

    let p = parse_commit_args(&args(&["3304", "--abort", "kade"])).unwrap();
    assert_eq!((p.mode, p.role.as_deref()), (Mode::Abort, Some("kade")));
}

#[test]
fn parse_refuses_conflicting_modes_and_bad_card() {
    assert!(parse_commit_args(&args(&["3304", "kade", "--continue", "--abort"])).is_err(),
        "--continue + --abort is a usage error");
    assert!(parse_commit_args(&args(&["3304", "kade", "--atomic", "--continue"])).is_err(),
        "--atomic + --continue is a usage error");
    assert!(parse_commit_args(&args(&["notanumber", "kade"])).is_err());
    assert!(parse_commit_args(&args(&[])).is_err());
}

// #3304 — the held-conflict instruction: names the conflicted files and BOTH
// in-verb follow-ups. The human edits files, never runs raw git — the guard
// stays whole because the resolution is reachable only through the verb.
#[test]
fn conflict_hold_message_names_files_and_both_verb_follow_ups() {
    let m = conflict_hold_message(3304, "kade", &["src/lib.rs".to_string(), "README".to_string()]);
    assert!(m.contains("src/lib.rs") && m.contains("README"), "conflicted files named: {m}");
    assert!(m.contains("werk-commit 3304 kade --continue"), "continue follow-up named: {m}");
    assert!(m.contains("werk-commit 3304 kade --abort"), "abort follow-up named: {m}");
    assert!(!m.contains("git rebase"), "never instructs raw git: {m}");
}

// #3304 AC4 — guard regression tripwire: the resolution lives in the verb, NEVER
// as a raw-git EXEMPTION in infra_guardrails. The mutating `git rebase <ref>`
// block must still exist, and the guard must carry no raw-git carve-out.
// #3484: the prior check `!contains("werk-commit")` was TOO BROAD — it tripped on
// the guard's legitimate BLOCK-messages that name werk-commit as the sanctioned
// path ("Commits land through … werk-commit") — correct UX, NOT an exemption — so
// the nightly went red on a false positive. A carve-out is an INTENTIONAL exemption
// that lets raw git through; by convention it carries a `RAW-GIT-CARVE-OUT:` marker
// at its site, and this test keys on that marker (intent), not bare string presence.
// (Ideal follow-on: a BEHAVIORAL test that the guard still BLOCKS raw git — keys on
// behavior, not source text. The marker is the cheap, intent-based v1.)
#[test]
fn infra_guardrails_has_no_new_raw_git_carve_out() {
    let guard_src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../chorus-hooks/src/hooks/infra_guardrails.rs");
    let src = std::fs::read_to_string(&guard_src)
        .unwrap_or_else(|e| panic!("guard source must exist at {}: {}", guard_src.display(), e));
    assert!(
        src.contains(r"\bgit\s+rebase\b"),
        "the mutating-rebase block regex must remain in infra_guardrails"
    );
    assert!(
        !src.contains("RAW-GIT-CARVE-OUT"),
        "infra_guardrails carries a RAW-GIT-CARVE-OUT marker — a raw-git exemption was added. \
         The resolution must live in the verb, not as an infra_guardrails exception (#3304); \
         an intentional exemption needs architecture sign-off, not just the marker."
    );
}

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
