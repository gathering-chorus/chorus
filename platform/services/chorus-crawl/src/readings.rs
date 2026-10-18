// @domain: heralds — the messages and events sensors (#4634)
//! #4634 — the messages and events sensors count what the team kept, one
//! SensorReading per Boston day, metric and role (Jeff, 2026-10-18: messages
//! and streams are "our most valuable long term data set besides our repo").
//! The first pass counts every past day from the source, so history is the
//! sensor's own measurement, never written by hand. Today is left out: a day
//! is counted once it is over.

use std::collections::BTreeMap;

/// (day, metric, role) → count. role is None for a team total.
pub type Counts = BTreeMap<(String, String, Option<String>), u64>;

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

/// The nth Sunday of a month, as a day number.
fn nth_sunday(y: i64, m: u32, n: i64) -> i64 {
    let first = days_from_civil(y, m, 1);
    let dow = (first + 4).rem_euclid(7); // 1970-01-01 was a Thursday; 0 = Sunday
    first + (7 - dow) % 7 + 7 * (n - 1)
}

/// The Boston calendar day of a UTC time ("2026-10-18 22:05:14" or with a T and
/// fractions/zone). US rule: daylight time from 2am on the second Sunday of
/// March to 2am on the first Sunday of November. None when it can't be read.
pub fn boston_day(utc: &str) -> Option<String> {
    let s = utc.trim();
    let (date, time) = (s.get(0..10)?, s.get(11..19)?);
    let mut dp = date.split('-');
    let (y, m, d): (i64, u32, u32) = (dp.next()?.parse().ok()?, dp.next()?.parse().ok()?, dp.next()?.parse().ok()?);
    let mut tp = time.split(':');
    let (hh, mm, ss): (i64, i64, i64) = (tp.next()?.parse().ok()?, tp.next()?.parse().ok()?, tp.next()?.parse().ok()?);
    let secs = days_from_civil(y, m, d) * 86400 + hh * 3600 + mm * 60 + ss;
    let dst_start = nth_sunday(y, 3, 2) * 86400 + 7 * 3600; // 2am EST = 07:00 UTC
    let dst_end = nth_sunday(y, 11, 1) * 86400 + 6 * 3600; // 2am EDT = 06:00 UTC
    let offset = if secs >= dst_start && secs < dst_end { 4 } else { 5 };
    let (by, bm, bd) = civil_from_days((secs - offset * 3600).div_euclid(86400));
    Some(format!("{by:04}-{bm:02}-{bd:02}"))
}

/// One pulse message row: created_at (UTC), type, from, delivery_status.
pub struct MessageRow<'a> {
    pub created_at: &'a str,
    pub kind: &'a str,
    pub from: &'a str,
    pub status: &'a str,
}

/// Messages per Boston day before `today`: nudges (a role sent it, also per
/// role), alerts (system or pulse sent it), jeff-inputs, and messages-sent
/// (delivered). Matches Kade's reference count (#4634 pair, 18:34).
pub fn count_messages(rows: &[MessageRow], today: &str) -> Counts {
    let mut c = Counts::new();
    let mut add = |day: &str, metric: &str, role: Option<&str>| {
        *c.entry((day.to_string(), metric.to_string(), role.map(str::to_string))).or_insert(0) += 1;
    };
    for r in rows {
        let Some(day) = boston_day(r.created_at) else { continue };
        if day.as_str() >= today {
            continue;
        }
        match r.kind {
            "nudge" if matches!(r.from, "system" | "pulse") => add(&day, "alerts", None),
            "nudge" => {
                add(&day, "nudges", None);
                add(&day, "nudges", Some(r.from));
            }
            "jeff-input" => add(&day, "jeff-inputs", None),
            _ => {}
        }
        if r.status == "delivered" {
            add(&day, "messages-sent", None);
        }
    }
    c
}

fn json_str<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let k = format!("\"{key}\":\"");
    let at = line.find(&k)? + k.len();
    let rest = &line[at..];
    Some(&rest[..rest.find('"')?])
}

