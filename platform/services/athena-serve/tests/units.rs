// #4186 — the serve leg's decisions, pure. The bats drives the binary with a
// stub launchctl and a stub curl for the integration proofs.
use athena_serve::{healthy, parse_args, refusal};

fn a(v: &[&str]) -> Vec<String> { v.iter().map(|s| s.to_string()).collect() }

#[test]
fn args_take_label_url_and_flags() {
    let p = parse_args(&a(&["com.chorus.athena-make", "http://localhost:3360/health", "--kickstart", "--timeout", "9"])).unwrap();
    assert_eq!(p.label, "com.chorus.athena-make");
    assert_eq!(p.health_url, "http://localhost:3360/health");
    assert!(p.kickstart);
    assert_eq!(p.timeout_s, 9);
    let d = parse_args(&a(&["x", "http://h/health"])).unwrap();
    assert!(!d.kickstart);
    assert_eq!(d.timeout_s, 60);
}

#[test]
fn args_refuse_missing_or_unknown() {
    assert!(parse_args(&a(&["only-one"])).is_err());
    assert!(parse_args(&a(&["x", "u", "--bogus"])).is_err());
    assert!(parse_args(&a(&["x", "u", "--timeout"])).is_err());
}

#[test]
fn healthy_needs_200_and_a_status_field() {
    assert!(healthy("200", r#"{"status":"healthy","uptime":3}"#));
    assert!(healthy("200", r#"{ "status" : "ok" }"#));
    assert!(healthy("200", r#"{ "ok": true, "service": "athena-make" }"#), "athena-make's own health body (first live run refused it)");
    assert!(!healthy("200", r#"{ "ok": false, "service": "athena-make" }"#));
    assert!(!healthy("503", r#"{"status":"healthy"}"#), "a 503 is not served, whatever the body says");
    assert!(!healthy("200", r#"{"latency":"6.401ms","note":"ok"}"#), "ok buried in another field is not a status");
    assert!(!healthy("000", ""), "no answer is not healthy");
}

#[test]
fn refusal_names_url_wait_and_last_answer() {
    let r = refusal("http://localhost:3360/health", 60, "000", "");
    assert!(r.contains("REFUSED") && r.contains("http://localhost:3360/health") && r.contains("60s") && r.contains("HTTP 000"));
    assert!(r.contains("nothing downstream may run"));
}
