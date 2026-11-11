// @test-type: unit — the per-turn context block that shows a role its open alert cases, against a temp digest dir
// @domain: alerts
// @card: #4957 · owner: silas
//! #4957 AC2 — a role reads its open alert cases in its own context on its next turn,
//! from the digest the 5-minute schedule writes (platform/scripts/alert-digest). Nothing
//! is typed into a pane. A digest with open cases is shown; one with none adds nothing;
//! a digest the schedule has not refreshed is shown with the word stale, never as current.
use chorus_hooks::alert_digest::augment_envelope_with_alert_digest;
use std::time::{Duration, SystemTime};

fn dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("t4957-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn ac2_open_cases_are_shown_in_the_roles_context() {
    let d = dir("open");
    std::fs::write(d.join("silas.md"), "# Open alert cases for silas — 1 (as of 2026-11-11T15:00:00Z)\n- bedroom-disk / bedroom-external: Bedroom external disk at 98% · open since 2026-11-10T06:00:00Z · last seen 2026-11-11T15:00:00Z\n").unwrap();
    let out = augment_envelope_with_alert_digest("silas", "ENVELOPE", &d, SystemTime::now());
    assert!(out.starts_with("ENVELOPE"), "{out}");
    assert!(out.contains("## Open alert cases (1)"), "{out}");
    assert!(out.contains("bedroom-disk / bedroom-external: Bedroom external disk at 98%"), "{out}");
    assert!(!out.contains("stale"), "{out}");
}

#[test]
fn ac2_negative_no_open_cases_and_another_roles_digest_add_nothing() {
    let d = dir("none");
    std::fs::write(d.join("silas.md"), "# Open alert cases for silas — 0 (as of 2026-11-11T15:00:00Z)\n").unwrap();
    std::fs::write(d.join("kade.md"), "# Open alert cases for kade — 1 (as of 2026-11-11T15:00:00Z)\n- crawler-out-of-sync / code: x · open since a · last seen b\n").unwrap();
    assert_eq!(augment_envelope_with_alert_digest("silas", "ENVELOPE", &d, SystemTime::now()), "ENVELOPE");
}

#[test]
fn ac2_a_digest_the_schedule_stopped_refreshing_says_stale() {
    let d = dir("stale");
    std::fs::write(d.join("silas.md"), "# Open alert cases for silas — 1 (as of 2026-11-11T15:00:00Z)\n- a / b: c · open since x · last seen y\n").unwrap();
    // read 20 minutes after it was written: four schedule passes missed
    let later = SystemTime::now() + Duration::from_secs(20 * 60);
    let out = augment_envelope_with_alert_digest("silas", "ENVELOPE", &d, later);
    assert!(out.contains("stale"), "{out}");
    assert!(out.contains("a / b: c"), "the last digest is still shown: {out}");
}

#[test]
fn ac2_a_missing_digest_says_so_rather_than_reading_as_nothing_wrong() {
    let d = dir("missing");
    let out = augment_envelope_with_alert_digest("silas", "ENVELOPE", &d, SystemTime::now());
    assert!(out.contains("no alert digest"), "{out}");
}
