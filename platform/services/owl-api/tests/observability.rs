//! #3354 observability ility — red-first tests (DEC-1674).
//!
//! These fail until TelemetryLine / ReqStatus / telemetry_path land in lib.rs.
//! They assert the envelope settled in the design of record (Kade's DE pass):
//! typed status, result_count, trace_id, dated-file rotation.

use owl_api::{telemetry_path, ReqStatus, TelemetryLine};

#[test]
fn telemetry_line_carries_the_full_envelope() {
    let t = TelemetryLine {
        class: "Domain".into(),
        entity: "tests".into(),
        route: "detail".into(),
        fold: String::new(),
        status: ReqStatus::Ok,
        result_count: 9,
        total_ms: 12,
        upstream_ms: 9,
        caller: "wren".into(),
        trace_id: "abc-123".into(),
    };
    let line = t.to_jsonl(1781200000000);
    assert!(line.contains("\"event\":\"api.request.served\""));
    assert!(line.contains("\"class\":\"Domain\""));
    assert!(line.contains("\"status\":\"ok\""));
    assert!(line.contains("\"result_count\":9"), "count:0+ok is the silent-broken-chain signal — count must serialize");
    assert!(line.contains("\"trace_id\":\"abc-123\""), "trace_id joins the card→werk chain");
    assert!(line.ends_with('\n'), "newline-terminated jsonl");
}

#[test]
fn typed_status_never_conflates_refusal_with_error() {
    // The 2026-06-11 noise lesson in the schema: refusals are not errors.
    assert_eq!(ReqStatus::Ok.as_str(), "ok");
    assert_eq!(ReqStatus::Refused("not-found".into()).as_str(), "refused:not-found");
    assert_eq!(ReqStatus::Error("fuseki-down".into()).as_str(), "error:fuseki-down");
}

#[test]
fn telemetry_path_is_dated_for_rotation() {
    // Kade's rotation catch: dated files = free day-boundary rotation.
    let p = telemetry_path("/tmp/chorus-x", 1781200000000); // 2026-06-11 UTC
    assert_eq!(p, "/tmp/chorus-x/ops/logs/owl-api-20260611.jsonl");
}
