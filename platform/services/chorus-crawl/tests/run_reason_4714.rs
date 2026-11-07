// @test-type: unit — hermetic: the reason rule over given pass output; no store.
// @domain: alerts
// @card: #4714
// @owner: kade
//! #4714 — src/sensor_run.rs `run_reason`, used by src/main.rs (sensor end event + sync reason). On 10-22 and 10-23 the 03:30 sync
//! run ended unmeasured and its sensor.run.ended event carried no reason, so the
//! "Sensor run not green" alert fired every 15 minutes for 29h with no cause.
//! A run that is not green must say why; green says nothing.
use chorus_crawl::sensor_run::run_reason;

const GRAFANA_UNMEASURED: &str = "sensor sync: run sync-x started\n\
chorus-crawl: reconcile grafana rules: UNMEASURED — no Grafana token: GRAFANA_TOKEN unset and ~/.chorus/secrets/grafana-token missing\n\
sync: UNMEASURED — not measured: Grafana rule; no readings written\n";

#[test]
fn an_unmeasured_run_names_the_check_and_its_cause() {
    let r = run_reason("unmeasured", GRAFANA_UNMEASURED).unwrap();
    assert!(r.contains("no Grafana token"), "the cause from the check line: {r}");
    assert!(r.contains("not measured: Grafana rule"), "the class that failed: {r}");
}

#[test]
fn a_green_run_has_no_reason() {
    assert_eq!(run_reason("green", GRAFANA_UNMEASURED), None);
}

// negative proof: the 10-22 shape — not green, and the pass printed no reason line.
// The old event said nothing; the reason must still be non-empty.
#[test]
fn negative_proof_a_red_run_with_no_reason_line_still_names_something() {
    let r = run_reason("red", "sensor services: run x started\nchorus-crawl: posted=0 replaced=0\n").unwrap();
    assert!(!r.trim().is_empty());
    assert!(r.contains("posted=0"), "falls back to the pass's last line: {r}");
    let r = run_reason("unmeasured", "").unwrap();
    assert!(!r.trim().is_empty(), "an empty pass still gets a reason");
}

#[test]
fn a_long_reason_is_capped() {
    let long = format!("x UNMEASURED — {}\n", "y".repeat(2000));
    assert!(run_reason("unmeasured", &long).unwrap().chars().count() <= 400);
}
