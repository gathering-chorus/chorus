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
    // #4634 — an explicit offset wins: the spine writes local time with an offset
    // ("…T01:00:00-0400"); reading that as UTC put every line from 00:00 to 04:00
    // Boston on the day before (23,738 of 10-18's lines, measured). No offset or
    // Z means UTC, as pulse's created_at.
    let rest = s.get(19..).unwrap_or("");
    let rest = rest.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    let off_secs: i64 = match rest.as_bytes().first() {
        Some(b'+') | Some(b'-') => {
            let sign = if rest.starts_with('-') { -1 } else { 1 };
            let digits: String = rest[1..].chars().filter(|c| c.is_ascii_digit()).collect();
            let (oh, om): (i64, i64) = (digits.get(0..2)?.parse().ok()?, digits.get(2..4).unwrap_or("00").parse().ok()?);
            sign * (oh * 3600 + om * 60)
        }
        _ => 0,
    };
    let secs = days_from_civil(y, m, d) * 86400 + hh * 3600 + mm * 60 + ss - off_secs;
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
/// role), alerts (system or pulse sent it) and jeff-inputs. messages-sent and
/// messages-kept come from pulse's Buzz reader (the countKinds rule), so the
/// alert compares like with like (Kade, #4634).
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
    }
    c
}

/// A string field of a one-line JSON object. Spaces around the colon are
/// allowed: a writer that adds one must not drop the line (Kade, #4634).
fn json_str<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let k = format!("\"{key}\"");
    let mut from = 0;
    while let Some(i) = line[from..].find(&k) {
        let rest = line[from + i + k.len()..].trim_start();
        if let Some(rest) = rest.strip_prefix(':') {
            let rest = rest.trim_start().strip_prefix('"')?;
            return Some(&rest[..rest.find('"')?]);
        }
        from += i + k.len();
    }
    None
}

/// Whether a door error means the bearer token must be reminted. An expired
/// token can also come back inside a 422 from the model door (measured
/// 2026-10-19: an hour-long pass lost one write to it), not only as a 401.
pub fn token_expired(err: &str) -> bool {
    err.contains("HTTP 401") || err.contains("identity-token-invalid")
}