/// Spine lines per Boston day before `today`: every event, and each role's
/// commands (agent.action — the streams). A line with no readable time is
/// skipped, never counted on a guessed day.
pub fn count_events<'a>(lines: impl Iterator<Item = &'a str>, today: &str) -> Counts {
    let mut c = Counts::new();
    for line in lines {
        let Some(day) = json_str(line, "timestamp").and_then(boston_day) else { continue };
        if day.as_str() >= today {
            continue;
        }
        *c.entry((day.clone(), "events".into(), None)).or_insert(0) += 1;
        if json_str(line, "event") == Some("agent.action") {
            *c.entry((day.clone(), "commands".into(), None)).or_insert(0) += 1;
            if let Some(role) = json_str(line, "role").filter(|r| !r.is_empty()) {
                *c.entry((day, "commands".into(), Some(role.to_string()))).or_insert(0) += 1;
            }
        }
    }
    c
}

/// A reading's name: <sensor>-<metric>[-<role>]-<day>, so a rerun replaces
/// that day instead of adding a second reading.
pub fn reading_name(sensor: &str, metric: &str, role: Option<&str>, day: &str) -> String {
    match role {
        Some(r) => format!("{sensor}-{metric}-{r}-{day}"),
        None => format!("{sensor}-{metric}-{day}"),
    }
}

/// The SensorReading rows for a set of counts, as (name, fields).
pub fn reading_rows(sensor: &str, counts: &Counts, run: &str) -> Vec<(String, Vec<(String, String)>)> {
    counts
        .iter()
        .map(|((day, metric, role), v)| {
            let mut f = vec![
                ("readingOfSensor".to_string(), sensor.to_string()),
                ("readingDay".to_string(), day.clone()),
                ("readingMetric".to_string(), metric.clone()),
                ("readingValue".to_string(), v.to_string()),
                ("readingUnit".to_string(), "count".to_string()),
                ("readingRun".to_string(), run.to_string()),
            ];
            if let Some(r) = role {
                f.push(("readingRole".to_string(), r.clone()));
            }
            (reading_name(sensor, metric, role.as_deref(), day), f)
        })
        .collect()
}

