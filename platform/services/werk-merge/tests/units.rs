//! Pure-helper unit tests (RED first) — the verb-contract helpers werk-merge shares
//! with the blueprint, plus the KEYSTONE of #3175: `pr_number_for_sha` resolves the
//! PR by HEAD sha, NOT by branch name. That is the exact bug Wren + Kade hit on
//! 2026-06-03: the interim `gh pr merge <branch>` matched the branch NAME and grabbed
//! a STALE already-merged PR (false-green, zero new commits) — or, with no PR, failed
//! loud. Resolving by the current HEAD oid is what makes the merge land the real work.

use werk_merge::{
    branch_name, classify_merge_error, failure_class, jsonl_line, parse_merge_args, pr_number_for_sha,
    require_approval, round_check, RoundCheck,
};

#[test]
fn branch_name_is_role_slash_card() {
    assert_eq!(branch_name("kade", 3175), "kade/3175");
}

// ── #3459 — round_check: accept a content-preserving rebase, refuse new content ──

#[test]
fn round_exact_sha_proceeds() {
    // the common case: the demo.presented witness carried this exact head-sha.
    assert_eq!(round_check(true, Some("p1"), &[]), RoundCheck::ExactSha);
}

#[test]
fn round_rebase_same_change_proceeds() {
    // sha moved (peer landed → rebase), but the diff's patch-id matches a demoed one:
    // the SAME change was demoed. This is the false-refusal the bug caused.
    let demoed = vec!["pAAA".to_string(), "pBBB".to_string()];
    assert_eq!(round_check(false, Some("pBBB"), &demoed), RoundCheck::RebasedSameChange);
}

#[test]
fn round_new_content_refuses() {
    // sha moved AND the patch-id differs from every demoed one — genuinely un-demoed
    // content. The #3365 safety must hold: refuse, re-demo.
    let demoed = vec!["pAAA".to_string()];
    assert_eq!(round_check(false, Some("pZZZ"), &demoed), RoundCheck::NoMatch);
}

#[test]
fn round_no_demoed_patch_refuses() {
    // no demo.presented patch-id on record for this card — cannot prove it was demoed.
    assert_eq!(round_check(false, Some("p1"), &[]), RoundCheck::NoMatch);
}

#[test]
fn round_uncomputable_current_patch_refuses() {
    // current patch-id couldn't be computed (empty diff / git failure) — never silently
    // allow; refuse (honest-red beats lying-green).
    assert_eq!(round_check(false, None, &["p1".to_string()]), RoundCheck::NoMatch);
    assert_eq!(round_check(false, Some(""), &["".to_string()]), RoundCheck::NoMatch);
}

