// @domain: time
// #4885 — the one Rust rule for Boston's offset, for the crates with no chrono and no OS call
// (chorus-crawl, werk-test). Include it inside a module:
//     mod boston { include!("../../shared/boston.rs"); }
// US rule since 2007: daylight time from 07:00 UTC on the second Sunday of March to 06:00 UTC on
// the first Sunday of November. chorus-hooks' clock_sync asks the OS (tz database) instead; a test
// there holds the two to the same answer.

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn nth_sunday(y: i64, m: u32, n: i64) -> i64 {
    let first = days_from_civil(y, m, 1);
    let dow = (first + 4).rem_euclid(7); // 1970-01-01 was a Thursday; 0 = Sunday
    first + (7 - dow) % 7 + 7 * (n - 1)
}

/// Boston's UTC offset in seconds at a UTC instant: -14400 (EDT) or -18000 (EST).
#[allow(dead_code)]
pub fn offset_secs(utc_secs: i64) -> i64 {
    let (y, _, _) = civil_from_days(utc_secs.div_euclid(86400));
    let start = nth_sunday(y, 3, 2) * 86400 + 7 * 3600;
    let end = nth_sunday(y, 11, 1) * 86400 + 6 * 3600;
    if utc_secs >= start && utc_secs < end { -4 * 3600 } else { -5 * 3600 }
}

/// The Boston calendar day of a UTC instant: "2026-11-04".
#[allow(dead_code)]
pub fn day(utc_secs: i64) -> String {
    let (y, m, d) = civil_from_days((utc_secs + offset_secs(utc_secs)).div_euclid(86400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Offset-ISO Boston to the second: "2026-11-04T04:26:30-05:00".
#[allow(dead_code)]
pub fn offset_iso(utc_secs: i64) -> String {
    let off = offset_secs(utc_secs);
    let local = utc_secs + off;
    let (y, m, d) = civil_from_days(local.div_euclid(86400));
    let tod = local.rem_euclid(86400);
    let tag = if off == -4 * 3600 { "-04:00" } else { "-05:00" };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}{tag}", tod / 3600, (tod % 3600) / 60, tod % 60)
}
