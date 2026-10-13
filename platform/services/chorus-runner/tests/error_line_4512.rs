// @test-type: unit
// @domain: services
//! #4512 AC4 — every failed call (a refused caller included) leaves ONE log
//! line naming caller, tool, reason, status and trace, as one JSON object.

use chorus_runner::{error_line, new_trace_id, refusal_reason, traceparent};
use chorus_oidc::auth::AuthError;
use serde_json::Value;

#[test]
fn a_refused_call_logs_caller_tool_reason_status_and_trace() {
    let trace = new_trace_id();
    let line = error_line("unverified", "-", refusal_reason(&AuthError::Expired), 401, &trace);
    assert!(!line.contains('\n'), "one line: {line}");
    let v: Value = serde_json::from_str(&line).expect("one JSON object");
    assert_eq!(v["caller"], "unverified");
    assert_eq!(v["status"], 401);
    assert_eq!(v["trace"], trace.as_str());
    assert!(v["reason"].as_str().unwrap().contains("expired"), "{line}");
    assert_eq!(v["event"], "runner.call.failed");
}

#[test]
fn every_refusal_reason_is_named_never_blank() {
    for e in [AuthError::Missing, AuthError::Hs256Retired, AuthError::Malformed, AuthError::BadSignature,
              AuthError::WrongAudience, AuthError::Expired, AuthError::WebIdNotAllowed, AuthError::UnknownAlg,
              AuthError::IssuerMismatch, AuthError::JwksUnreachable] {
        assert!(refusal_reason(&e).len() > 10, "{e:?}");
    }
}

#[test]
fn the_trace_passed_to_the_door_is_the_trace_logged() {
    let trace = new_trace_id();
    assert_eq!(trace.len(), 32);
    let tp = traceparent(&trace);
    assert!(tp.starts_with(&format!("00-{trace}-")), "{tp}");
    assert_ne!(new_trace_id(), trace, "two calls, two traces");
}
