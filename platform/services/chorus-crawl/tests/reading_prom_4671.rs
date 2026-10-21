// @test-type: unit — hermetic: prom text from given readings, written into a temp dir.
// @domain: alerts
// @card: #4671
// @owner: wren
//! #4671 slice 3 (Kade 10-21 12:55) — a sensor's readings also land as Prometheus
//! gauges (chorus-crawl/src/readings.rs), so a Grafana rule can see them: the
//! SensorReading rows live in Fuseki, which Grafana cannot query, and Loki is the
//! store this morning's load check found overloaded.
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

#[test]
fn the_newest_days_readings_become_one_series_per_sensor_metric_and_subject() {
    let rows = vec![
        row("messages", "messages-sent", "2026-10-19", "500", None),
        row("messages", "messages-sent", "2026-10-20", "696", None),
        row("messages", "messages-kept", "2026-10-20", "655", None),
        row("messages", "doubles-nudge", "2026-10-20", "0", Some("wren")),
    ];
    let text = prom_lines("messages", &rows);
    assert!(text.starts_with("# TYPE chorus_sensor_reading gauge\n"), "{text}");
    assert!(text.contains("chorus_sensor_reading{sensor=\"messages\",metric=\"messages-sent\",subject=\"\"} 696\n"), "{text}");
    assert!(text.contains("chorus_sensor_reading{sensor=\"messages\",metric=\"messages-kept\",subject=\"\"} 655\n"), "{text}");
    assert!(text.contains("chorus_sensor_reading{sensor=\"messages\",metric=\"doubles-nudge\",subject=\"wren\"} 0\n"), "{text}");
    assert!(!text.contains("} 500"), "only the newest day is a gauge, so a rule never reads an old day: {text}");
    assert!(!text.contains("day="), "no day label, so the series count does not grow daily: {text}");
}

#[test]
fn negative_proof_a_value_that_is_not_a_number_is_left_out_not_written_as_zero() {
    let rows = vec![row("messages", "messages-sent", "2026-10-20", "unmeasured", None)];
    let text = prom_lines("messages", &rows);
    assert!(!text.contains("chorus_sensor_reading{"), "{text}");
}

#[test]
fn the_file_is_written_whole_or_not_at_all() {
    let dir = std::env::temp_dir().join(format!("prom-4671-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let rows = vec![row("messages", "messages-sent", "2026-10-20", "696", None)];
    let path = write_prom(&dir, "messages", &rows).unwrap();
    assert_eq!(path, dir.join("sensor_messages.prom"));
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(body.contains("} 696\n"), "{body}");
    let leftovers: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp")).collect();
    assert!(leftovers.is_empty(), "the temp file is moved into place, never left behind");
}
