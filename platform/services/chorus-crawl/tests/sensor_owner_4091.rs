// @test-type: unit — hermetic: the event a sensor run ends with, built from given inputs; no store.
// @domain: heralds
// @card: #4091
// @owner: kade
//! #4091 AC8 — a sensor run that ends red or unmeasured reaches the sensor's
//! owner. The Grafana rule routes by the `owner` field of `sensor.run.ended`,
//! so the event carries the Sensor row's owner, never a hard-coded role.
use chorus_crawl::sensor_run::{event_owner, run_event_args};

#[test]
fn a_red_run_names_its_owner_on_the_event() {
    let ev = run_event_args("services", "services-20261021t085402z", "red", Some("silas"), false).unwrap();
    assert!(ev.contains(&"owner=silas".to_string()), "{ev:?}");
    assert!(ev.contains(&"outcome=red".to_string()), "{ev:?}");
}

// NEGATIVE PROOF: no owner → no owner field, so the alert records it unrouted
// instead of guessing a role (on the old code there was never an owner field).
#[test]
fn a_run_with_no_owner_carries_no_owner_field() {
    let ev = run_event_args("services", "x", "red", None, false).unwrap();
    assert!(ev.iter().all(|a| !a.starts_with("owner=")), "{ev:?}");
}

#[test]
fn a_dry_run_still_emits_nothing() {
    assert!(run_event_args("services", "x", "red", Some("silas"), true).is_none());
}

#[test]
fn the_owner_is_the_sensor_rows_owner_bare() {
    let row = r#"{ "links": { "ownedBy": "chorus:principal-silas" } }"#;
    assert_eq!(event_owner(Some(row), "kade").as_deref(), Some("silas"));
}

#[test]
fn a_first_run_with_no_row_is_owned_by_the_role_that_ran_it() {
    assert_eq!(event_owner(None, "kade").as_deref(), Some("kade"));
    assert_eq!(event_owner(None, "").as_deref(), None);
}

// NEGATIVE PROOF: a row whose ownedBy is empty names no one.
#[test]
fn a_row_with_an_empty_owner_names_no_one() {
    assert_eq!(event_owner(Some(r#"{ "links": { "ownedBy": "" } }"#), "kade"), None);
}
