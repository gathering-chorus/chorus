//! Pure-helper unit tests (RED first). The NOVEL logic of accept is the authority
//! gate (DEC-048: only Wren/Jeff finalize; a builder never self-accepts). That's a
//! pure function — test it exhaustively here. Plus the shared verb-contract helpers.

use werk_accept::{
    accept_output, branch_name, can_accept, demo_decision_line, demo_verdict_pass, jsonl_line,
    parse_accept_args, script_path,
};

// #3327 — GO is one silent ceremony. signal() records the go internally (witness +
// signal.go event) but returns NO human-facing announce on the clean WIP path; the
// only output a clean go:true run shows is finalize's "#N finalized". accept_output
// is the pure join: empty signal message ⇒ finalize alone (no "go signaled… act
// continues to merge" double-go), non-empty signal (the already-Done audit) ⇒ both.
#[test]
fn accept_output_clean_go_is_finalize_only_no_double_go() {
    let out = accept_output("", "#3327 finalized (board Done + closed)");
    assert_eq!(out, "#3327 finalized (board Done + closed)");
    assert!(!out.contains("go signaled"), "clean go must not re-announce a go");
    assert!(!out.contains("continues to merge"), "no future-tense merge language post-finalize");
}

#[test]
fn accept_output_audit_path_keeps_both_messages() {
    // already-Done recovery (#3298 audit): signal returns the audit note → show both.
    let out = accept_output("#3327 already finalized — recorded jeff's accept (audit)", "noop");
    assert_eq!(out, "#3327 already finalized — recorded jeff's accept (audit) | noop");
}

// #3237 — werk-accept's go-signal (and werk-do-more) write a demo.decision line
// into ops/logs/werk-demo.jsonl that werk-demo's read_decision must match. The
// match is byte-exact on a COMMA-TERMINATED card_id ("card_id":N,) — if that comma
// drifts, werk-demo blocks forever and the seam silently breaks. Pin it here.
#[test]
fn demo_decision_line_is_byte_exact_for_read_decision() {
    let line = demo_decision_line(1, 3237, "go", "jeff", "t");
    // THE load-bearing assertion (navigator #1): card_id comma-terminated.
    assert!(line.contains("\"card_id\":3237,"), "card_id must be comma-terminated: {}", line);
    assert!(line.contains("\"event\":\"demo.decision\""), "event must be demo.decision: {}", line);
    assert!(line.contains("\"decision\":\"go\""), "decision must be go: {}", line);
}

// #3298 — accept --atomic parse (the standalone accept door; same CLI-seam discipline
// as push #3296 / merge #3297): recognize --atomic anywhere, never mis-read as the role.
#[test]
fn parse_accept_args_recognizes_atomic_anywhere() {
    let (c, r, a) = parse_accept_args(&["3298".into(), "kade".into(), "--atomic".into()]).unwrap();
    assert_eq!((c, r.as_str(), a), (3298, "kade", true));
    let (c, r, a) = parse_accept_args(&["3298".into(), "--atomic".into(), "kade".into()]).unwrap();
    assert_eq!((c, r.as_str(), a), (3298, "kade", true), "--atomic not mistaken for role");
    let (c, r, a) = parse_accept_args(&["3298".into(), "kade".into()]).unwrap();
    assert_eq!((c, r.as_str(), a), (3298, "kade", false));
    assert!(parse_accept_args(&["notanum".into(), "kade".into()]).is_err());
    assert!(parse_accept_args(&["3298".into()]).is_err(), "role required");
}

// #3324 AUDIT — demo_decision_line_carries_more_and_no_go deleted: no-go/more
// emission belonged to werk-do-more, removed by #3311; nothing writes those
// values (write_decision is only ever called with "go").

#[test]
fn script_path_resolves_absolute_under_home_platform_scripts() {
    // #3183: werk-accept is exec'd by the chorus-mcp daemon, whose PATH lacks
    // platform/scripts — bare-name `cards` died "No such file or directory" (proven
    // LIVE accepting #3211). Resolve absolutely from home, like werk-pull's #3151 fix.
    use std::path::Path;
    assert_eq!(script_path(Path::new("/repo"), "cards"), "/repo/platform/scripts/cards");
    assert_eq!(script_path(Path::new("/x/y"), "chorus-werk"), "/x/y/platform/scripts/chorus-werk");
    assert_eq!(script_path(Path::new("/x/y"), "chorus-log"), "/x/y/platform/scripts/chorus-log");
}

#[test]
fn jeff_can_accept_any_card_including_jeff_owned() {
    assert!(can_accept("jeff", "kade"));
    assert!(can_accept("jeff", "silas"));
    assert!(can_accept("jeff", "wren"));
    // human final-authority is exempt from the self-accept rule (#3057 gate-arch):
    // there is no higher authority to protect against, so jeff accepts jeff-owned.
    assert!(can_accept("jeff", "jeff"));
}

#[test]
fn wren_can_accept_other_builders_cards() {
    assert!(can_accept("wren", "kade"));
    assert!(can_accept("wren", "silas"));
}

#[test]
fn builder_cannot_self_accept() {
    // DEC-048: the builder of a card can never finalize their own card.
    assert!(!can_accept("kade", "kade"));
    assert!(!can_accept("silas", "silas"));
    assert!(!can_accept("wren", "wren")); // even Wren can't self-accept her own card
}

#[test]
fn non_authority_roles_cannot_accept() {
    // only jeff/wren are accept authorities; kade/silas never finalize.
    assert!(!can_accept("kade", "silas"));
    assert!(!can_accept("silas", "kade"));
}

// #3324 AUDIT — branch_name test deleted: the helper has zero callers in lib.rs
// (accept is finalize-only since #3175; branch close is chorus-werk remove's).
// Removing the dead helper itself is a fill card (blast-radius pass, #3148).

#[test]
fn demo_verdict_pass_reads_the_demo_witness() {
    // #3116: accept gates on a demo.verdict=pass in the werk-demo witness.
    use std::path::PathBuf;
    let home: PathBuf = std::env::temp_dir().join(format!(
        "wa-verdict-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    let logs = home.join("ops/logs");
    std::fs::create_dir_all(&logs).unwrap();
    let witness = logs.join("werk-demo.jsonl");

    // no witness → no demo ran → false
    assert!(!demo_verdict_pass(&home, 3116));

    // a PASS for #31160 must NOT satisfy #3116 (comma-terminated key guards the prefix)
    std::fs::write(&witness, "{\"event\":\"demo.verdict\",\"card_id\":31160,\"verdict\":\"pass\"}\n").unwrap();
    assert!(!demo_verdict_pass(&home, 3116));

    // a FAIL for #3116 → false
    std::fs::write(&witness, "{\"event\":\"demo.verdict\",\"card_id\":3116,\"verdict\":\"fail\"}\n").unwrap();
    assert!(!demo_verdict_pass(&home, 3116));

    // a PASS for #3116 → true
    std::fs::write(&witness, "{\"event\":\"demo.verdict\",\"card_id\":3116,\"verdict\":\"pass\",\"prover\":\"jeff\"}\n").unwrap();
    assert!(demo_verdict_pass(&home, 3116));

    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn jsonl_line_is_valid_witness_record() {
    let line = jsonl_line(1, "accept.completed", "kade", 3057, "t", ",\"sha\":\"abc\"");
    assert!(line.ends_with('\n'));
    assert!(line.contains("\"event\":\"accept.completed\""));
    assert!(line.contains("\"card_id\":3057"));
}
