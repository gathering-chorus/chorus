//! #4416 — every nightly suite row carries how long the suite ran.
//! Jeff, 2026-10-01: "we have run times for suites now?" — they did not.

use werk_test::nightly_run::{
    parse_suite_line, parse_unit_time_line, seconds, suite_row_payload, suite_row_payload_timed, unit_line_key, SuiteRow,
};

#[test]
fn a_unit_that_sleeps_two_seconds_records_at_least_two() {
    werk_test::time_unit("fixture/slow-4416.bats", || std::thread::sleep(std::time::Duration::from_millis(2000)));
    let line = werk_test::unit_time_line("bats", "fixture/slow-4416.bats", "fixture/slow-4416.bats").expect("timed");
    let (_, ms) = parse_unit_time_line(&line).unwrap();
    assert!(ms >= 2000, "{line}");
}

#[test]
fn negative_proof_a_unit_that_exits_at_once_records_under_one_second() {
    werk_test::time_unit("fixture/fast-4416.bats", || ());
    let line = werk_test::unit_time_line("bats", "fixture/fast-4416.bats", "fixture/fast-4416.bats").expect("timed");
    let (_, ms) = parse_unit_time_line(&line).unwrap();
    assert!(ms < 1000, "{line}");
}

#[test]
fn an_untimed_unit_prints_no_time_line() {
    assert_eq!(werk_test::unit_time_line("bats", "never-ran.bats", "never-ran.bats"), None);
}

#[test]
fn the_time_line_and_the_unit_line_share_one_key() {
    let (key, ms) = parse_unit_time_line("nightly-unit-time|bats|platform/tests/x.bats|12345").unwrap();
    assert_eq!(key, "bats|platform/tests/x.bats");
    assert_eq!(ms, 12345);
    assert_eq!(unit_line_key("nightly-unit|bats|platform/tests/x.bats|pass|3 pass, 0 fail").as_deref(), Some(key.as_str()));
    assert_eq!(parse_unit_time_line("nightly-unit|bats|x|pass|1 pass"), None);
    assert_eq!(parse_unit_time_line("nightly-unit-time|bats|x|not-a-number"), None);
}

#[test]
fn a_timed_row_writes_its_seconds_beside_the_suite_line_and_in_the_graph() {
    let mut row = SuiteRow::new("bats", "platform/tests/x.bats", "kade", "pass", "3 pass, 0 fail");
    assert_eq!(row.time_line(), None);
    row.millis = Some(12_345);
    assert_eq!(row.time_line().as_deref(), Some("SUITETIME|bats|platform/tests/x.bats|12.3"));
    let body = suite_row_payload_timed("2026-10-01T16:30:00", 7, &row, 1);
    assert!(body.ends_with(",\"suiteSeconds\":\"12.3\"}"), "{body}");
    assert_eq!(body.matches('{').count(), 1, "one object: {body}");
    // an untimed row's payload is exactly the old one
    row.millis = None;
    assert_eq!(suite_row_payload_timed("r", 1, &row, 1), suite_row_payload("r", 1, &row, 1));
}

#[test]
fn every_suite_reader_skips_the_time_line() {
    assert_eq!(parse_suite_line("SUITETIME|bats|platform/tests/x.bats|12.3"), None);
    assert_eq!(seconds(999), "0.9");
    assert_eq!(seconds(61_050), "61.0");
}