#[test]
fn demoed_patch_ids_extracts_only_this_cards_presented_lines() {
    use werk_merge::demoed_patch_ids;
    let witness = concat!(
        r#"{"event":"demo.presented","card_id":3459,"round":"aaa","patch_id":"P1"}"#, "\n",
        r#"{"event":"demo.presented","card_id":9999,"round":"bbb","patch_id":"OTHER"}"#, "\n",
        r#"{"event":"demo.gate","card_id":3459,"patch_id":"NOTPRESENTED"}"#, "\n",
        r#"{"event":"demo.presented","card_id":3459,"round":"ccc","patch_id":"P2"}"#, "\n",
        r#"{"event":"demo.presented","card_id":3459,"round":"ddd"}"#, "\n", // pre-#3459, no patch_id
    );
    let got = demoed_patch_ids(witness, "\"card_id\":3459,");
    assert_eq!(got, vec!["P1".to_string(), "P2".to_string()]);
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

// ── #3495 — failure_class: the DORA change-failure-rate discriminator ──
// CHANGE = the change itself is bad (failed its tests/gates); TOOLING = the
// pipeline mechanics failed, the change is fine. Emitted AT THE SOURCE so CFR is
// honest by construction (ADR-046: emit the discriminating field, don't post-hoc
// classify). Only the existing flow-report walk-away bar (#3266) counts these;
// once it filters failureClass="tooling", today's pr-create-fail/announce-missing
// retries stop poisoning CFR.

#[test]
fn failure_class_test_and_gate_failures_are_change() {
    assert_eq!(failure_class("test-fail"), "change");
    assert_eq!(failure_class("gate-fail"), "change");
}

#[test]
fn failure_class_werk_merge_mechanical_reasons_are_tooling() {
    for r in [
        "no-werk",
        "branch-mismatch",
        "pr-create-fail",
        "no-open-pr",
        "announce-missing-this-round",
        "merge-fail",
    ] {
        assert_eq!(failure_class(r), "tooling", "{} must be tooling (not a change failure)", r);
    }
}

#[test]
fn failure_class_merge_conflict_defaults_tooling_v1() {
    // v1: merge-conflict / not-mergeable default to tooling (the common
    // rebase-artifact case). Promoting a genuine CONTENT conflict to "change"
    // needs the git conflict signal at the emit point — a flagged #3495 follow-on,
    // not faked here.
    assert_eq!(failure_class("merge-conflict"), "tooling");
    assert_eq!(failure_class("not-mergeable"), "tooling");
}

#[test]
fn failure_class_unknown_reason_defaults_tooling() {
    // Conservative: an unrecognized reason does NOT inflate CFR. A new
    // change-class reason must be added to the CHANGE arm explicitly.
    assert_eq!(failure_class("some-future-reason"), "tooling");
}

// ── #3297 — merge --atomic parse (same CLI-seam discipline as push #3296): recognize
// --atomic anywhere, never mis-read as the role. ──
#[test]
fn parse_merge_args_recognizes_atomic_anywhere() {
    let (c, r, a) = parse_merge_args(&["3297".into(), "kade".into(), "--atomic".into()], None).unwrap();
    assert_eq!((c, r.as_str(), a), (3297, "kade", true));
    let (c, r, a) = parse_merge_args(&["3297".into(), "--atomic".into(), "kade".into()], None).unwrap();
    assert_eq!((c, r.as_str(), a), (3297, "kade", true), "--atomic not mistaken for role");
    let (c, r, a) = parse_merge_args(&["3297".into()], Some("kade".into())).unwrap();
    assert_eq!((c, r.as_str(), a), (3297, "kade", false));
    assert!(parse_merge_args(&["nope".into(), "kade".into()], None).is_err());
}

// ── #3297 — the APPROVAL GATE (ADR-037 §22/§24: merge mutates main; one gate, two
// doors). In the FLOW the demo→GO is the approval (the land supplies the accepter), so
// the verb records, not gates. For merge --atomic (standalone, no land-GO) the verb
// itself DEMANDS authorization — refuse no-approval unless an accepter is supplied. That
// is the door --atomic must never become a quiet unauthorized ship through. ──
#[test]
fn require_approval_gates_atomic_without_an_accepter() {
    // --atomic, no accepter → REFUSE (the standalone door needs explicit authorization)
    let err = require_approval(true, None).expect_err("merge --atomic without accepter must refuse");
    let lc = err.to_lowercase();
    assert!(lc.contains("no-approval") || lc.contains("accepter"), "typed no-approval refusal, got: {}", err);
    // --atomic WITH an accepter → ok, returns who authorized (for the {who,what,when} event)
    assert_eq!(require_approval(true, Some("jeff".into())).unwrap(), "jeff");
    // FLOW (not atomic) WITH the land's accepter → ok, records who
    assert_eq!(require_approval(false, Some("jeff".into())).unwrap(), "jeff");
    // FLOW with no explicit accepter → ok (the demo→GO was the approval, not the verb's gate)
    assert!(require_approval(false, None).is_ok());
}

// #3336 — content-verify idempotency: `git diff --quiet` exit 0 (Ok→true) means trees
// identical = HEAD's content already on origin/main. Pin the exit-code→meaning mapping.
#[test]
fn head_content_merged_maps_diff_quiet_to_merged() {
    use werk_merge::head_content_merged;
    assert!(head_content_merged(true), "diff --quiet exit 0 (no diff) => content already on main");
    assert!(!head_content_merged(false), "diff present => not yet merged, proceed to merge");
}
