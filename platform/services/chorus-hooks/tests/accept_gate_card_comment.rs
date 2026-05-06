//! #2177 — accept_gate reads `demo:preflight-pass` card comment as demo evidence.
//!
//! Retires the brief-file check in `accept_gate::demo_brief_exists` that scanned
//! `roles/wren/briefs/` for files matching `*demo*<card_id>*`. /demo skill writes
//! a `demo:preflight-pass` comment on the card; the gate must read THAT, not files.
//!
//! Bug Jeff/Wren see today: `cards done <id>` blocked by accept_gate even when
//! `demo:preflight-pass` is on the card, because no brief file was written.
//!
//! AC:
//!   1. Hook reads cards API for 'demo:preflight-pass' comment.
//!   2. File-based check removed.
//!   3. Zero-hits grep confirms no brief-file path remnants.

use chorus_hooks::accept_gate::demo_evidence_exists;

#[test]
fn passes_when_preflight_pass_comment_present() {
    let view = "\
#2177 Test
  Status:   WIP
  Owner:    silas
  Comments (1):
    [silas] demo:preflight-pass
";
    assert!(demo_evidence_exists(view), "must accept demo:preflight-pass comment as evidence");
}

#[test]
fn denies_when_only_unrelated_comments() {
    let view = "\
#2177 Test
  Status:   WIP
  Owner:    silas
  Comments (1):
    [silas] working on it
";
    assert!(!demo_evidence_exists(view), "no preflight-pass = no demo evidence");
}

#[test]
fn denies_on_empty_view() {
    assert!(!demo_evidence_exists(""));
}
