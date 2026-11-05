// @test-type: unit — the review ledger over recorded findings; no model, no git
// @domain: proving
// @card: #4889
//! Jeff 11-05 10:06 ("isnt there just a new commit to review?"): a round reads only what
//! changed since the last reviewed commit, and settled findings stay settled. Kade 10:07:
//! an earlier round's open finding carries forward until a later diff fixes it, never
//! dropped. No waiver (Kade 10:24). Exercises werk-demo src/lib.rs apply_ledger, last_reviewed.
use werk_demo::{apply_ledger, finding_key, last_reviewed};

const PRIOR: &str = "gate-arch: [part 1/2 fail] src/a.rs:10 — item 2 — a reaches into b | src/b.rs:4 — item 3 — two paths | note: src/c.rs:1 — item 6 — naming";

fn files(v: &[&str]) -> Vec<String> { v.iter().map(|s| s.to_string()).collect() }

#[test]
fn negative_proof_an_unfixed_old_finding_still_shows_red_on_a_delta_round() {
    // the delta touched neither file and raised nothing new
    let (r, f) = apply_ledger("pass", "no findings", Some(PRIOR), Some(&files(&["src/z.rs"])));
    assert_eq!(r, "fail", "{f}");
    assert!(f.contains("carried (not yet fixed): src/a.rs:10 — item 2") && f.contains("carried (not yet fixed): src/b.rs:4 — item 3"), "{f}");
    assert!(!f.contains("src/c.rs"), "a note is not carried as a failure: {f}");
}

#[test]
fn a_finding_whose_file_changed_and_is_not_raised_again_is_fixed() {
    let (r, f) = apply_ledger("pass", "no findings", Some(PRIOR), Some(&files(&["src/a.rs", "src/b.rs"])));
    assert_eq!((r.as_str(), f.as_str()), ("pass", "no findings"));
}

#[test]
fn a_finding_raised_again_counts_once_not_twice() {
    let now = "[part 1/1 fail] src/a.rs:12 — item 2 — a still reaches into b";
    let (r, f) = apply_ledger("fail", now, Some(PRIOR), Some(&files(&["src/a.rs"])));
    assert_eq!(r, "fail");
    assert_eq!(f.matches("src/a.rs").count(), 1, "{f}");
    assert!(f.contains("carried (not yet fixed): src/b.rs:4"), "{f}");
}

#[test]
fn a_whole_branch_round_carries_nothing_and_an_error_stays_unmeasured() {
    assert_eq!(apply_ledger("pass", "no findings", Some(PRIOR), None).0, "pass");
    assert_eq!(apply_ledger("error", "semgrep: x is not on canonical", Some(PRIOR), Some(&files(&[]))).0, "error");
}

#[test]
fn the_last_reviewed_commit_is_the_newest_record_that_names_one() {
    let w = "{\"ts\":1,\"event\":\"demo.gate.result\",\"card_id\":4889,\"gate\":\"arch\",\"commit\":\"aaa\",\"findings\":\"x\"}\n\
             {\"ts\":2,\"event\":\"demo.gate.result\",\"card_id\":4889,\"gate\":\"arch\",\"commit\":\"\",\"findings\":\"error\"}\n\
             {\"ts\":3,\"event\":\"demo.gate.result\",\"card_id\":4889,\"gate\":\"code\",\"commit\":\"ccc\",\"findings\":\"y\"}\n\
             {\"ts\":4,\"event\":\"demo.gate.result\",\"card_id\":48890,\"gate\":\"arch\",\"commit\":\"zzz\",\"findings\":\"z\"}\n";
    assert_eq!(last_reviewed(w, 4889, "arch"), Some(("aaa".to_string(), "x".to_string())));
    assert_eq!(last_reviewed(w, 4889, "quality"), None);
}

#[test]
fn a_findings_key_is_its_item_and_file() {
    assert_eq!(finding_key("src/a.rs:10 — item 2 — x"), (2, "src/a.rs".to_string()));
    assert_eq!(finding_key("platform/x.ts:3 — /gate-quality item 2: swallowed"), (2, "platform/x.ts".to_string()));
    assert_eq!(finding_key("a.rs:1 — b is undefined"), (0, "a.rs".to_string()));
}
