//! #3203 — context-inject FORCE (HIP-001, the forcing pattern applied to the inject).
//!
//! The inject was an IGNORE: it appends a context block but can't compel use, so
//! surfaced records get scrolled past (proven live 2026-06-03 — even the "we vs you"
//! and scarecrow records would have been ignored). This turns it into a FORCE via the
//! existing Stop-hook: the turn cannot complete while the inject surfaced material this
//! turn AND the response neither USED nor explicitly DISMISSED-with-reason any of it.
//!
//! The force-vs-theater fork (AC3): a "did you cite it" check produces citation theater.
//! The real demand is a DECISION — use-or-reject — so an explicit dismissal-with-reason
//! satisfies the gate, and that decision is logged so rote dismissals are auditable.
//!
//! Pure verdict fn pinned here, RED before it exists.

use chorus_hooks::{
    inject_engagement_verdict, last_assistant_text, read_surfaced_in, record_surfaced_in,
    EngagementVerdict,
};

// AC2 — nothing surfaced this turn → never block a bare turn.
#[test]
fn no_surfaced_records_passes() {
    let v = inject_engagement_verdict(&[], "any response at all");
    assert!(matches!(v, EngagementVerdict::Pass), "no surfaced records must pass; got {v:?}");
}

// AC1 — surfaced records ignored by the response → BLOCK, naming what's unaddressed.
#[test]
fn surfaced_but_ignored_blocks() {
    let surfaced = vec!["scarecrow wizard oz".to_string(), "competing paths".to_string()];
    let v = inject_engagement_verdict(&surfaced, "Here is an unrelated answer about deploy topology and ports.");
    match v {
        EngagementVerdict::Block { reason } => {
            assert!(reason.to_lowercase().contains("scarecrow") || reason.to_lowercase().contains("surfaced"),
                "block reason must name what's unaddressed; got: {reason}");
        }
        other => panic!("ignored surfaced records must block; got {other:?}"),
    }
}

// "used" — the response references a surfaced record → Pass.
#[test]
fn using_a_surfaced_record_passes() {
    let surfaced = vec!["scarecrow wizard oz".to_string()];
    let v = inject_engagement_verdict(&surfaced, "On the scarecrow record — yes, you raised the wizard of oz line on 03-31.");
    assert!(matches!(v, EngagementVerdict::Pass), "using a surfaced record must pass; got {v:?}");
}

// AC3 — explicit dismissal-with-reason → Pass (can't be forced into citation theater).
#[test]
fn explicit_dismissal_with_reason_passes() {
    let surfaced = vec!["scarecrow wizard oz".to_string()];
    let v = inject_engagement_verdict(&surfaced, "dismiss-inject: the scarecrow records don't bear on this deploy question.");
    assert!(matches!(v, EngagementVerdict::Pass), "explicit dismissal-with-reason must pass; got {v:?}");
}

// The escape hatch needs a REASON — a bare dismissal marker with nothing after it still blocks.
#[test]
fn bare_dismissal_without_reason_blocks() {
    let surfaced = vec!["scarecrow wizard oz".to_string()];
    let v = inject_engagement_verdict(&surfaced, "dismiss-inject:");
    assert!(matches!(v, EngagementVerdict::Block { .. }), "dismissal with no reason must still block; got {v:?}");
}

// AC1 input — the Stop gate reads MY last assistant message from the transcript JSONL.
#[test]
fn last_assistant_text_reads_the_last_assistant_turn() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("transcript.jsonl");
    std::fs::write(&path, concat!(
        "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"the prompt\"}}\n",
        "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"first reply\"}]}}\n",
        "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"LAST reply about the scarecrow\"}]}}\n",
    )).unwrap();
    let got = last_assistant_text(path.to_str().unwrap()).expect("must read an assistant turn");
    assert!(got.contains("LAST reply"), "must return the LAST assistant text; got: {got}");
    assert!(!got.contains("first reply"), "must not return an earlier turn; got: {got}");
}

#[test]
fn last_assistant_text_missing_file_is_none() {
    assert!(last_assistant_text("/no/such/transcript.jsonl").is_none());
}

// AC1 input — the inject records what it surfaced; the gate reads it back (roundtrip).
#[test]
fn surfaced_records_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let records = vec!["scarecrow 03-31".to_string(), "we vs you".to_string()];
    record_surfaced_in(dir.path(), "sess-abc", &records).unwrap();
    let got = read_surfaced_in(dir.path(), "sess-abc");
    assert_eq!(got, records, "surfaced records must round-trip per session");
}

#[test]
fn surfaced_read_missing_session_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    assert!(read_surfaced_in(dir.path(), "never-written").is_empty());
}
