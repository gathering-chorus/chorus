//! #4081 — the schedule readings, against plist and launchctl text as Library
//! printed it on 2026-10-11.

use chorus_crawl::schedule::*;
use chorus_crawl::services::parse_json;

#[test]
fn a_daily_calendar_entry_reads_as_daily_with_its_time() {
    let p = parse_json(r#"{"StartCalendarInterval":{"Hour":3,"Minute":0},"StandardOutPath":"/Users/jeffbridwell/Library/Logs/Chorus/nightly-suites.log"}"#).unwrap();
    assert_eq!(calendar_lines(&p), vec!["daily 03:00"]);
    assert_eq!(log_path(&p).as_deref(), Some("/Users/jeffbridwell/Library/Logs/Chorus/nightly-suites.log"));
    assert_eq!(interval_secs(&p), None);
}

#[test]
fn negative_proof_every_calendar_entry_is_kept_not_just_the_first() {
    let p = parse_json(r#"{"StartCalendarInterval":[{"Weekday":1,"Hour":9,"Minute":30},{"Weekday":4,"Hour":9,"Minute":30},{"Minute":15}]}"#).unwrap();
    assert_eq!(calendar_lines(&p), vec!["Mon 09:30", "Thu 09:30", "every hour at :15"]);
}

#[test]
fn an_interval_job_reads_its_seconds() {
    let p = parse_json(r#"{"StartInterval":3600}"#).unwrap();
    assert_eq!(interval_secs(&p), Some(3600));
    assert!(calendar_lines(&p).is_empty());
}

#[test]
fn month_and_day_wildcards_follow_launchd() {
    let p = parse_json(r#"{"StartCalendarInterval":{"Day":1,"Hour":0,"Minute":5}}"#).unwrap();
    assert_eq!(calendar_lines(&p), vec!["day 1 00:05"]);
}

#[test]
fn print_gives_runs_and_never_exited_is_not_exit_zero() {
    let ran = "\tstate = not running\n\truns = 1\n\tlast exit code = 0\n";
    assert_eq!(runs_from_print(ran), Some(1));
    assert!(!never_exited(ran));
    let never = "\tstate = running\n\truns = 1\n\tlast exit code = (never exited)\n";
    assert!(never_exited(never));
    assert_eq!(runs_from_print(""), None, "no print text means not looked at, never zero runs");
}

#[test]
fn print_disabled_names_the_disabled_labels() {
    let text = "\tdisabled services = {\n\t\t\"com.docker.helper\" => enabled\n\t\t\"com.chorus.session-health\" => disabled\n\t\t\"com.logitech.logitune.launcher\" => disabled\n\t}\n";
    assert_eq!(disabled_labels(text), vec!["com.chorus.session-health", "com.logitech.logitune.launcher"]);
}

use chorus_crawl::services::{desired_row, plist_meta, Unit};

fn job(meta: chorus_crawl::services::UnitMeta, exit: Option<i64>) -> Unit {
    Unit { label: "com.chorus.nightly-suites".into(), run_state: "loaded".into(), meta, evidence_unavailable: false, ports: vec![], last_exit_code: exit }
}

#[test]
fn a_job_row_carries_its_schedule_runs_log_and_enabled() {
    let p = parse_json(r#"{"ProgramArguments":["/bin/true"],"StartCalendarInterval":[{"Hour":3,"Minute":0},{"Weekday":0,"Hour":12,"Minute":0}],"StandardOutPath":"/tmp/n.log"}"#).unwrap();
    let mut m = plist_meta(&p);
    m.runs = Some(4);
    m.disabled = Some(false);
    let r = desired_row("library", &job(m, Some(0)), &[], "2026-10-11T20:00:00Z");
    let all = |k: &str| r.fields.iter().filter(|(f, _)| f == k).map(|(_, v)| v.as_str()).collect::<Vec<_>>();
    assert_eq!(all("scheduleKind"), vec!["calendar"]);
    assert_eq!(all("scheduleSpec"), vec!["daily 03:00", "Sun 12:00"]);
    assert_eq!(all("logPath"), vec!["/tmp/n.log"]);
    assert_eq!(all("runCount"), vec!["4"]);
    assert_eq!(all("enabled"), vec!["true"]);
    assert_eq!(all("lastExitCode"), vec!["0"]);
}

#[test]
fn negative_proof_never_exited_writes_no_exit_code_and_unlooked_writes_no_runs() {
    let p = parse_json(r#"{"StartInterval":3600}"#).unwrap();
    let mut m = plist_meta(&p);
    m.never_exited = true;
    let r = desired_row("bedroom", &job(m, Some(0)), &[], "t");
    let has = |k: &str| r.fields.iter().any(|(f, _)| f == k);
    assert!(!has("lastExitCode"), "a job that never finished a run must not read exit 0");
    assert!(!has("runCount") && !has("enabled"), "not looked at is absent, never 0 or false");
    assert!(r.fields.iter().any(|(f, v)| f == "intervalSecs" && v == "3600"));
}
