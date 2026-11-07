// @test-type: unit — hermetic: the credential, loopback, ICD-map and rule/silence rules over given inputs; no Grafana, no store.
// @domain: alerts
// @card: #4091
// @owner: kade
//! #4091 AC1/AC2/AC6 — convergence-land writes a Grafana rule from an Alert row
//! using only the ICD field maps, on loopback, with a scoped credential.
use convergence_land::*;
use serde_json::{json, Value};

fn maps() -> Vec<Mapping> {
    let m = |f: &str, t: &str, k: &str, x: &str| Mapping { field: f.into(), target: t.into(), kind: k.into(), transform: x.into() };
    vec![
        m("label", "title", "copy", ""), m("comment", "annotations.summary", "copy", ""),
        m("alertRoute", "labels.severity", "copy", ""), m("hasDomain", "folder", "map", ""),
        m("ownedBy", "labels.role", "map", ""), m("metricQuery", "data[A].model.expr", "copy", ""),
        m("metricDatasource", "data[A].datasourceUid", "copy", ""), m("forDuration", "for", "copy", ""),
        m("evalInterval", "group.interval", "copy", ""), m("name", "uid", "copy", ""),
        m("contactPoint", "notification.receiver", "constant", "chorus-roles"),
    ]
}

// the door's single-row envelope, as GET /v1/alerts/alerts/host-pressure serves it
fn row() -> Value {
    json!({"id": "chorus:host-pressure", "data": {"label": "Host memory pressure",
           "comment": "pressure level >= 4 for 5m", "alertRoute": "critical", "metricQuery": "chorus_host_pressure >= 4",
           "metricDatasource": "prometheus", "forDuration": "5m", "evalInterval": "1m"},
           "links": {"ownedBy": "chorus:principal-silas", "hasDomain": "chorus:infrastructure"}})
}

#[test]
fn an_alert_row_becomes_a_grafana_rule_through_the_maps() {
    let (r, interval) = rule(&row(), &maps(), "chorus-infrastructure").unwrap();
    assert_eq!(r["title"], "Host memory pressure");
    assert_eq!(r["uid"], "host-pressure");
    assert_eq!(r["for"], "5m");
    assert_eq!(r["labels"]["role"], "silas", "routes to the owner, bare");
    assert_eq!(r["labels"]["severity"], "critical");
    assert_eq!(r["labels"]["domain"], "infrastructure");
    assert_eq!(r["annotations"]["summary"], "pressure level >= 4 for 5m");
    assert_eq!(r["data"][0]["model"]["expr"], "chorus_host_pressure >= 4");
    assert_eq!(r["data"][0]["datasourceUid"], "prometheus");
    assert_eq!(r["notification_settings"]["receiver"], "chorus-roles");
    assert_eq!(r["folderUID"], "chorus-infrastructure");
    assert_eq!(interval, "1m");
}

// AC2: a changed map changes what is written, with no code change.
#[test]
fn a_changed_map_changes_the_rule() {
    let mut m = maps();
    m.iter_mut().find(|x| x.field == "alertRoute").unwrap().target = "labels.priority".into();
    let (r, _) = rule(&row(), &m, "f").unwrap();
    assert_eq!(r["labels"]["priority"], "critical");
    assert!(r["labels"].get("severity").is_none());
}

// NEGATIVE PROOF (AC2): no maps → nothing is guessed.
#[test]
fn no_maps_is_refused() {
    let e = rule(&row(), &[], "f").unwrap_err();
    assert!(e.contains("no Grafana ICD field maps"), "{e}");
}

// NEGATIVE PROOF: a row with no query cannot become a working rule.
#[test]
fn a_row_with_no_query_is_refused_naming_it() {
    let mut r = row();
    r["data"].as_object_mut().unwrap().remove("metricQuery");
    let e = rule(&r, &maps(), "f").unwrap_err();
    assert!(e.contains("metricQuery"), "{e}");
}

// NEGATIVE PROOF (AC6): no credential refuses every write, naming where it should be, never a value.
#[test]
fn no_credential_is_refused_naming_the_env_var_and_file() {
    let e = credential(None, None, "/Users/x").unwrap_err();
    assert!(e.contains("GRAFANA_ALERTING_TOKEN") && e.contains("/Users/x/.chorus/credentials/grafana-alerting.token"), "{e}");
    assert!(credential(Some("  ".into()), Some(("".into(), 0o600)), "/h").is_err());
}

#[test]
fn the_env_var_wins_then_a_private_file() {
    assert_eq!(credential(Some("envtok".into()), Some(("filetok".into(), 0o600)), "/h").unwrap(), "envtok");
    assert_eq!(credential(None, Some(("filetok\n".into(), 0o600)), "/h").unwrap(), "filetok");
}

// NEGATIVE PROOF: a token file others can read is refused, and its value is not printed.
#[test]
fn a_world_readable_token_file_is_refused() {
    let e = credential(None, Some(("secret-value".into(), 0o644)), "/h").unwrap_err();
    assert!(e.contains("chmod 600") && !e.contains("secret-value"), "{e}");
}

