// @domain: time — the platform domain this suite guards (#4885)
//! #4885 — two Rust sources of Boston's offset remain on purpose: clock_sync asks the OS (tz
//! database) and shared/boston.rs carries the US rule for the crates with no chrono and no OS call.
//! This holds them to one answer on both change days and either side of them.
mod boston { include!("../../shared/boston.rs"); }

fn os_offset_secs(utc_secs: i64) -> i64 {
    let o = std::process::Command::new("date").args(["-r", &utc_secs.to_string(), "+%z"])
        .env("TZ", "America/New_York").output().unwrap();
    let z = String::from_utf8_lossy(&o.stdout).trim().to_string();
    let sign = if z.starts_with('-') { -1 } else { 1 };
    sign * (z[1..3].parse::<i64>().unwrap() * 3600 + z[3..5].parse::<i64>().unwrap() * 60)
}

fn secs(iso: &str) -> i64 {
    let o = std::process::Command::new("date").args(["-u", "-j", "-f", "%Y-%m-%dT%H:%M:%SZ", iso, "+%s"]).output().unwrap();
    String::from_utf8_lossy(&o.stdout).trim().parse().unwrap()
}

#[test]
fn the_shared_rule_and_the_os_agree_around_both_changes() {
    for iso in ["2026-03-08T06:59:59Z", "2026-03-08T07:00:00Z", "2026-11-01T05:59:59Z", "2026-11-01T06:00:00Z",
                "2026-07-01T12:00:00Z", "2026-01-15T12:00:00Z", "2027-03-14T07:00:00Z", "2027-11-07T05:59:59Z"] {
        let t = secs(iso);
        assert_eq!(boston::offset_secs(t), os_offset_secs(t), "{iso}");
    }
}

#[test]
fn negative_proof_a_rule_a_day_off_disagrees_with_the_os() {
    // the shape of the retired month rules: switching at the start of the change day, not at 2am
    let t = secs("2026-11-01T04:30:00Z"); // 00:30 EDT
    let day_start_rule = -5 * 3600;       // "November 1st is EST"
    assert_ne!(day_start_rule, os_offset_secs(t));
    assert_eq!(boston::offset_secs(t), os_offset_secs(t));
    assert_eq!(boston::day(t), "2026-11-01");
    assert_eq!(boston::offset_iso(t), "2026-11-01T00:30:00-04:00");
}
