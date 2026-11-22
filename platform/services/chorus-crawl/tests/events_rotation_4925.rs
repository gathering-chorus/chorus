// @domain: code — placed by chorus-crawl's domain rules (#4538)
//! #4925 row 8 (Silas, Jeff yes 11-22 09:51) — each chorus.log rotation took older days
//! out of the events pass's sources; fill_zero_days gave them 0 and plan_readings UPDATED
//! the stored counts down to 0 (events 11-20, 11-10, 10-15, 08-01 all read 0 now).
use chorus_crawl::readings::{count_events, fill_zero_days, plan_readings, reading_rows, spine_archives, SPINE_ARCHIVE_DIR};
use std::collections::BTreeMap;

fn line(day: &str) -> String {
    format!(r#"{{"event":"agent.action","role":"kade","timestamp":"{day}T12:00:00Z"}}"#)
}

#[test]
fn negative_proof_a_zero_filled_day_never_lowers_a_stored_count() {
    // the live log holds only 11-21; 11-20 is zero-filled
    let lines = vec![line("2026-11-19"), line("2026-11-21"), line("2026-11-21")];
    let mut c = count_events(lines.iter(), "2026-11-22");
    fill_zero_days(&mut c, &["events"], "2026-11-22");
    let desired = reading_rows("events", &c, "r");
    let mut stored = BTreeMap::new();
    stored.insert("events-events-2026-11-20".to_string(), "5000".to_string());
    let (create, update, _) = plan_readings(desired, &stored);
    assert!(!update.iter().any(|(n, _)| n == "events-events-2026-11-20"), "a stored 5000 was rewritten to 0");
    assert!(!create.iter().any(|(n, _)| n == "events-events-2026-11-20"));
    // control: a day with no stored row still gets its 0, and a real count still updates
    assert!(create.iter().any(|(n, _)| n == "events-events-2026-11-21"));
    stored.insert("events-events-2026-11-21".to_string(), "1".to_string());
    let (_, update, _) = plan_readings(reading_rows("events", &c, "r"), &stored);
    assert!(update.iter().any(|(n, _)| n == "events-events-2026-11-21"), "control: a real count no longer updates");
}

#[test]
fn the_rotated_logs_are_sources_oldest_first() {
    let names = vec![
        "chorus.log.20261122T043012.gz".to_string(),
        "chorus.log.20261106T043007.sha256".to_string(),
        "chorus.log.20261025T094956.gz".to_string(),
        "chorus.log.20261106T043007.gz".to_string(),
        "notes.txt".to_string(),
    ];
    let got = spine_archives("/h", &names);
    assert_eq!(got, vec![
        "/h/.chorus/chorus.log.archived-20260723.gz".to_string(),
        format!("/h/{SPINE_ARCHIVE_DIR}/chorus.log.20261025T094956.gz"),
        format!("/h/{SPINE_ARCHIVE_DIR}/chorus.log.20261106T043007.gz"),
        format!("/h/{SPINE_ARCHIVE_DIR}/chorus.log.20261122T043012.gz"),
    ]);
}

#[test]
fn negative_proof_with_the_archive_read_yesterday_counts_its_real_lines_not_zero() {
    // a rotation: the .gz holds 11-20 (3 lines), the live log only 11-21
    let archived = vec![line("2026-11-20"), line("2026-11-20"), line("2026-11-20")];
    let live = vec![line("2026-11-21")];
    let get = |c: &chorus_crawl::readings::Counts| c.get(&("2026-11-20".to_string(), "events".to_string(), None)).copied();
    let mut only_live = count_events(live.iter(), "2026-11-22");
    only_live.insert(("2026-11-19".into(), "events".into(), None), 1);
    fill_zero_days(&mut only_live, &["events"], "2026-11-22");
    assert_eq!(get(&only_live), Some(0), "the old source set: the rotated day reads 0");
    let mut both = count_events(archived.iter().chain(live.iter()), "2026-11-22");
    fill_zero_days(&mut both, &["events"], "2026-11-22");
    assert_eq!(get(&both), Some(3), "with the archive read, the day counts its lines");
}
