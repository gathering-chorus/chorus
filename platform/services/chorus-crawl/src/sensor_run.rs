// @domain: services — picked by its owner, Kade, from the crawler's candidates (#4538)
//! #4405 reopen 4 — a sensor pass as a record (Jeff, 2026-10-12: "make sure
//! our runner is properly aligned integrated and visible as a sensor"). Each
//! pass already prints one tally line; this reads it back into the counts a
//! SensorRun row carries, so a reader can tell a sensor that looked an hour
//! ago and changed nothing from one that has not looked in a week.

/// The passes the herald core has, by Sensor name. `files` is the default pass
/// (no subcommand).
pub const SENSORS: [&str; 8] = ["files", "code", "services", "stores", "infra", "messages", "events", "spine"];

/// What each sensor is, as its Sensor row states it. The rows live in the
/// store (#4432: rows are not replayed from files); a sensor writes its own
/// row the first time it runs, as its owner, so the owner is the row's owner.
pub struct SensorDef {
    pub name: &'static str,
    pub reads: &'static str,
    pub writes: &'static [&'static str],
    pub takes_target: bool,
    pub window_minutes: u32,
}

pub const SENSOR_DEFS: [SensorDef; 8] = [
    SensorDef { name: "files", reads: "git ls-files since the last watermark: files, tests, pages, endpoints, logs", writes: &["CodeFile", "Test", "Page", "Endpoint", "LogSource"], takes_target: false, window_minutes: 1440 },
    SensorDef { name: "code", reads: "Cargo.toml, package.json and Python build files in the target repository", writes: &["CodeUnit", "Repository"], takes_target: true, window_minutes: 1440 },
    SensorDef { name: "services", reads: "launchctl, ps and lsof on Library; ssh to Bedroom; deploy events on the spine", writes: &["ServiceInstance", "ScheduledJob"], takes_target: false, window_minutes: 60 },
    SensorDef { name: "stores", reads: "the store map, Fuseki datasets and the SQLite files under ~/.chorus", writes: &["PersistenceStore"], takes_target: false, window_minutes: 60 },
    SensorDef { name: "infra", reads: "df, scutil, sysctl and sw_vers on Library and Bedroom", writes: &["Machine", "Disk"], takes_target: false, window_minutes: 60 },
    // #4634 — measuring-only sensors: they write SensorReadings, no rows of their own kind
    SensorDef { name: "messages", reads: "pulse's messages.db, read-only: nudges, alerts, Jeff's input and deliveries per Boston day", writes: &[], takes_target: false, window_minutes: 1440 },
    SensorDef { name: "spine", reads: "the spine's last-write time: hours since chorus.log was written, every 10 minutes", writes: &[], takes_target: false, window_minutes: 10 },
    SensorDef { name: "events", reads: "the spine (chorus.log and its archive): events and each role's commands per Boston day", writes: &[], takes_target: false, window_minutes: 1440 },
];

/// A Sensor row's fields, without its name. `sensorWrites` repeats, one per kind.
pub fn sensor_fields(name: &str) -> Option<Vec<(String, String)>> {
    let d = SENSOR_DEFS.iter().find(|d| d.name == name)?;
    let mut f = vec![
        ("label".to_string(), d.name.to_string()),
        ("sensorPass".to_string(), d.name.to_string()),
        ("sensorReads".to_string(), d.reads.to_string()),
        ("takesTarget".to_string(), d.takes_target.to_string()),
        ("sensorWindowMinutes".to_string(), d.window_minutes.to_string()),
    ];
    f.extend(d.writes.iter().map(|k| ("sensorWrites".to_string(), k.to_string())));
    Some(f)
}

/// What one pass planned, from its tally line. None means the line was not
/// there: the pass died before it counted, which is a red, never a zero.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub create: u64,
    pub update: u64,
    pub delete: u64,
    pub unchanged: u64,
}

