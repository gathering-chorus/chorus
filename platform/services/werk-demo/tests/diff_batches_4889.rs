// @test-type: unit — pure: diff splitting and verdict combining; no model, no git.
// @domain: proving
// @card: #4889
//! Kade 2026-11-04 19:00: "Skipping diffs over 60k is hollow. Review big diffs per file;
//! can't read it all → unmeasured, never pass." The old gate cut the diff at 60k and
//! reviewed the head only, so #4474 (61,978) and #4499 (80,026) were reviewed in part.
use werk_demo::{combine_verdicts, diff_batches};

fn file(name: &str, hunks: usize, hunk_len: usize) -> String {
    let mut f = format!("diff --git a/{name} b/{name}\n--- a/{name}\n+++ b/{name}\n");
    for h in 0..hunks {
        f.push_str(&format!("@@ -{h},1 +{h},1 @@ {name}-hunk-{h}\n"));
        f.push_str(&format!("+{}\n", "x".repeat(hunk_len)));
    }
    f
}

#[test]
fn a_small_diff_is_one_batch_read_whole() {
    let d = file("a.rs", 2, 100) + &file("b.rs", 1, 100);
    let (b, unread) = diff_batches(&d, 60_000);
    assert_eq!(b, vec![d]);
    assert!(unread.is_empty());
}

#[test]
fn negative_proof_nothing_past_the_cap_is_dropped() {
    // three 25k files under a 60k cap: the old cut kept ~2.4 files and lost the rest
    let d = file("a.rs", 1, 25_000) + &file("b.rs", 1, 25_000) + &file("c.rs", 1, 25_000);
    let (b, unread) = diff_batches(&d, 60_000);
    assert!(unread.is_empty());
    assert_eq!(b.len(), 2, "two batches under the cap");
    assert!(b.iter().all(|x| x.chars().count() <= 60_000));
    assert_eq!(b.concat(), d, "every line of the diff is in exactly one batch");
    let truncated: String = d.chars().take(60_000).collect();
    assert!(truncated.chars().count() < d.chars().count() && !truncated.ends_with(&d[d.len() - 100..]),
            "control: the old 60k cut lost the end of c.rs");
}

#[test]
fn a_file_over_the_cap_is_split_by_hunk_under_its_header() {
    let d = file("big.rs", 4, 20_000);
    let (b, unread) = diff_batches(&d, 60_000);
    assert!(unread.is_empty());
    assert!(b.len() >= 2, "{} batches", b.len());
    for x in &b {
        assert!(x.starts_with("diff --git a/big.rs b/big.rs\n"), "each piece carries its file header");
        assert!(x.chars().count() <= 60_000);
    }
    for h in 0..4 {
        assert_eq!(b.iter().filter(|x| x.contains(&format!("big.rs-hunk-{h}\n"))).count(), 1, "hunk {h} read once");
    }
}

#[test]
fn negative_proof_a_hunk_too_big_to_read_is_named_unread() {
    let d = file("small.rs", 1, 10) + &file("huge.lock", 1, 70_000);
    let (b, unread) = diff_batches(&d, 60_000);
    assert_eq!(unread, vec!["a/huge.lock b/huge.lock".to_string()]);
    assert!(b.iter().any(|x| x.contains("small.rs")), "the rest is still reviewed");
}

#[test]
fn negative_proof_unread_is_unmeasured_never_pass() {
    let all_pass = vec![("pass".to_string(), "ok".to_string())];
    let (r, f) = combine_verdicts(&all_pass, &["a/huge.lock b/huge.lock".to_string()]);
    assert_eq!(r, "error", "a diff not read whole is never a pass");
    assert!(f.starts_with("unmeasured: too big to read whole: a/huge.lock"), "{f}");
    assert_eq!(combine_verdicts(&all_pass, &[]).0, "pass", "control: read whole and clean is a pass");
}

#[test]
fn any_failed_part_fails_and_any_errored_part_errors() {
    let p = |r: &str, f: &str| (r.to_string(), f.to_string());
    let (r, f) = combine_verdicts(&[p("pass", "fine"), p("fail", "x.rs:3 — logic — inverted check")], &[]);
    assert_eq!(r, "fail");
    assert!(f.contains("[part 2/2 fail] x.rs:3"), "{f}");
    assert_eq!(combine_verdicts(&[p("fail", "a"), p("error", "timed out")], &[]).0, "error");
}
