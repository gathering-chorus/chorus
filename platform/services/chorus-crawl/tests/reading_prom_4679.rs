// @test-type: unit — hermetic: prom text from given readings.
// @domain: alerts
// @card: #4679
// @owner: wren
//! #4679 — the messages counts never reached Prometheus: they are written for the
//! finished day while the backup metrics are written for today, and prom_lines kept
//! one newest day for the whole sensor (chorus-crawl/src/readings.rs). Measured
//! 10-22 10:05: 24h of chorus_sensor_reading{sensor="messages"} holds only
//! backup-age, backup-bytes and store-bytes, so both #4671 messages rules read
//! "normal" on no data.
use chorus_crawl::readings::*;

fn row(sensor: &str, metric: &str, day: &str, v: &str, role: Option<&str>) -> ReadingRow {
    let mut f = vec![
        ("readingOfSensor".to_string(), sensor.to_string()),
        ("readingDay".to_string(), day.to_string()),
        ("readingMetric".to_string(), metric.to_string()),
        ("readingValue".to_string(), v.to_string()),
    ];
    if let Some(r) = role { f.push(("readingRole".to_string(), r.to_string())); }
    (reading_name(sensor, metric, role, day), f)
}

/// The live shape: counts for the finished day, point-in-time metrics for today.
fn live_shape() -> Vec<ReadingRow> {
    vec![
        row("messages", "messages-sent", "2026-10-20", "500", None),
        row("messages", "messages-sent", "2026-10-21", "696", None),
        row("messages", "messages-kept", "2026-10-21", "655", None),
        row("messages", "doubles-nudge", "2026-10-21", "0", Some("wren")),
        row("messages", "backup-age", "2026-10-22", "8", None),
        row("messages", "store-bytes", "2026-10-22", "33243136", None),
    ]
}

#[test]
fn each_metric_keeps_its_own_newest_day_so_yesterdays_counts_reach_prometheus() {
    let text = prom_lines("messages", &live_shape());
    assert!(text.contains("chorus_sensor_reading{sensor=\"messages\",metric=\"messages-sent\",subject=\"\"} 696\n"), "{text}");
    assert!(text.contains("chorus_sensor_reading{sensor=\"messages\",metric=\"messages-kept\",subject=\"\"} 655\n"), "{text}");
    assert!(text.contains("chorus_sensor_reading{sensor=\"messages\",metric=\"doubles-nudge\",subject=\"wren\"} 0\n"), "{text}");
    assert!(text.contains("chorus_sensor_reading{sensor=\"messages\",metric=\"backup-age\",subject=\"\"} 8\n"), "{text}");
    assert!(!text.contains("} 500\n"), "an older day of the same metric is still left out: {text}");
}

#[test]
fn negative_proof_one_newest_day_for_the_whole_sensor_drops_the_counts() {
    // The rule this card replaces: keep only rows of the sensor-wide newest day.
    let rows = live_shape();
    let newest = rows.iter().filter_map(|(_, f)| f.iter().find(|(k, _)| k == "readingDay").map(|(_, v)| v.clone())).max().unwrap();
    let kept: Vec<_> = rows.iter().filter(|(_, f)| f.iter().any(|(k, v)| k == "readingDay" && *v == newest)).collect();
    assert!(kept.iter().all(|(_, f)| !f.iter().any(|(k, v)| k == "readingMetric" && v == "messages-sent")),
        "the old rule cannot see messages-sent, which is the gap this card closes");
    let text = prom_lines("messages", &rows);
    assert!(text.contains("metric=\"messages-sent\""), "the new rule must: {text}");
}

#[test]
fn an_age_gauge_says_how_many_days_behind_the_newest_reading_each_series_is() {
    let text = prom_lines("messages", &live_shape());
    assert!(text.contains("# TYPE chorus_sensor_reading_age_days gauge\n"), "{text}");
    assert!(text.contains("chorus_sensor_reading_age_days{sensor=\"messages\",metric=\"messages-sent\",subject=\"\"} 1\n"), "{text}");
    assert!(text.contains("chorus_sensor_reading_age_days{sensor=\"messages\",metric=\"backup-age\",subject=\"\"} 0\n"), "{text}");
}

#[test]
fn negative_proof_an_age_gauge_tells_a_stale_series_from_a_current_one() {
    let mut rows = live_shape();
    rows.push(row("messages", "doubles-reply", "2026-10-18", "3", None));
    let text = prom_lines("messages", &rows);
    assert!(text.contains("chorus_sensor_reading_age_days{sensor=\"messages\",metric=\"doubles-reply\",subject=\"\"} 4\n"), "{text}");
    assert!(!text.contains("metric=\"doubles-reply\",subject=\"\"} 0\n"), "a 4-day-old value must not read as current: {text}");
}

#[test]
fn label_values_escape_backslash_quote_and_newline() {
    let rows = vec![row("messages", "doubles-a\\b", "2026-10-21", "1", Some("we\"ird\nrole"))];
    let text = prom_lines("messages", &rows);
    assert!(text.contains("metric=\"doubles-a\\\\b\""), "{text}");
    assert!(text.contains("subject=\"we\\\"ird\\nrole\""), "{text}");
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        assert!(line.ends_with(" 1") || line.ends_with(" 0"), "every sample stays on one line: {line:?}");
    }
}

#[test]
fn the_gauge_dir_is_the_env_dir_else_the_collector_under_home() {
    assert_eq!(prom_dir(Some("/tmp/g".into()), Some("/Users/x".into())).unwrap(), std::path::PathBuf::from("/tmp/g"));
    assert_eq!(prom_dir(None, Some("/Users/x".into())).unwrap(),
        std::path::PathBuf::from("/Users/x/CascadeProjects/shared-observability/data/textfile_collector"));
}

#[test]
fn negative_proof_no_home_and_no_env_dir_is_an_error_not_a_guess() {
    assert!(prom_dir(None, None).is_err());
    assert!(prom_dir(Some(String::new()), Some(String::new())).is_err(), "empty counts as unset");
    assert!(prom_dir(Some("relative/dir".into()), None).is_err(), "a relative dir lands wherever the run started");
}

#[test]
fn negative_proof_a_hostile_label_never_splits_or_unbalances_a_sample() {
    // Kade's #4679 review: the old version asserted on a literal. This one runs prom_lines:
    // with escaping removed, the newline starts a line that is not a sample, and the bare
    // quote leaves an odd number of unescaped quotes.
    let rows = vec![row("messages", "m\"x", "2026-10-21", "1", Some("a\nb\\"))];
    let text = prom_lines("messages", &rows);
    for line in text.lines() {
        assert!(line.starts_with('#') || line.starts_with("chorus_sensor_reading"), "a label value split the line: {line:?}");
        let unescaped = line.replace("\\\\", "").replace("\\\"", "").matches('"').count();
        assert_eq!(unescaped % 2, 0, "unbalanced quotes: {line:?}");
    }
}