/// Days between the first and last counted day that have no reading at all:
/// a gap in the record, which the monitor shows and an alert can fire on.
pub fn missing_days(counts: &Counts) -> Vec<String> {
    let days: Vec<&String> = counts.keys().map(|(d, _, _)| d).collect();
    let (Some(first), Some(last)) = (days.iter().min(), days.iter().max()) else { return vec![] };
    let num = |d: &str| -> Option<i64> {
        Some(days_from_civil(d.get(0..4)?.parse().ok()?, d.get(5..7)?.parse().ok()?, d.get(8..10)?.parse().ok()?))
    };
    let (Some(a), Some(b)) = (num(first), num(last)) else { return vec![] };
    let have: std::collections::BTreeSet<&str> = days.iter().map(|d| d.as_str()).collect();
    (a..=b)
        .map(|n| {
            let (y, m, d) = civil_from_days(n);
            format!("{y:04}-{m:02}-{d:02}")
        })
        .filter(|d| !have.contains(d.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_evening_is_the_same_boston_day_and_after_midnight_utc_is_the_day_before() {
        assert_eq!(boston_day("2026-10-18 22:05:14").as_deref(), Some("2026-10-18"));
        assert_eq!(boston_day("2026-10-19T03:30:00.123Z").as_deref(), Some("2026-10-18")); // 23:30 EDT
        assert_eq!(boston_day("2026-01-10 04:59:59").as_deref(), Some("2026-01-09")); // 23:59 EST
        assert_eq!(boston_day("2026-01-10 05:00:00").as_deref(), Some("2026-01-10"));
    }

    #[test]
    fn the_daylight_time_switch_days_land_right() {
        // 2026: DST starts Mar 8, ends Nov 1
        assert_eq!(boston_day("2026-03-08 06:59:59").as_deref(), Some("2026-03-08")); // 01:59 EST
        assert_eq!(boston_day("2026-11-01 04:30:00").as_deref(), Some("2026-11-01")); // 00:30 EDT
        assert_eq!(boston_day("2026-11-02 04:30:00").as_deref(), Some("2026-11-01")); // 23:30 EST
    }

    #[test]
    fn an_unreadable_time_is_none_never_a_guessed_day() {
        assert_eq!(boston_day("yesterday"), None);
        assert_eq!(boston_day(""), None);
    }

    #[test]
    fn messages_split_into_nudges_alerts_jeff_inputs_and_sent_and_today_is_left_out() {
        let rows = [
            MessageRow { created_at: "2026-10-17 14:00:00", kind: "nudge", from: "kade", status: "delivered" },
            MessageRow { created_at: "2026-10-17 15:00:00", kind: "nudge", from: "system", status: "delivered" },
            MessageRow { created_at: "2026-10-17 16:00:00", kind: "jeff-input", from: "jeff", status: "queued" },
            MessageRow { created_at: "2026-10-18 16:00:00", kind: "nudge", from: "kade", status: "delivered" },
        ];
        let c = count_messages(&rows, "2026-10-18");
        let get = |m: &str, r: Option<&str>| c.get(&("2026-10-17".to_string(), m.to_string(), r.map(str::to_string))).copied();
        assert_eq!(get("nudges", None), Some(1));
        assert_eq!(get("nudges", Some("kade")), Some(1));
        assert_eq!(get("alerts", None), Some(1));
        assert_eq!(get("jeff-inputs", None), Some(1));
        assert_eq!(get("messages-sent", None), Some(2));
        assert!(c.keys().all(|(d, _, _)| d != "2026-10-18"), "today must be left to the next pass");
    }

    #[test]
    fn events_count_every_line_and_commands_per_role() {
        let lines = [
            r#"{"event":"agent.action","role":"silas","timestamp":"2026-10-17T14:00:00Z"}"#,
            r#"{"event":"hook.decision","role":"silas","timestamp":"2026-10-17T14:00:01Z"}"#,
            r#"{"event":"agent.action","role":"kade","timestamp":"2026-10-17T15:00:00Z"}"#,
            r#"not json at all"#,
        ];
        let c = count_events(lines.iter().copied(), "2026-10-18");
        let get = |m: &str, r: Option<&str>| c.get(&("2026-10-17".to_string(), m.to_string(), r.map(str::to_string))).copied();
        assert_eq!(get("events", None), Some(3));
        assert_eq!(get("commands", None), Some(2));
        assert_eq!(get("commands", Some("kade")), Some(1));
    }

    #[test]
    fn a_reading_is_named_so_a_rerun_replaces_the_day() {
        assert_eq!(reading_name("messages", "nudges", Some("kade"), "2026-10-17"), "messages-nudges-kade-2026-10-17");
        assert_eq!(reading_name("events", "events", None, "2026-03-29"), "events-events-2026-03-29");
        let mut c = Counts::new();
        c.insert(("2026-10-17".into(), "nudges".into(), Some("kade".into())), 7);
        let rows = reading_rows("messages", &c, "messages-20261018t230000z");
        assert_eq!(rows[0].0, "messages-nudges-kade-2026-10-17");
        assert!(rows[0].1.contains(&("readingValue".to_string(), "7".to_string())));
        assert!(rows[0].1.contains(&("readingRole".to_string(), "kade".to_string())));
    }

    #[test]
    fn negative_proof_a_day_with_nothing_shows_as_missing() {
        let mut c = Counts::new();
        c.insert(("2026-10-15".into(), "events".into(), None), 5);
        c.insert(("2026-10-17".into(), "events".into(), None), 5);
        assert_eq!(missing_days(&c), vec!["2026-10-16".to_string()]);
        c.insert(("2026-10-16".into(), "events".into(), None), 1);
        assert!(missing_days(&c).is_empty());
    }
}
