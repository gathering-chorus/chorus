// @test-type: unit
// @domain: alerts
//! #4690 — drift (src/lib.rs) compares a rule's `for` by its value, not its
//! text: Alert rows hold "0m", Grafana answers "0s", the same wait, and the
//! crawler alert fired every hour on it (Wren measured both sides, 10-21 20:37).
use convergence_land::{drift, for_secs};
use serde_json::{json, Value};

fn rule(uid: &str, for_: &str) -> Value {
    json!({"uid": uid, "title": "t", "for": for_, "labels": {"role": "kade"}})
}
fn want(uid: &str, for_: &str) -> Vec<(String, Value)> { vec![(uid.to_string(), rule(uid, for_))] }

#[test]
fn durations_read_as_seconds_in_grafanas_forms() {
    assert_eq!(for_secs("0m"), Some(0));
    assert_eq!(for_secs("0s"), Some(0));
    assert_eq!(for_secs("5m"), Some(300));
    assert_eq!(for_secs("300s"), Some(300));
    assert_eq!(for_secs("1h30m"), Some(5400));
    assert_eq!(for_secs("2h"), Some(7200));
}

#[test]
fn the_same_wait_in_other_units_is_not_stale() {
    let d = drift(&want("messages-doubles", "0m"), &[rule("messages-doubles", "0s")]);
    assert!(d.stale.is_empty(), "{:?}", d.stale);
    let d = drift(&want("a", "5m"), &[rule("a", "300s")]);
    assert!(d.stale.is_empty(), "{:?}", d.stale);
}

#[test]
fn negative_proof_a_different_wait_or_an_unreadable_one_stays_stale() {
    let d = drift(&want("a", "5m"), &[rule("a", "0s")]);
    assert_eq!(d.stale, vec!["a (for)".to_string()]);
    assert_eq!(for_secs("soon"), None);
    assert_eq!(for_secs(""), None);
    assert_eq!(for_secs("5x"), None);
    assert_eq!(for_secs("300"), None, "a bare number has no unit (Silas)");
    let d = drift(&want("a", ""), &[rule("a", "0s")]);
    assert_eq!(d.stale, vec!["a (for)".to_string()], "an empty wait is not 0s (Silas)");
    let d = drift(&want("a", "soon"), &[rule("a", "soon")]);
    assert_eq!(d.stale, vec!["a (for)".to_string()], "an unreadable wait never reads as a match, even against itself");
}

#[test]
fn a_rule_with_no_for_on_either_side_is_not_stale_but_one_sided_is() {
    let bare = |uid: &str| json!({"uid": uid, "title": "t", "labels": {"role": "kade"}});
    let d = drift(&[("a".to_string(), bare("a"))], &[bare("a")]);
    assert!(d.stale.is_empty(), "absent on both sides matched before #4690 and still does (Silas): {:?}", d.stale);
    let d = drift(&[("a".to_string(), bare("a"))], &[rule("a", "0s")]);
    assert_eq!(d.stale, vec!["a (for)".to_string()], "absent on one side only is a difference");
}
