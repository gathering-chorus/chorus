// @domain: builds — placed by chorus-crawl's domain rules (#4538)
//! #4453 — a long cargo unit shows its count while it runs. Jeff, 2026-10-10
//! 20:20 (#4235 run 4): chorus-hooks ran 15 minutes with no count visible.
use werk_test::CargoProgress;

const LIVE: &str = "\
    Starting 4 tests across 2 binaries (1 test skipped)
        PASS [   0.012s] chorus-hooks::a tests::one
        PASS [   0.020s] chorus-hooks::a tests::two
        FAIL [   0.101s] chorus-hooks::b tests::three
--- STDOUT: chorus-hooks::b tests::three ---
thread 'three' panicked at src/b.rs:9:5:
        FAIL [   0.101s] chorus-hooks::b tests::three
";

#[test]
fn counts_cases_as_they_finish_against_the_planned_total() {
    let mut p = CargoProgress::default();
    for l in LIVE.lines().take(3) {
        p.feed(l);
    }
    assert_eq!(p.line(), "2 passed · 0 failed of 4", "mid-run: two cases done");
    for l in LIVE.lines().skip(3) {
        p.feed(l);
    }
    assert_eq!((p.passed, p.failed, p.total), (2, 1, Some(4)));
}

#[test]
fn negative_proof_a_failure_printed_twice_counts_once_and_noise_counts_nothing() {
    let mut p = CargoProgress::default();
    for l in LIVE.lines() {
        p.feed(l);
    }
    // the summary repeats the FAIL line; a panic message is not a case
    assert_eq!(p.failed, 1);
    assert_eq!(p.passed + p.failed, 3);
}

#[test]
fn before_nextest_names_a_total_the_line_says_so_far() {
    let mut p = CargoProgress::default();
    p.feed("        PASS [   0.012s] x::y t");
    assert_eq!(p.line(), "1 passed · 0 failed so far");
}
