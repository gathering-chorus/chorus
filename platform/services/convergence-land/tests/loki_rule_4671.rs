// @test-type: unit — hermetic: rule() over given rows and maps; no Grafana, no store.
// @domain: alerts
// @card: #4671
// @owner: wren
//! #4671 slice 1 (Kade 10-21 12:42) — convergence-land/src/lib.rs rule() builds a
//! rule for a Loki row. Measured on #4091 AC8: a Loki query sent as a Prometheus
//! instant query errors in Grafana. A Loki row is A (queryType instant) → B (reduce
//! last of A) → C (threshold on B). A Prometheus row builds exactly today's rule.
use convergence_land::*;
use serde_json::{json, Value};

fn maps() -> Vec<Mapping> {
    let m = |f: &str, t: &str, k: &str, x: &str| Mapping { field: f.into(), target: t.into(), kind: k.into(), transform: x.into() };
    vec![
        m("label", "title", "copy", ""), m("hasDomain", "folder", "map", ""),
        m("ownedBy", "labels.role", "map", ""), m("metricQuery", "data[A].model.expr", "copy", ""),
        m("metricDatasource", "data[A].datasourceUid", "copy", ""), m("forDuration", "for", "copy", ""),
        m("name", "uid", "copy", ""),
    ]
}

fn row(ds: &str, expr: &str) -> Value {
    json!({"id": "chorus:shadow-mismatch-or-missed", "data": {"label": "Buzz shadow not matching",
           "metricQuery": expr, "metricDatasource": ds, "forDuration": "5m"},
           "links": {"ownedBy": "chorus:principal-silas", "hasDomain": "chorus:alerts"}})
}

const LOKI_EXPR: &str = r#"sum(count_over_time({job="platform-chorus"} |= "buzz.delivery.shadow" | json | verdict != "match" [15m]))"#;

#[test]
fn a_loki_row_is_an_instant_query_reduced_then_thresholded() {
    let (r, _) = rule(&row("loki", LOKI_EXPR), &maps(), "chorus-alerts").unwrap();
    let d = r["data"].as_array().unwrap();
    assert_eq!(d.len(), 3, "A, B, C");
    assert_eq!(d[0]["refId"], "A");
    assert_eq!(d[0]["datasourceUid"], "loki");
    assert_eq!(d[0]["model"]["queryType"], "instant");
    assert!(d[0]["model"].get("instant").is_none(), "Loki takes queryType, not the Prometheus instant flag");
    assert_eq!(d[0]["model"]["expr"], LOKI_EXPR);
    assert_eq!(d[1]["refId"], "B");
    assert_eq!(d[1]["datasourceUid"], "__expr__");
    assert_eq!(d[1]["model"]["type"], "reduce");
    assert_eq!(d[1]["model"]["expression"], "A");
    assert_eq!(d[1]["model"]["reducer"], "last");
    assert_eq!(d[2]["refId"], "C");
    assert_eq!(d[2]["model"]["type"], "threshold");
    assert_eq!(d[2]["model"]["expression"], "B", "the threshold reads the reduced series");
    assert_eq!(r["condition"], "C");
}

#[test]
fn negative_proof_a_prometheus_row_builds_exactly_todays_rule() {
    let (r, _) = rule(&row("prometheus", "chorus_host_pressure >= 4"), &maps(), "chorus-alerts").unwrap();
    let d = r["data"].as_array().unwrap();
    assert_eq!(d.len(), 2, "A and C only — no reduce step");
    assert_eq!(d[0]["model"]["instant"], true);
    assert!(d[0]["model"].get("queryType").is_none());
    assert_eq!(d[1]["refId"], "C");
    assert_eq!(d[1]["model"]["expression"], "A");
}

