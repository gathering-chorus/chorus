// @test-type: unit — hermetic: reads the repo's ontology/sensors-4405 model file and the crawler's own constants; no store.
// @domain: messages
// @card: #4666
// @owner: silas
//! #4666 — every metric the messages sensor writes must be in SensorReading's
//! readingMetric sh:in (ontology/sensors-4405). #4628 taught the crawler to
//! write doubles-<kind> but the model never allowed them: staging refused the
//! whole batch with 422 (10-20 17:04), sent and kept included, so no doubles row
//! was ever stored.
use chorus_crawl::readings::DOUBLE_KINDS;

/// The values of readingMetric's sh:in list, read from the Turtle line that declares it.
fn allowed(ttl: &str) -> Vec<String> {
    let line = ttl.lines().find(|l| l.contains("chorus:readingMetric") && l.contains("sh:in")).unwrap_or("");
    let Some(open) = line.find("sh:in (") else { return vec![] };
    let rest = &line[open + 7..];
    let list = &rest[..rest.find(')').unwrap_or(rest.len())];
    list.split('"').skip(1).step_by(2).map(str::to_string).collect()
}

fn missing(ttl: &str, wanted: &[String]) -> Vec<String> {
    let a = allowed(ttl);
    wanted.iter().filter(|w| !a.contains(w)).cloned().collect()
}

fn doubles() -> Vec<String> {
    DOUBLE_KINDS.iter().map(|k| format!("doubles-{k}")).collect()
}

fn model() -> String {
    // run time, not compile time (the 4030 rule)
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR for tests");
    let path = format!("{dir}/../../../roles/silas/ontology/sensors-4405.ttl");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[test]
fn the_model_allows_every_doubles_metric_the_sensor_writes() {
    assert_eq!(missing(&model(), &doubles()), Vec::<String>::new());
}

#[test]
fn the_model_still_allows_the_metrics_it_allowed_before() {
    let before = ["nudges", "jeff-inputs", "alerts", "messages-sent", "messages-kept", "events", "commands",
        "unreadable-lines", "spine-write-age", "backup-age", "backup-bytes", "store-bytes", "missing", "stale"];
    assert_eq!(missing(&model(), &before.map(String::from)), Vec::<String>::new());
}

// NEGATIVE PROOF: the 10-20 model (14 values, no doubles) is caught, naming all five.
#[test]
fn a_model_without_the_doubles_values_is_caught() {
    let old = r#"    sh:property [ sh:path chorus:readingMetric ; sh:minCount 1 ; sh:in ( "nudges" "jeff-inputs" "alerts" "messages-sent" "messages-kept" ) ] ;"#;
    assert_eq!(missing(old, &doubles()).len(), 5);
}

#[test]
fn a_file_with_no_readingmetric_list_reads_as_all_missing_not_all_allowed() {
    assert_eq!(missing("no list here", &doubles()).len(), 5);
}
