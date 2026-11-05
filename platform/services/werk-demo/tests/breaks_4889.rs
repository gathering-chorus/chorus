// @test-type: unit — pure: the breaks/notes split over recorded findings; no model
// @domain: proving
// @card: #4889
//! Jeff 2026-11-05 16:05 ("Yes, certainly"): a finding that breaks something Jeff uses
//! blocks the land; every other finding is a note on his go page, and ships.
//! Exercises werk-demo src/lib.rs apply_breaks, open_findings, breaks_answer, breaks_prompt.
use werk_demo::{apply_breaks, breaks_answer, breaks_prompt, open_findings};

const RED: &str = "[part 1/2 fail] /code-review: a.rs:1 — loses Jeff's message | note: b.rs:2 — style | carried (not yet fixed): c.rs:3 — item 2 — slow";

#[test]
fn the_open_findings_are_the_fails_and_the_carried_never_the_notes() {
    assert_eq!(open_findings(RED), vec!["a.rs:1 — loses Jeff's message", "c.rs:3 — item 2 — slow"]);
}

#[test]
fn findings_that_break_nothing_become_notes_and_the_gate_passes() {
    let (r, f) = apply_breaks("fail", RED, Some(&[false, false]));
    assert_eq!(r, "pass", "{f}");
    assert!(f.contains("note: a.rs:1 — loses Jeff's message") && f.contains("note: c.rs:3 — item 2 — slow") && f.contains("note: b.rs:2"), "{f}");
}

#[test]
fn negative_proof_one_finding_that_breaks_something_jeff_uses_blocks() {
    let (r, f) = apply_breaks("fail", RED, Some(&[true, false]));
    assert_eq!(r, "fail");
    assert!(f.contains("/code-review: a.rs:1 — loses Jeff's message") && f.contains("note: c.rs:3"), "{f}");
}

#[test]
fn negative_proof_no_answer_or_a_mismatched_answer_leaves_the_red() {
    assert_eq!(apply_breaks("fail", RED, None), ("fail".to_string(), RED.to_string()));
    assert_eq!(apply_breaks("fail", RED, Some(&[false])).0, "fail");
    assert_eq!(apply_breaks("error", "x", Some(&[])).0, "error");
}

#[test]
fn the_answer_is_a_json_list_of_booleans_and_anything_else_is_none() {
    assert_eq!(breaks_answer("ok\n```json\n[true, false]\n```\n"), Some(vec![true, false]));
    assert_eq!(breaks_answer("```json\n[true, maybe]\n```"), None);
    assert_eq!(breaks_answer("{\"findings\":[]}"), None);
}

#[test]
fn the_question_numbers_each_finding_and_names_what_jeff_uses() {
    let p = breaks_prompt(&["a.rs:1 — x".to_string(), "b.rs:2 — y".to_string()]);
    assert!(p.contains("1. a.rs:1 — x\n2. b.rs:2 — y") && p.contains("his messages reaching a role") && p.contains("go page"), "{p}");
}
