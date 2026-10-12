// @domain: schedule — what runs when: a timed launchd unit's schedule, read from its plist and launchctl (#4081)
//! #4081 — the schedule domain. Pure readings only: a plist's calendar entries
//! and interval, its log path, and what `launchctl print` / `print-disabled`
//! say about runs and whether the unit is enabled. Every value is read from the
//! machine; none is typed.

use crate::services::Json;

const DAYS: [&str; 8] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

fn num(j: Option<&Json>) -> Option<u64> {
    match j {
        Some(Json::Num(n)) => n.parse().ok(),
        _ => None,
    }
}

/// One StartCalendarInterval entry → a line a person reads. A missing key is a
/// wildcard (launchd's rule): no Hour = every hour, no Minute = minute 0.
pub fn calendar_line(e: &Json) -> String {
    let (month, day, weekday, hour, minute) = (num(e.get("Month")), num(e.get("Day")), num(e.get("Weekday")), num(e.get("Hour")), num(e.get("Minute")));
    let min = minute.unwrap_or(0);
    let time = match hour {
        Some(h) => format!("{h:02}:{min:02}"),
        None => format!("every hour at :{min:02}"),
    };
    let mut when: Vec<String> = Vec::new();
    if let Some(m) = month {
        when.push(format!("month {m}"));
    }
    if let Some(d) = day {
        when.push(format!("day {d}"));
    }
    if let Some(w) = weekday {
        when.push(DAYS.get(w as usize).copied().unwrap_or("?").to_string());
    }
    if when.is_empty() && hour.is_some() {
        when.push("daily".into());
    }
    when.push(time);
    when.join(" ")
}

/// Every calendar entry of a plist, in order. A plist may hold one entry (a
/// dict) or several (an array): all of them are kept, never just the first.
pub fn calendar_lines(plist: &Json) -> Vec<String> {
    match plist.get("StartCalendarInterval") {
        Some(Json::Arr(a)) => a.iter().map(calendar_line).collect(),
        Some(e @ Json::Obj(_)) => vec![calendar_line(e)],
        _ => vec![],
    }
}

pub fn interval_secs(plist: &Json) -> Option<u64> {
    num(plist.get("StartInterval"))
}

pub fn log_path(plist: &Json) -> Option<String> {
    plist.get("StandardOutPath").and_then(Json::as_str).filter(|s| !s.is_empty()).map(String::from)
}

/// `launchctl print`'s run count. None when print said nothing (could not look).
pub fn runs_from_print(text: &str) -> Option<u64> {
    text.lines().find_map(|l| l.trim().strip_prefix("runs = ")).and_then(|v| v.trim().parse().ok())
}

/// "(never exited)" means the job has not finished a run yet — never exit 0.
pub fn never_exited(text: &str) -> bool {
    text.lines().any(|l| l.trim() == "last exit code = (never exited)")
}

/// `launchctl print-disabled gui/<uid>` → the labels it marks disabled. This,
/// not the plist's Disabled key, is what launchd obeys after a manual disable.
pub fn disabled_labels(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let t = l.trim();
            let (label, state) = t.split_once("=>")?;
            let label = label.trim().trim_matches('"');
            (state.trim().starts_with("disabled") || state.trim() == "true").then(|| label.to_string())
        })
        .collect()
}