fn number_after(line: &str, word: &str) -> Option<u64> {
    let at = line.find(&format!("{word} "))? + word.len() + 1;
    let digits: String = line[at..].chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// The tally line each pass prints, read back. The passes word it slightly
/// differently (services says `retire`, stores says `absent`, infra deletes
/// nothing); all of them say `create` and `unchanged`.
pub fn tally(sensor: &str, output: &str) -> Option<Counts> {
    let prefix = format!("{sensor}: ");
    let line = output.lines().rev().find(|l| {
        let l = l.trim_start();
        l.starts_with(&prefix) && l.contains("create ") && l.contains("unchanged ")
    })?;
    Some(Counts {
        create: number_after(line, "create")?,
        update: number_after(line, "update").unwrap_or(0),
        delete: number_after(line, "delete").or_else(|| number_after(line, "retire")).or_else(|| number_after(line, "absent")).unwrap_or(0),
        unchanged: number_after(line, "unchanged")?,
    })
}

/// A run's name: lowercase, because the door slugs names and one capital
/// letter made a row the door created unreadable by the name it was sent as.
/// No kind prefix: the door adds `sensor-run-` itself and refuses a name that
/// already carries it (422 double-prefix, measured on prod after the land).
pub fn run_name(sensor: &str, iso_utc: &str) -> String {
    let stamp: String = iso_utc.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase();
    format!("{sensor}-{stamp}")
}

/// A run name handed in from outside: this sensor's, lowercase letters,
/// digits and single dashes, so the door stores it under the same name.
pub fn is_run_name(name: &str, sensor: &str) -> bool {
    name.starts_with(&format!("{sensor}-"))
        && !name.starts_with("sensor-")
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.contains("--")
        && !name.ends_with('-')
}

/// The fields of a SensorRun row. Edges are written as the target's door
/// name; the door adds the kind prefix (`sensor-`, `principal-`) and refuses
/// one already there (services.rs REF_PREFIXES). A run that has not ended has no end and
/// no counts; a red run keeps whatever it did count.
pub fn run_fields(sensor: &str, target: &str, started: &str, ended: Option<&str>, outcome: &str, counts: Option<Counts>, requested_by: Option<&str>) -> Vec<(String, String)> {
    let mut f = vec![
        ("runOfSensor".to_string(), sensor.to_string()),
        ("runTarget".to_string(), target.to_string()),
        ("startedAt".to_string(), started.to_string()),
        ("sensorOutcome".to_string(), outcome.to_string()),
    ];
    if let Some(e) = ended {
        f.push(("sensorEndedAt".to_string(), e.to_string()));
    }
    if let Some(c) = counts {
        f.push(("plannedCreate".to_string(), c.create.to_string()));
        f.push(("plannedUpdate".to_string(), c.update.to_string()));
        f.push(("plannedDelete".to_string(), c.delete.to_string()));
        f.push(("plannedUnchanged".to_string(), c.unchanged.to_string()));
    }
    if let Some(r) = requested_by {
        f.push(("requestedBy".to_string(), r.to_string()));
    }
    f
}

/// Green only when the pass exited 0 AND counted. An exit 0 with no tally
/// line is a pass that did not do what it says, and reads red.
pub fn outcome(exit: i32, counts: Option<Counts>) -> &'static str {
    if exit == 0 && counts.is_some() {
        "green"
    } else {
        "red"
    }
}

#[cfg(test)]
mod tests_4405 {
    use super::*;

    #[test]
    fn every_pass_tally_line_reads_back() {
        let c = tally("code", "code:   x → y\ncode: create 0 · update 0 · delete 0 · unchanged 50\ncode: dry-run").unwrap();
        assert_eq!(c, Counts { create: 0, update: 0, delete: 0, unchanged: 50 });
        let s = tally("services", "services: 75 rows observed · create 1 · update 2 · retire 3 · unchanged 69 · old #3870 rows still in the graph 0").unwrap();
        assert_eq!(s, Counts { create: 1, update: 2, delete: 3, unchanged: 69 });
        let st = tally("stores", "stores: 12 rows observed · create 0 · update 1 (absent 2) · unchanged 9").unwrap();
        assert_eq!(st, Counts { create: 0, update: 1, delete: 2, unchanged: 9 });
        let i = tally("infra", "infra: create 0 · update 2 · unchanged 31").unwrap();
        assert_eq!(i, Counts { create: 0, update: 2, delete: 0, unchanged: 31 });
    }

