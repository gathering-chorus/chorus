// @test-type: unit — hermetic: the call-event rule over given results; no Grafana, no store.
// @domain: alerts
// @card: #4714
// @owner: kade
//! #4714 — src/lib.rs `call_event`, emitted by src/main.rs on every call. convergence-land printed only text, so the
//! sync runs of 10-22 and 10-23 that could not measure left no reason anywhere.
//! Every call ends with one structured event; anything not ok names why.
use convergence_land::call_event;

fn args(a: &[&str]) -> Vec<String> { a.iter().map(|s| s.to_string()).collect() }
fn has(ev: &[String], f: &str) -> bool { ev.iter().any(|e| e == f) }

#[test]
fn a_measured_sync_is_ok_with_its_counts() {
    let ev = call_event("kade", &args(&["sync"]), &Ok("measured tree=6 graph=6".into()));
    assert_eq!(&ev[..3], &["convergence.call", "kade", "op=sync"]);
    assert!(has(&ev, "outcome=ok") && has(&ev, "counts=tree=6 graph=6"), "{ev:?}");
    assert!(!ev.iter().any(|e| e.starts_with("reason=")));
}

#[test]
fn an_unmeasured_sync_names_its_reason() {
    let ev = call_event("kade", &args(&["sync"]), &Ok("unmeasured Grafana rules unreadable (401)".into()));
    assert!(has(&ev, "outcome=unmeasured") && has(&ev, "reason=Grafana rules unreadable (401)"), "{ev:?}");
}

#[test]
fn a_refused_alert_names_its_target_and_reason() {
    let ev = call_event("silas", &args(&["alert", "disk-full"]), &Err("no Alert row disk-full".into()));
    assert!(has(&ev, "op=alert") && has(&ev, "target=disk-full") && has(&ev, "outcome=refused"));
    assert!(has(&ev, "reason=no Alert row disk-full"), "{ev:?}");
}

// negative proof: an unmeasured or refused call with an empty why still carries a reason
#[test]
fn negative_proof_not_ok_never_has_an_empty_reason() {
    for r in [Ok("unmeasured".to_string()), Err(String::new())] {
        let ev = call_event("", &args(&["sync"]), &r);
        let reason = ev.iter().find(|e| e.starts_with("reason=")).expect("a reason field");
        assert!(reason.len() > "reason=".len(), "{ev:?}");
        assert_eq!(ev[1], "system", "no role reads as system");
    }
}
