// @domain: time — the platform domain this suite guards (#4885, folds #4699)
//! #4885 — every prompt's injected context starts with the system's Boston now, so no agent
//! computes the time it puts in its header (536 hand lookups in 24h, 2026-11-04). Nothing goes to
//! stderr (no hook-error banner, #1559): the line rides the additionalContext envelope.
use chorus_hooks::{build_manifest_envelope, build_manifest_envelope_at, now_line};
use chrono::{DateTime, FixedOffset, TimeZone, Utc};

/// The check a role's header is held to: the envelope's Now line is within 60s of `clock`.
fn now_is_fresh(envelope: &str, clock: DateTime<FixedOffset>) -> bool {
    let Some(line) = envelope.lines().find(|l| l.starts_with("Now: ")) else { return false };
    let t = line.trim_start_matches("Now: ").trim_end_matches(" Boston");
    let Ok(naive) = chrono::NaiveDateTime::parse_from_str(&format!("{t}:00"), "%Y-%m-%d %H:%M:%S") else { return false };
    let Some(at) = clock.offset().from_local_datetime(&naive).single() else { return false };
    (clock - at).num_seconds().abs() < 60 + 59 // the line is to the minute
}

fn est() -> FixedOffset { FixedOffset::west_opt(5 * 3600).unwrap() }
fn edt() -> FixedOffset { FixedOffset::west_opt(4 * 3600).unwrap() }

#[test]
fn the_envelope_starts_with_one_now_line_in_boston() {
    let at = Utc.with_ymd_and_hms(2026, 11, 4, 9, 30, 0).unwrap().with_timezone(&est());
    let env = build_manifest_envelope_at("silas", Some("#4885"), "ok", 1, 1, at);
    let mut lines = env.lines();
    assert!(lines.next().unwrap().starts_with("<chorus-context"));
    assert_eq!(lines.next().unwrap(), "Now: 2026-11-04 04:30 Boston");
    assert_eq!(env.matches("Now: ").count(), 1, "one Now line");
}

#[test]
fn the_live_envelope_carries_now_within_a_minute_of_the_clock() {
    let env = build_manifest_envelope("silas", None, "ok", 0, 0);
    let clock = Utc::now().with_timezone(&chorus_hooks_now_offset());
    assert!(now_is_fresh(&env, clock), "{env}");
}

#[test]
fn negative_proof_a_stale_clock_fails_the_check() {
    let stale = Utc.with_ymd_and_hms(2026, 10, 21, 21, 32, 0).unwrap().with_timezone(&edt());
    let env = build_manifest_envelope_at("wren", None, "ok", 0, 0, stale);
    // Wren's header on 10-21 said 17:32 when it was 19:29
    let real = Utc.with_ymd_and_hms(2026, 10, 21, 23, 29, 0).unwrap().with_timezone(&edt());
    assert!(!now_is_fresh(&env, real));
    assert!(now_is_fresh(&env, stale), "the same envelope is fresh at its own time: the check separates the two");
    // and an envelope with no Now line is never fresh
    assert!(!now_is_fresh("<chorus-context role=\"x\">\nYou are x.", real));
}

#[test]
fn dst_both_sides_render_boston_wall_time() {
    // 2026-11-01 06:30Z is 01:30 EST (after fall-back); 2026-07-01 12:00Z is 08:00 EDT
    assert_eq!(now_line(Utc.with_ymd_and_hms(2026, 11, 1, 6, 30, 0).unwrap().with_timezone(&est())), "Now: 2026-11-01 01:30 Boston");
    assert_eq!(now_line(Utc.with_ymd_and_hms(2026, 7, 1, 12, 0, 0).unwrap().with_timezone(&edt())), "Now: 2026-07-01 08:00 Boston");
}

/// The OS's Boston offset now, the same source the hook uses.
fn chorus_hooks_now_offset() -> FixedOffset {
    let o = std::process::Command::new("date").args(["+%z"]).env("TZ", "America/New_York").output().unwrap();
    let z = String::from_utf8_lossy(&o.stdout).trim().to_string();
    let sign = if z.starts_with('-') { -1 } else { 1 };
    FixedOffset::east_opt(sign * (z[1..3].parse::<i32>().unwrap() * 3600 + z[3..5].parse::<i32>().unwrap() * 60)).unwrap()
}