    #[test]
    fn negative_proof_a_pass_that_never_counted_is_red_not_zero() {
        assert_eq!(tally("code", "chorus-crawl code: no identity token\n"), None);
        // another sensor's line is not this one's
        assert_eq!(tally("infra", "code: create 0 · update 0 · delete 0 · unchanged 50"), None);
        assert_eq!(outcome(0, None), "red");
        assert_eq!(outcome(1, Some(Counts::default())), "red");
        assert_eq!(outcome(0, Some(Counts::default())), "green");
    }

    #[test]
    fn a_run_name_survives_the_doors_slug() {
        let n = run_name("code", "2026-10-12T21:30:05Z");
        assert_eq!(n, "code-20261012t213005z");
        assert!(!n.starts_with("sensor-run-"), "the door adds the kind prefix and refuses a second one");
        assert_eq!(n, n.to_lowercase());
        assert!(!n.contains("--"));
    }

    #[test]
    fn negative_proof_a_handed_in_run_name_must_be_this_sensors_and_slug_safe() {
        assert!(is_run_name("code-20261012t213005z", "code"));
        assert!(!is_run_name("infra-20261012t213005z", "code"), "another sensor's run");
        assert!(!is_run_name("code-2026T21", "code"), "a capital is stored under another name");
        assert!(!is_run_name("code--x", "code"));
        assert!(!is_run_name("code-x y", "code"));
        // NEGATIVE PROOF (reopen 5): the old prefixed name is the one the door refused
        assert!(!is_run_name("sensor-run-code-20261012t213005z", "code"), "a kind-prefixed name is a 422 at the door");
    }

    #[test]
    fn every_sensor_has_one_row_definition() {
        for s in SENSORS {
            let f = sensor_fields(s).unwrap_or_else(|| panic!("{s} has no Sensor row"));
            // #4634 — measuring sensors write readings only; every other sensor writes rows
            let measuring = matches!(s, "messages" | "events" | "spine");
            assert_eq!(f.iter().any(|(k, _)| k == "sensorWrites"), !measuring, "{s}: sensorWrites must be {}", if measuring { "absent" } else { "present" });
            assert!(f.iter().any(|(k, v)| k == "sensorReads" && !v.is_empty()), "{s} says nothing about what it reads");
        }
        assert_eq!(SENSOR_DEFS.len(), SENSORS.len());
        // NEGATIVE PROOF: a name that is not a sensor has no row
        assert!(sensor_fields("weather").is_none());
        assert_eq!(sensor_fields("code").unwrap().iter().filter(|(k, _)| k == "sensorWrites").count(), 2);
    }

    #[test]
    fn a_running_row_has_no_end_and_no_counts() {
        let f = run_fields("infra", "library", "2026-10-12T21:30:05Z", None, "running", None, Some("silas"));
        assert!(!f.iter().any(|(k, _)| k == "sensorEndedAt" || k.starts_with("planned")), "{f:?}");
        assert!(f.contains(&("runOfSensor".into(), "infra".into())));
        assert!(f.contains(&("requestedBy".into(), "silas".into())));
        assert!(!f.iter().any(|(_, v)| v.starts_with("sensor-") || v.starts_with("principal-")), "the door refuses a prefixed edge: {f:?}");
        let done = run_fields("infra", "library", "s", Some("e"), "green", Some(Counts { update: 2, unchanged: 31, ..Default::default() }), None);
        assert!(done.contains(&("plannedUpdate".into(), "2".into())));
        assert!(done.contains(&("sensorEndedAt".into(), "e".into())));
    }
}
