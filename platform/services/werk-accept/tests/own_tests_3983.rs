//! #3983 — every accepted card carries its own-test evidence.

use werk_accept::own_tests_at_accept;

fn line(card: u64, extra: &str) -> String {
    format!("{{\"event\":\"demo.test_result\",\"card_id\":{},\"result\":\"pass\"{}}}\n", card, extra)
}

#[test]
fn accept_reads_the_coverage_the_cards_test_run_recorded() {
    let w = line(3983, ",\"coverage\":\"zero-coverage:0\"") + &line(3983, ",\"coverage\":\"covered:14\"");
    assert_eq!(own_tests_at_accept(&w, 3983), ("covered".into(), "14".into()));
}

/// NEGATIVE PROOF: the latest run's zero-coverage is what accept carries, and
/// a card with no recorded coverage is `not-recorded`, never covered
#[test]
fn zero_coverage_and_not_recorded_stay_visible_at_accept() {
    let w = line(3983, ",\"coverage\":\"covered:14\"") + &line(3983, ",\"coverage\":\"zero-coverage:0\"");
    assert_eq!(own_tests_at_accept(&w, 3983).0, "zero-coverage");
    assert_eq!(own_tests_at_accept(&line(3983, ""), 3983).0, "not-recorded");
    assert_eq!(own_tests_at_accept(&line(39831, ",\"coverage\":\"covered:2\""), 3983).0, "not-recorded");
}