#[test]
fn grafana_on_loopback_only() {
    for ok in ["http://localhost:3100", "http://127.0.0.1:3100/", "http://[::1]:3100"] {
        assert!(loopback(ok).is_ok(), "{ok}");
    }
    // NEGATIVE PROOF (ADR-012)
    for bad in ["http://10.20.30.40:3100", "https://grafana.example.com", "http://localhost.evil.com:3100", ""] {
        assert!(loopback(bad).is_err(), "{bad}");
    }
}

#[test]
fn maps_read_from_a_sparql_result() {
    let s = json!({"results": {"bindings": [
        {"field": {"value": "label"}, "target": {"value": "title"}, "kind": {"value": "copy"}, "transform": {"value": "String copy"}},
        {"field": {"value": ""}, "target": {"value": "x"}}
    ]}});
    let m = mappings(&s);
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].target, "title");
}

#[test]
fn a_silence_matches_the_rule_uid_and_is_bounded() {
    let s = silence("host-pressure", 120, "2026-10-21T10:00:00Z", "2026-10-21T12:00:00Z", "kade").unwrap();
    assert_eq!(s["matchers"][0]["value"], "host-pressure");
    assert_eq!(s["endsAt"], "2026-10-21T12:00:00Z");
    // NEGATIVE PROOF: no zero-length or open-ended silence
    assert!(silence("x", 0, "a", "b", "k").is_err());
    assert!(silence("x", 8 * 24 * 60, "a", "b", "k").is_err());
}

#[test]
fn links_are_read_bare_and_data_wins() {
    assert_eq!(field(&row(), "hasDomain").as_deref(), Some("infrastructure"));
    assert_eq!(field(&row(), "label").as_deref(), Some("Host memory pressure"));
    assert_eq!(field(&row(), "name").as_deref(), Some("host-pressure"), "a single row's name is its id");
    assert_eq!(field(&row(), "nope"), None);
}

// #4091 AC4 — the grafana sync reading.
fn built() -> Value { rule(&row(), &maps(), "chorus-infrastructure").unwrap().0 }

#[test]
fn a_rule_as_its_row_builds_it_is_in_sync() {
    let d = drift(&[("host-pressure".into(), built())], &[built()]);
    assert_eq!(d, Drift { tree: 1, graph: 1, missing: vec![], stale: vec![] });
}

// NEGATIVE PROOF (AC4): a row with no rule reads missing.
#[test]
fn a_row_with_no_rule_reads_missing() {
    let d = drift(&[("host-pressure".into(), built())], &[]);
    assert_eq!(d.missing, vec!["host-pressure".to_string()]);
    assert_eq!(d.graph, 0);
}

// NEGATIVE PROOF (AC4): a rule changed by hand in Grafana reads stale, naming what changed.
#[test]
fn a_hand_edited_rule_reads_stale() {
    let mut hand = built();
    hand["data"][0]["model"]["expr"] = json!("chorus_host_pressure >= 2");
    hand["for"] = json!("1m");
    let d = drift(&[("host-pressure".into(), built())], &[hand]);
    assert_eq!(d.stale.len(), 1);
    assert!(d.stale[0].contains("data/0/model/expr") && d.stale[0].contains("for"), "{:?}", d.stale);
}

#[test]
fn a_rule_no_row_claims_reads_stale() {
    let mut orphan = built();
    orphan["uid"] = json!("gone-row");
    let d = drift(&[], &[orphan]);
    assert_eq!(d.stale, vec!["gone-row (no Alert row claims it)".to_string()]);
}

// #4091 reopen 2 — Grafana 12 403s a GET of a missing folder; the listing decides.
#[test]
fn a_folder_is_found_in_the_listing() {
    let l = r#"[{"uid":"sharedwithme","title":"Shared with me"},{"uid":"chorus-infrastructure","title":"Chorus infrastructure"}]"#;
    assert_eq!(folder_present(l, "chorus-infrastructure"), Some(true));
    assert_eq!(folder_present(l, "chorus-alerts"), Some(false));
}

// NEGATIVE PROOF: an error body (the 403 Grafana sends) is not read as "absent", so no folder is created blind.
#[test]
fn an_error_body_is_not_a_listing() {
    assert_eq!(folder_present(r#"{"message":"Permissions needed: folders:read"}"#, "x"), None);
    assert_eq!(folder_present("not json", "x"), None);
}

// #4091 reopen 3 — two header files held at once in one run (Grafana's, then the
// door's for the write-back). NEGATIVE PROOF: on the pid-only name the second
// failed "File exists" and the rule's uid was never written back.
#[test]
fn two_header_files_can_be_held_at_once_and_each_is_private_and_removed() {
    use std::os::unix::fs::PermissionsExt;
    let a = header_file("Authorization: Bearer a").unwrap();
    let b = header_file("Authorization: Bearer b").unwrap();
    assert_ne!(a.0, b.0);
    for h in [&a, &b] {
        assert_eq!(std::fs::metadata(&h.0).unwrap().permissions().mode() & 0o777, 0o600);
    }
    let (pa, pb) = (a.0.clone(), b.0.clone());
    drop(a);
    drop(b);
    assert!(!std::path::Path::new(&pa).exists() && !std::path::Path::new(&pb).exists());
}