/// Whether a line is one whole JSON value. std only (the crate's contract):
/// a small validator, not a parser — it reads nothing out of the line.
pub fn is_json(line: &str) -> bool {
    fn ws(b: &[u8], mut i: usize) -> usize { while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\r' | b'\n') { i += 1 } i }
    fn value(b: &[u8], i: usize, depth: u32) -> Option<usize> {
        if depth > 64 { return None }
        let i = ws(b, i);
        match *b.get(i)? {
            b'{' => {
                let mut i = ws(b, i + 1);
                if b.get(i) == Some(&b'}') { return Some(i + 1) }
                loop {
                    if b.get(i) != Some(&b'"') { return None }
                    i = ws(b, string(b, i)?);
                    if b.get(i) != Some(&b':') { return None }
                    i = ws(b, value(b, i + 1, depth + 1)?);
                    match b.get(i)? { b',' => i = ws(b, i + 1), b'}' => return Some(i + 1), _ => return None }
                }
            }
            b'[' => {
                let mut i = ws(b, i + 1);
                if b.get(i) == Some(&b']') { return Some(i + 1) }
                loop {
                    i = ws(b, value(b, i, depth + 1)?);
                    match b.get(i)? { b',' => i += 1, b']' => return Some(i + 1), _ => return None }
                }
            }
            b'"' => string(b, i),
            b't' => b[i..].starts_with(b"true").then_some(i + 4),
            b'f' => b[i..].starts_with(b"false").then_some(i + 5),
            b'n' => b[i..].starts_with(b"null").then_some(i + 4),
            _ => {
                let mut j = i;
                if b.get(j) == Some(&b'-') { j += 1 }
                let digits = |mut j: usize| { let s = j; while j < b.len() && b[j].is_ascii_digit() { j += 1 } (j > s).then_some(j) };
                j = digits(j)?;
                if b.get(j) == Some(&b'.') { j = digits(j + 1)? }
                if matches!(b.get(j), Some(b'e' | b'E')) { j += 1; if matches!(b.get(j), Some(b'+' | b'-')) { j += 1 } j = digits(j)? }
                Some(j)
            }
        }
    }
    fn string(b: &[u8], mut i: usize) -> Option<usize> {
        i += 1;
        while i < b.len() {
            match b[i] {
                b'"' => return Some(i + 1),
                b'\\' => i += 2,
                c if c < 0x20 => return None,
                _ => i += 1,
            }
        }
        None
    }
    let b = line.as_bytes();
    value(b, 0, 0).map(|i| ws(b, i) == b.len()).unwrap_or(false)
}

/// Spine lines per Boston day before `today`: every event, and each role's
/// commands (agent.action — the streams). A line that is not whole JSON (a
/// torn or interleaved append) is `unreadable-lines`, never an event (Kade,
/// #4634). It is dayed by the time it still carries, or else by the last day
/// read; before any day is known it waits for the next readable line.
pub fn count_events<S: AsRef<str>>(lines: impl Iterator<Item = S>, today: &str) -> Counts {
    let mut c = Counts::new();
    let mut last_day: Option<String> = None;
    let mut pending_unreadable = 0u64;
    for line in lines {
        let line = line.as_ref();
        let day = json_str(line, "timestamp").and_then(boston_day);
        if !is_json(line) {
            match day.or_else(|| last_day.clone()) {
                Some(d) if d.as_str() < today => *c.entry((d, "unreadable-lines".into(), None)).or_insert(0) += 1,
                Some(_) => {}
                None => pending_unreadable += 1,
            }
            continue;
        }
        let Some(day) = day else {
            // whole JSON with no readable time: still not an event on a guessed day
            match &last_day {
                Some(d) if d.as_str() < today => *c.entry((d.clone(), "unreadable-lines".into(), None)).or_insert(0) += 1,
                Some(_) => {}
                None => pending_unreadable += 1,
            }
            continue;
        };
        if pending_unreadable > 0 && day.as_str() < today {
            *c.entry((day.clone(), "unreadable-lines".into(), None)).or_insert(0) += pending_unreadable;
            pending_unreadable = 0;
        }
        last_day = Some(day.clone());
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

/// Every day from a metric's first counted day to the day before `today` gets
/// that metric, 0 when nothing happened: a day that was read is never left
/// blank (Kade, #4634). Before a metric's first day it stays blank: the source
/// did not record that kind yet (commands began 08-21, your messages 06-11), so
/// a 0 there would claim a silence that was never measured.
pub fn fill_zero_days(counts: &mut Counts, metrics: &[&str], today: &str) {
    let num = |d: &str| -> Option<i64> {
        Some(days_from_civil(d.get(0..4)?.parse().ok()?, d.get(5..7)?.parse().ok()?, d.get(8..10)?.parse().ok()?))
    };
    let Some(t) = num(today) else { return };
    for metric in metrics {
        let first = counts.iter().filter(|((_, m, r), v)| m == metric && r.is_none() && **v > 0).map(|((d, _, _), _)| d.clone()).min();
        let Some(a) = first.as_deref().and_then(num) else { continue };
        for n in a..t {
            let (y, m, d) = civil_from_days(n);
            counts.entry((format!("{y:04}-{m:02}-{d:02}"), metric.to_string(), None)).or_insert(0);
        }
    }
}

/// A reading's name: <sensor>-<metric>[-<role>]-<day>, so a rerun replaces
/// that day instead of adding a second reading.
pub fn reading_name(sensor: &str, metric: &str, role: Option<&str>, day: &str) -> String {
    match role {
        Some(r) => format!("{sensor}-{metric}-{r}-{day}"),
        None => format!("{sensor}-{metric}-{day}"),
    }
}

/// The unit a metric is measured in, so an age and a size never mix.
pub fn unit_of(metric: &str) -> &'static str {
    match metric {
        "backup-age" | "spine-write-age" => "hours",
        "backup-bytes" | "store-bytes" => "bytes",
        _ => "count",
    }
}

/// What a sensor read about its store and its backup copy, for one day: the
/// store's size, the copy's size and how old the copy is. A copy that could
/// not be read gives no copy readings (unmeasured), never a zero.
pub fn backup_readings(day: &str, store_bytes: Option<u64>, copy: Option<(u64, u64)>, now_secs: u64) -> Counts {
    let mut c = Counts::new();
    if let Some(b) = store_bytes {
        c.insert((day.to_string(), "store-bytes".into(), None), b);
    }
    if let Some((bytes, mtime)) = copy {
        c.insert((day.to_string(), "backup-bytes".into(), None), bytes);
        c.insert((day.to_string(), "backup-age".into(), None), now_secs.saturating_sub(mtime) / 3600);
    }
    c
}

/// #4630 — where the spine's backup lives. The rsync mirror no longer copies
/// the live spine (a copy of the growing 5 GB file never finished), so the
/// copy to stat is the newest dated slice memory-backup wrote. A path with no
/// `*` (messages.db's mirror, or a test's CHORUS_BACKUP_COPY) is stat'ed as is.
pub const SPINE_SLICES: &str = "/Users/jeffbridwell/Backups/chorus-memory/*/spine-*.jsonl.gz";

/// The shell line that reads a copy's size and mtime: the newest match when
/// `copy` is a glob, the file itself when it is not.
pub fn copy_stat_cmd(copy: &str) -> String {
    if copy.contains('*') {
        format!("stat -f '%z %m' \"$(ls -t {copy} | head -1)\"")
    } else {
        format!("stat -f '%z %m' '{copy}'")
    }
}

/// Days a metric reads 0 although the source is never truly silent: the
/// spine always has events, so events=0 is a gap in the record, not a quiet
/// day (Kade, #4634: 04-26..05-03 read 0, the 05-04 loss).
pub fn zero_days(c: &Counts, metric: &str) -> Vec<String> {
    c.iter().filter(|((_, m, r), v)| m == metric && r.is_none() && **v == 0).map(|((d, _, _), _)| d.clone()).collect()
}

/// A copy is stale when it is more than `max_hours` old (the alert's rule).
pub fn backup_stale(c: &Counts, day: &str, max_hours: u64) -> Option<u64> {
    c.get(&(day.to_string(), "backup-age".to_string(), None)).copied().filter(|h| *h > max_hours)
}

/// The sensor a pass writes its readings as. The 10-minute spine check runs
/// the events pass with --age-only but records as `spine`, so it must also
/// read back its stored rows as `spine`: reading them as `events` finds none,
/// re-creates today's row and the door refuses it (409) on every run after the
/// first (found writing real readings to staging, 2026-10-19).
pub fn reading_sensor(which: &str, age_only: bool) -> &str {
    if age_only { "spine" } else { which }
}

/// The door's filter for a pass's stored readings. An edge is filtered by the
/// target's door name, kind prefix included (`sensor-messages`): a bare
/// `messages` matches nothing, so every run after the first would re-create
/// all its rows and be refused (409), measured on staging 2026-10-19.
pub fn stored_readings_filter(which: &str, age_only: bool) -> String {
    format!("readingOfSensor=sensor-{}", reading_sensor(which, age_only))
}

/// A single decimal reading (an age in hours), outside the day counts: the
/// spine's write age is measured every few minutes and must keep its
/// fraction, so 10 minutes reads 0.17 h, never 0 (Kade, #4634).
pub fn decimal_reading(sensor: &str, metric: &str, day: &str, value: f64, run: &str) -> ReadingRow {
    (
        reading_name(sensor, metric, None, day),
        vec![
            ("readingOfSensor".to_string(), sensor.to_string()),
            ("readingDay".to_string(), day.to_string()),
            ("readingMetric".to_string(), metric.to_string()),
            ("readingValue".to_string(), format!("{value:.2}")),
            ("readingUnit".to_string(), "hours".to_string()),
            ("readingRun".to_string(), run.to_string()),
        ],
    )
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
                ("readingUnit".to_string(), unit_of(metric).to_string()),
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

pub type ReadingRow = (String, Vec<(String, String)>);

/// The Buzz reader's lines ({"day","sent","kept","doubles"}) as messages-sent,
/// messages-kept and doubles-<kind> counts. A line it can't read is skipped.
pub fn buzz_counts(jsonl: &str) -> Counts {
    let mut c = Counts::new();
    for l in jsonl.lines() {
        let num = |k: &str| -> Option<u64> {
            let at = l.find(&format!("\"{k}\":"))? + k.len() + 3;
            l[at..].chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().ok()
        };
        let (Some(day), Some(sent), Some(kept)) = (json_str(l, "day"), num("sent"), num("kept")) else { continue };
        c.insert((day.to_string(), "messages-sent".into(), None), sent);
        c.insert((day.to_string(), "messages-kept".into(), None), kept);
        // #4628 — doubles per kind ({"doubles":{"jeff":N,…}}), one reading each
        if let Some(at) = l.find("\"doubles\":{") {
            let obj = &l[at + 11..];
            let obj = &obj[..obj.find('}').unwrap_or(obj.len())];
            for kind in DOUBLE_KINDS {
                let key = format!("\"{kind}\":");
                let Some(k) = obj.find(&key) else { continue };
                let n: String = obj[k + key.len()..].chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(n) = n.parse() {
                    c.insert((day.to_string(), format!("doubles-{kind}"), None), n);
                }
            }
        }
    }
    c
}

/// #4628 — the kinds the Buzz reader counts doubles for (pulse's DOUBLE_KINDS).
pub const DOUBLE_KINDS: [&str; 5] = ["jeff", "reply", "nudge", "alert", "land"];

/// What a pass must write: readings not yet stored, readings whose value
/// changed (a late-arriving line), and how many already match.
/// A reading whose every write matters, not only its value: the spine's write age.
pub fn is_heartbeat(fields: &[(String, String)]) -> bool {
    fields.iter().any(|(k, v)| k == "readingMetric" && v == "spine-write-age")
}

pub fn plan_readings(desired: Vec<ReadingRow>, stored: &BTreeMap<String, String>) -> (Vec<ReadingRow>, Vec<ReadingRow>, usize) {
    let (mut create, mut update, mut unchanged) = (vec![], vec![], 0);
    for (name, fields) in desired {
        let value = fields.iter().find(|(k, _)| k == "readingValue").map(|(_, v)| v.clone()).unwrap_or_default();
        match stored.get(&name) {
            None => create.push((name, fields)),
            // an age is a heartbeat: rewrite it every pass so its run (the alert's
            // freshness stamp) moves even when the age itself reads 0.00 again —
            // left unchanged, spine-not-growing aged a live sensor as stopped (#4634)
            Some(v) if v.parse::<f64>().ok() == value.parse::<f64>().ok() && !is_heartbeat(&fields) => unchanged += 1,
            Some(_) => update.push((name, fields)),
        }
    }
    (create, update, unchanged)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spine_check_writes_and_reads_back_as_spine_and_the_day_passes_as_themselves() {
        assert_eq!(stored_readings_filter("events", true), "readingOfSensor=sensor-spine");
        assert_eq!(stored_readings_filter("messages", false), "readingOfSensor=sensor-messages");
        assert_eq!(reading_sensor("events", true), "spine");
        assert_eq!(reading_sensor("events", false), "events");
        assert_eq!(reading_sensor("messages", false), "messages");
        let r = decimal_reading(reading_sensor("events", true), "spine-write-age", "2026-10-19", 0.17, "spine-x");
        assert!(r.0.starts_with("spine-spine-write-age-"), "{}", r.0);
        assert!(r.1.contains(&("readingOfSensor".to_string(), reading_sensor("events", true).to_string())));
    }

    #[test]
    fn a_local_time_with_its_offset_is_dated_by_that_offset_not_read_as_utc() {
        // the spine's own form: 01:00 Boston on 10-18 is 10-18, not 10-17
        assert_eq!(boston_day("2026-10-18T01:00:00.123-0400").as_deref(), Some("2026-10-18"));
        assert_eq!(boston_day("2026-10-18T01:00:00-04:00").as_deref(), Some("2026-10-18"));
        assert_eq!(boston_day("2026-10-18T23:59:59-0400").as_deref(), Some("2026-10-18"));
        assert_eq!(boston_day("2026-10-18T05:00:00+0100").as_deref(), Some("2026-10-18")); // 00:00 Boston
        assert_eq!(boston_day("2026-10-18T03:30:00Z").as_deref(), Some("2026-10-17")); // 23:30 the day before
    }

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
        assert_eq!(get("messages-sent", None), None, "sent comes from the Buzz reader, not here");
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
    fn the_spine_age_is_rewritten_every_pass_so_its_run_stays_fresh_other_readings_only_on_change() {
        let mut stored = BTreeMap::new();
        stored.insert("spine-spine-write-age-2026-10-19".to_string(), "0.00".to_string());
        stored.insert("events-events-2026-10-18".to_string(), "5".to_string());
        let age = decimal_reading("spine", "spine-write-age", "2026-10-19", 0.0, "spine-20261019t192713z");
        let mut c = Counts::new();
        c.insert(("2026-10-18".into(), "events".into(), None), 5);
        let mut desired = reading_rows("events", &c, "events-x");
        desired.push(age);
        let (create, update, unchanged) = plan_readings(desired, &stored);
        assert!(create.is_empty());
        assert_eq!(update.len(), 1, "the age is rewritten even at the same value");
        assert_eq!(update[0].0, "spine-spine-write-age-2026-10-19");
        assert_eq!(unchanged, 1, "a day count at the same value is left alone");
    }

    #[test]
    fn an_expired_token_inside_a_422_is_reminted_like_a_401() {
        assert!(token_expired("HTTP 401 unauthorized"));
        assert!(token_expired(r#"HTTP 422 { "error": "validation", "message": "athena-model: identity-token-invalid: Expired" }"#));
        assert!(!token_expired(r#"HTTP 422 { "error": "validation", "message": "off-model property" }"#));
    }

    #[test]
    fn a_torn_line_that_still_carries_a_time_is_unreadable_on_its_day_never_an_event() {
        let lines = [
            r#"{"event":"agent.action","role":"silas","timestamp":"2026-10-17T14:00:00Z"}"#,
            // two appends interleaved: the time is readable, the line is not JSON
            r#"{"event":"membrane.violation","timestamp":"2026-10-17T14:00:01Z","x":{"event":"hook"#,
            r#"cision","role":"kade"}"#,
            r#"{"event":"hook.decision","timestamp":"2026-10-17T14:00:02Z","note":"a \"quoted\" [x]"}"#,
        ];
        let c = count_events(lines.iter().copied(), "2026-10-18");
        let get = |m: &str| c.get(&("2026-10-17".to_string(), m.to_string(), None)).copied();
        assert_eq!(get("events"), Some(2), "only whole lines are events");
        assert_eq!(get("unreadable-lines"), Some(2), "the torn line by its own time, the tail by the last day");
        assert!(is_json(r#"{"a":[1,-2.5e3,true,null,{"b":"c\\d"}]}"#));
        assert!(!is_json(r#"{"a":1}{"b":2}"#), "two values on one line are a torn append");
        assert!(!is_json(r#"{"a":"open"#));
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

    #[test]
    fn a_quiet_day_reads_zero_and_is_not_missing() {
        let mut c = Counts::new();
        c.insert(("2026-10-15".into(), "nudges".into(), None), 3);
        c.insert(("2026-10-17".into(), "nudges".into(), None), 2);
        c.insert(("2026-10-15".into(), "alerts".into(), None), 1);
        fill_zero_days(&mut c, &["nudges", "alerts"], "2026-10-18");
        assert_eq!(c.get(&("2026-10-16".into(), "nudges".into(), None)), Some(&0));
        assert_eq!(c.get(&("2026-10-17".into(), "alerts".into(), None)), Some(&0));
        assert!(missing_days(&c).is_empty());
        assert!(c.keys().all(|(d, _, _)| d.as_str() < "2026-10-18"));
    }

    #[test]
    fn a_spaced_writer_still_counts_and_an_unreadable_line_is_its_own_reading() {
        let lines = [
            r#"{"event" : "agent.action", "role": "kade", "timestamp" : "2026-10-17T14:00:00Z"}"#,
            r#"{"event":"x","when":"2026-10-17T15:00:00Z"}"#,
        ];
        let c = count_events(lines.iter().copied(), "2026-10-18");
        let get = |m: &str, r: Option<&str>| c.get(&("2026-10-17".to_string(), m.to_string(), r.map(str::to_string))).copied();
        assert_eq!(get("commands", Some("kade")), Some(1));
        assert_eq!(get("unreadable-lines", None), Some(1));
        assert_eq!(get("events", None), Some(1));
    }

    #[test]
    fn a_rerun_writes_only_new_and_changed_days() {
        let mut c = Counts::new();
        c.insert(("2026-10-15".into(), "events".into(), None), 4);
        c.insert(("2026-10-16".into(), "events".into(), None), 5);
        c.insert(("2026-10-17".into(), "events".into(), None), 6);
        let mut stored = BTreeMap::new();
        stored.insert("events-events-2026-10-15".to_string(), "4".to_string());
        stored.insert("events-events-2026-10-16".to_string(), "3".to_string());
        let (create, update, unchanged) = plan_readings(reading_rows("events", &c, "r"), &stored);
        assert_eq!(create.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(), ["events-events-2026-10-17"]);
        assert_eq!(update.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(), ["events-events-2026-10-16"]);
        assert_eq!(unchanged, 1);
    }

    #[test]
    fn backup_readings_carry_bytes_and_hours_and_a_26_hour_copy_is_stale() {
        let now = 1_000_000;
        let c = backup_readings("2026-10-18", Some(5_220_000_000), Some((4_400_000_000, now - 26 * 3600)), now);
        assert_eq!(c.get(&("2026-10-18".into(), "backup-age".into(), None)), Some(&26));
        assert_eq!(backup_stale(&c, "2026-10-18", 24), Some(26));
        let rows = reading_rows("events", &c, "r");
        let unit = |n: &str| rows.iter().find(|r| r.0 == n).and_then(|r| r.1.iter().find(|(k, _)| k == "readingUnit")).map(|(_, v)| v.clone());
        assert_eq!(unit("events-backup-age-2026-10-18").as_deref(), Some("hours"));
        assert_eq!(unit("events-backup-bytes-2026-10-18").as_deref(), Some("bytes"));
    }

    #[test]
    fn negative_proof_a_fresh_copy_is_not_stale_and_an_unread_copy_writes_nothing() {
        let now = 1_000_000;
        let fresh = backup_readings("2026-10-18", Some(10), Some((10, now - 2 * 3600)), now);
        assert_eq!(backup_stale(&fresh, "2026-10-18", 24), None);
        let unread = backup_readings("2026-10-18", Some(10), None, now);
        assert!(unread.get(&("2026-10-18".into(), "backup-age".into(), None)).is_none());
        assert!(unread.get(&("2026-10-18".into(), "backup-bytes".into(), None)).is_none());
    }

    #[test]
    fn the_spine_copy_is_the_newest_dated_slice_not_the_retired_mirror() {
        let cmd = copy_stat_cmd(SPINE_SLICES);
        assert!(cmd.contains("ls -t /Users/jeffbridwell/Backups/chorus-memory/*/spine-*.jsonl.gz | head -1"), "{cmd}");
        assert!(!SPINE_SLICES.contains("dot-chorus/.chorus/chorus.log"));
        // negative proof: a plain path is stat'ed as is, never through ls -t
        let plain = copy_stat_cmd("/x/messages.db");
        assert_eq!(plain, "stat -f '%z %m' '/x/messages.db'");
        assert_ne!(plain, cmd);
    }

    #[test]
    fn the_newest_slice_command_reads_the_newest_file_in_a_real_shell() {
        let d = std::env::temp_dir().join(format!("copy-stat-{}", std::process::id()));
        for (day, bytes) in [("2026-10-17", 3), ("2026-10-18", 7)] {
            std::fs::create_dir_all(d.join(day)).unwrap();
            std::fs::write(d.join(day).join(format!("spine-{day}.jsonl.gz")), vec![0u8; bytes]).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(1100));
        }
        let out = std::process::Command::new("sh").arg("-c")
            .arg(copy_stat_cmd(&format!("{}/*/spine-*.jsonl.gz", d.display()))).output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        let _ = std::fs::remove_dir_all(&d);
        // the 7-byte file is the newer one; the older 3-byte one must not be read
        assert!(text.starts_with("7 "), "{text}");
    }

    #[test]
    fn a_zero_event_day_is_a_gap_and_a_busy_day_is_not() {
        let lines = [r#"{"event":"a","timestamp":"2026-10-14T14:00:00Z"}"#, r#"{"event":"a","timestamp":"2026-10-16T14:00:00Z"}"#];
        let mut c = count_events(lines.iter().copied(), "2026-10-17");
        fill_zero_days(&mut c, &["events"], "2026-10-17");
        assert_eq!(zero_days(&c, "events"), vec!["2026-10-15".to_string()]);
    }

    #[test]
    fn the_spine_write_age_keeps_its_fraction() {
        let r = decimal_reading("events", "spine-write-age", "2026-10-18", 600.0 / 3600.0, "run");
        assert_eq!(r.0, "events-spine-write-age-2026-10-18");
        assert!(r.1.contains(&("readingValue".to_string(), "0.17".to_string())));
        assert!(r.1.contains(&("readingUnit".to_string(), "hours".to_string())));
    }

    #[test]
    fn the_reader_doubles_become_one_reading_per_kind_and_a_line_without_them_writes_none() {
        let c = buzz_counts("{\"day\":\"2026-10-18\",\"sent\":985,\"kept\":571,\"doubles\":{\"jeff\":103,\"reply\":530,\"nudge\":0,\"alert\":0,\"land\":0}}\n");
        assert_eq!(c.get(&("2026-10-18".into(), "doubles-jeff".into(), None)), Some(&103));
        assert_eq!(c.get(&("2026-10-18".into(), "doubles-reply".into(), None)), Some(&530));
        assert_eq!(c.get(&("2026-10-18".into(), "doubles-land".into(), None)), Some(&0));
        // negative proof: an older reader's line has no doubles; reading none must not invent zeros
        let old = buzz_counts("{\"day\":\"2026-10-17\",\"sent\":837,\"kept\":0}\n");
        assert!(old.keys().all(|(_, m, _)| !m.starts_with("doubles-")));
    }

    #[test]
    fn the_buzz_reader_lines_become_sent_and_kept() {
        let c = buzz_counts("{\"day\":\"2026-10-17\",\"sent\":837,\"kept\":0}\nnot json\n");
        assert_eq!(c.get(&("2026-10-17".into(), "messages-sent".into(), None)), Some(&837));
        assert_eq!(c.get(&("2026-10-17".into(), "messages-kept".into(), None)), Some(&0));
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn a_metric_is_not_zero_filled_before_the_source_first_recorded_it() {
        let mut c = Counts::new();
        c.insert(("2026-08-19".into(), "events".into(), None), 100);
        c.insert(("2026-08-21".into(), "commands".into(), None), 5);
        c.insert(("2026-08-21".into(), "events".into(), None), 90);
        fill_zero_days(&mut c, &["events", "commands"], "2026-08-23");
        assert!(c.get(&("2026-08-20".into(), "commands".into(), None)).is_none(), "commands did not exist yet on 08-20");
        assert_eq!(c.get(&("2026-08-22".into(), "commands".into(), None)), Some(&0));
        assert_eq!(c.get(&("2026-08-20".into(), "events".into(), None)), Some(&0));
    }
}
