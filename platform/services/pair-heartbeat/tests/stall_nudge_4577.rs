// @domain: roles
// #4577 — a stalled navigator nudges Abby; an active one nudges nobody.
use pair_heartbeat::{stall_nudge, Level, STALL_TO};
use std::fs;
use std::process::Command;

#[test]
fn a_stall_nudges_abby() {
    let (to, msg) = stall_nudge("4577", "silas", Level::Stall, 185).expect("a stall nudges");
    assert_eq!(to, "abby-normal");
    assert!(msg.contains("#4577") && msg.contains("silas"));
}

#[test]
fn negative_proof_below_a_stall_nobody_is_nudged() {
    for l in [Level::Active, Level::Warn, Level::ReNudge] {
        assert_eq!(stall_nudge("4577", "silas", l, 100), None, "{l:?} must not nudge");
    }
}

/// The real tick against a fake CHORUS_HOME: a silent navigator reaches
/// ops-nudge to Abby once; a navigator with a newer turn reaches nobody.
fn tick(stream_ts: &str, last_seen: &str) -> String {
    let home = tempfile_dir();
    let scripts = home.join("platform/scripts");
    fs::create_dir_all(&scripts).unwrap();
    let log = home.join("nudges.log");
    fs::write(scripts.join("ops-nudge"), format!("echo \"$1|$2\" >> {}\n", log.display())).unwrap();
    fs::write(scripts.join("chorus-log"), "true\n").unwrap();
    fs::create_dir_all(home.join("ops/active-pairs")).unwrap();
    let old = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() - 200;
    fs::write(
        home.join("ops/active-pairs/4577.json"),
        format!("{{\"card\":\"4577\",\"navigator\":\"silas\",\"driver\":\"kade\",\"last_seen_iso\":\"{last_seen}\",\"last_active_epoch\":\"{old}\",\"last_level\":\"active\"}}"),
    ).unwrap();
    let obs = home.join("obs");
    fs::create_dir_all(&obs).unwrap();
    fs::write(obs.join("silas-observations.jsonl"), format!("{{\"ts\":\"{stream_ts}\",\"role\":\"silas\"}}\n")).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_pair-heartbeat"))
        .env("CHORUS_HOME", &home)
        .env("PAIR_OBS_DIR", &obs)
        .env_remove("CHORUS_ROLE")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let entry = fs::read_to_string(home.join("ops/active-pairs/4577.json")).unwrap();
    assert!(entry.contains("\"driver\":\"kade\""), "the tick must keep the driver: {entry}");
    fs::read_to_string(&log).unwrap_or_default()
}

fn tempfile_dir() -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("hb-4577-{}-{}", std::process::id(), rand_suffix()));
    fs::create_dir_all(&d).unwrap();
    d
}

fn rand_suffix() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}

#[test]
fn the_tick_nudges_abby_for_a_silent_navigator() {
    let log = tick("2026-10-17T08:00:00-0400", "2026-10-17T08:00:00-0400");
    assert_eq!(log.lines().count(), 1, "{log}");
    assert!(log.starts_with(&format!("{STALL_TO}|pair #4577")), "{log}");
}

#[test]
fn negative_proof_the_tick_leaves_an_active_navigator_alone() {
    let log = tick("2026-10-17T08:05:00-0400", "2026-10-17T08:00:00-0400");
    assert_eq!(log, "", "an active navigator must not nudge anyone");
}
