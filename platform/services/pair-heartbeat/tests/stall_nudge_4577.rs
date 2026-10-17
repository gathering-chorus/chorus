// @domain: roles
// #4577 — a stalled navigator nudges Abby; an active one nudges nobody.
use pair_heartbeat::{level_nudge, wip_ids, Level, STALL_TO};
use std::fs;
use std::process::Command;

#[test]
fn a_stall_nudges_abby() {
    let (to, msg) = level_nudge("4577", "silas", "kade", Level::Stall, 185).expect("a stall nudges");
    assert_eq!(to, "abby-normal");
    assert!(msg.contains("#4577") && msg.contains("silas"));
}

#[test]
fn negative_proof_below_120s_nobody_is_nudged() {
    for l in [Level::Active, Level::Warn] {
        assert_eq!(level_nudge("4577", "silas", "kade", l, 100), None, "{l:?} must not nudge");
    }
}

/// Abby 1 (10-17): at 120s the navigator hears it first, naming the driver.
#[test]
fn at_120s_the_navigator_is_nudged_first() {
    let (to, msg) = level_nudge("4577", "silas", "kade", Level::ReNudge, 125).expect("120s nudges");
    assert_eq!(to, "silas");
    assert!(msg.contains("kade is waiting"), "{msg}");
}

#[test]
fn wip_ids_reads_the_board_cards() {
    let j = r#"{"data":{"total":2,"cards":[{"id":4576,"owner":"Wren"},{"id":4577,"owner":"Kade"}]}}"#;
    assert_eq!(wip_ids(j), vec!["4576", "4577"]);
}

/// The real tick against a fake CHORUS_HOME: a silent navigator reaches
/// ops-nudge to Abby once; a navigator with a newer turn reaches nobody.
fn tick(stream_ts: &str, last_seen: &str) -> String {
    tick_on_board(stream_ts, last_seen, r#"{"data":{"cards":[{"id":4577}]}}"#).0
}

/// (nudges, whether the pair's registry file is still there)
fn tick_on_board(stream_ts: &str, last_seen: &str, board: &str) -> (String, bool) {
    let home = tempfile_dir();
    fs::write(home.join("board.json"), board).unwrap();
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
        .env("PAIR_BOARD_URL", format!("file://{}", home.join("board.json").display()))
        .env_remove("CHORUS_ROLE")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let reg = home.join("ops/active-pairs/4577.json");
    if let Ok(entry) = fs::read_to_string(&reg) {
        assert!(entry.contains("\"driver\":\"kade\""), "the tick must keep the driver: {entry}");
    }
    (fs::read_to_string(&log).unwrap_or_default(), reg.exists())
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

/// Abby 4 (10-17 09:24): a pair whose card left WIP is dropped, not nudged.
#[test]
fn a_pair_on_a_card_not_in_wip_is_dropped_and_nobody_is_nudged() {
    let (log, kept) = tick_on_board("2026-10-17T08:00:00-0400", "2026-10-17T08:00:00-0400", r#"{"data":{"cards":[{"id":4576}]}}"#);
    assert_eq!(log, "", "{log}");
    assert!(!kept, "the registry file must be removed");
}

/// Negative proof: an unreadable board drops nothing (missing data is not "not in WIP").
#[test]
fn negative_proof_an_unreadable_board_drops_nothing() {
    let (log, kept) = tick_on_board("2026-10-17T08:00:00-0400", "2026-10-17T08:00:00-0400", "not json");
    assert!(kept, "an unreadable board must not drop a pair");
    assert!(log.starts_with("abby-normal|pair #4577"), "{log}");
}
