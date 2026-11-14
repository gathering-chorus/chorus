// @test-type: unit
// @domain: alerts
//! #4970 — a rule made from a monitor tells once: its Alert row carries
//! repeatInterval, mapped onto notification_settings.repeat_interval. drift
//! (src/lib.rs) compares that wait by its value, so a hand edit in Grafana reads
//! stale and the same wait in Grafana's own units does not (Kade, 2026-11-14 17:44).
use convergence_land::{drift, for_secs};
use serde_json::{json, Value};

fn rule(uid: &str, repeat: Option<&str>) -> Value {
    let mut r = json!({"uid": uid, "title": "t", "labels": {"role": "silas"}, "notification_settings": {"receiver": "chorus-roles"}});
    if let Some(w) = repeat { r["notification_settings"]["repeat_interval"] = json!(w); }
    r
}
fn want(uid: &str, repeat: Option<&str>) -> Vec<(String, Value)> { vec![(uid.to_string(), rule(uid, repeat))] }

#[test]
fn the_same_wait_is_not_stale_in_any_unit() {
    assert_eq!(for_secs("8760h"), for_secs("1y"));
    assert_eq!(for_secs("2w"), Some(1_209_600));
    for have in ["8760h", "1y", "365d"] {
        let d = drift(&want("bedroom-disk-full", Some("8760h")), &[rule("bedroom-disk-full", Some(have))]);
        assert!(d.stale.is_empty(), "{have}: {:?}", d.stale);
    }
}

#[test]
fn a_rule_with_no_repeat_on_either_side_is_unchanged() {
    let d = drift(&want("host-pressure", None), &[rule("host-pressure", None)]);
    assert!(d.stale.is_empty(), "{:?}", d.stale);
}

#[test]
fn negative_proof_a_hand_edited_repeat_reads_stale() {
    let d = drift(&want("bedroom-disk-full", Some("8760h")), &[rule("bedroom-disk-full", Some("4h"))]);
    assert_eq!(d.stale, vec!["bedroom-disk-full (notification_settings/repeat_interval)".to_string()]);
    let d = drift(&want("bedroom-disk-full", Some("8760h")), &[rule("bedroom-disk-full", None)]);
    assert_eq!(d.stale.len(), 1, "a repeat removed by hand reads stale: {:?}", d.stale);
}

// #4970 AC5 — notify off rides on the rule as a label; a hand edit to it reads stale.
fn labelled(uid: &str, notify: Option<&str>) -> Value {
    let mut r = rule(uid, None);
    if let Some(n) = notify { r["labels"]["notify"] = json!(n); }
    r
}

#[test]
fn notify_off_on_both_sides_is_not_stale() {
    let d = drift(&[("buzz-shadow-not-matching".into(), labelled("buzz-shadow-not-matching", Some("off")))],
                  &[labelled("buzz-shadow-not-matching", Some("off"))]);
    assert!(d.stale.is_empty(), "{:?}", d.stale);
}

#[test]
fn negative_proof_notify_turned_back_on_by_hand_reads_stale() {
    let d = drift(&[("buzz-shadow-not-matching".into(), labelled("buzz-shadow-not-matching", Some("off")))],
                  &[labelled("buzz-shadow-not-matching", None)]);
    assert_eq!(d.stale, vec!["buzz-shadow-not-matching (labels/notify)".to_string()]);
}
