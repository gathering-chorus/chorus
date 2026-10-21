// @test-type: unit — hermetic: the exit and outcome rules over given inputs; no store.
// @domain: messages
// @card: #4666
// @owner: silas
//! #4666 — the 10-20 04:45 messages run recorded sensorOutcome green while
//! messages-sent and messages-kept were unmeasured (the reader died). A pass that
//! could not measure part of its job is not green.
use chorus_crawl::sensor_run::{outcome, pass_exit, UNMEASURED_EXIT};

#[test]
fn a_pass_that_measured_everything_and_wrote_is_green() {
    assert_eq!(pass_exit(0, &[]), 0);
}

// NEGATIVE PROOF: the 10-20 shape — readings written, messages-kept unmeasured —
// must not read green (on the old code the pass returned the write's 0).
#[test]
fn a_pass_with_an_unmeasured_metric_is_unmeasured_not_green() {
    let code = pass_exit(0, &["messages-kept".into()]);
    assert_eq!(code, UNMEASURED_EXIT);
    assert_eq!(outcome(code, Some(Default::default())), "unmeasured");
}

#[test]
fn a_failed_write_keeps_its_own_exit() {
    assert_eq!(pass_exit(1, &["messages-kept".into()]), 1);
    assert_eq!(outcome(pass_exit(1, &[]), Some(Default::default())), "red");
}

// #4666 — the run's end is a spine event the Grafana rule reads.
use chorus_crawl::sensor_run::run_event_args;

#[test]
fn a_run_ends_with_an_event_naming_its_outcome() {
    let ev = run_event_args("messages", "messages-20261021t084502z", "unmeasured", None, false).unwrap();
    assert_eq!(ev[0], "sensor.run.ended");
    assert!(ev.contains(&"outcome=unmeasured".to_string()), "{ev:?}");
    assert!(ev.contains(&"sensor=messages".to_string()), "{ev:?}");
}

// NEGATIVE PROOF: a dry run writes nothing and must not page anyone.
#[test]
fn a_dry_run_emits_nothing() {
    assert!(run_event_args("messages", "x", "red", None, true).is_none());
}
