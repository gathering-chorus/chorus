// @domain: roles
// #4752 — a pair stays open until its card is proven in prod. While cicd and the
// land have the card (phase handed-off) the navigator has nothing to answer, so
// silence is not a stall; a hand-off older than the timeout (a land stranded
// unproven, #4823) ends the pair instead of keeping it registered forever.
use std::fs;
use std::process::Command;

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
}

/// One real tick on a fake CHORUS_HOME with #4752 handed off `ago` seconds
/// before now. (nudges, spine lines, the registry file if still there)
fn tick(phase: &str, ago: u64) -> (String, String, Option<String>) {
    let home = std::env::temp_dir().join(format!("hb-4752-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let scripts = home.join("platform/scripts");
    fs::create_dir_all(&scripts).unwrap();
    fs::create_dir_all(home.join("ops/active-pairs")).unwrap();
    fs::write(home.join("board.json"), r#"{"data":{"cards":[{"id":4752}]}}"#).unwrap();
    let nudges = home.join("nudges.log");
    let spine = home.join("spine.log");
    fs::write(scripts.join("ops-nudge"), format!("echo \"$1|$2\" >> {}\n", nudges.display())).unwrap();
    fs::write(scripts.join("chorus-log"), format!("echo \"$*\" >> {}\n", spine.display())).unwrap();
    let old = now() - 200;
    let handed = now() - ago;
    fs::write(
        home.join("ops/active-pairs/4752.json"),
        format!("{{\"card\":\"4752\",\"navigator\":\"silas\",\"driver\":\"kade\",\"last_seen_iso\":\"2026-10-31T19:00:00-0400\",\"last_active_epoch\":\"{old}\",\"last_level\":\"active\",\"phase\":\"{phase}\",\"handed_off_epoch\":\"{handed}\",\"attempt_run\":\"pair-run-2\",\"handed_off_head\":\"abc123\"}}"),
    )
    .unwrap();
    let obs = home.join("obs");
    fs::create_dir_all(&obs).unwrap();
    fs::write(obs.join("silas-observations.jsonl"), "{\"ts\":\"2026-10-31T19:00:00-0400\",\"role\":\"silas\"}\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_pair-heartbeat"))
        .env("CHORUS_HOME", &home)
        .env("PAIR_OBS_DIR", &obs)
        .env("PAIR_BOARD_URL", format!("file://{}", home.join("board.json").display()))
        .env("WTFU_DAGU", "http://127.0.0.1:9")
        .env("WTFU_LOKI", "http://127.0.0.1:9")
        .env_remove("CHORUS_ROLE")
        .env_remove("PAIR_HANDOFF_TIMEOUT_S")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    (
        fs::read_to_string(&nudges).unwrap_or_default(),
        fs::read_to_string(&spine).unwrap_or_default(),
        fs::read_to_string(home.join("ops/active-pairs/4752.json")).ok(),
    )
}

#[test]
fn a_handed_off_pair_is_not_a_stall_and_keeps_its_phase() {
    let (nudges, spine, reg) = tick("handed-off", 3600);
    assert_eq!(nudges, "", "a navigator with nothing to answer must not be nagged");
    assert!(!spine.contains("pair.navigator.stall"), "{spine}");
    let reg = reg.expect("an hour-old hand-off stays registered");
    assert!(reg.contains("\"phase\":\"handed-off\""), "the tick must keep the phase: {reg}");
    assert!(reg.contains("\"driver\":\"kade\""), "{reg}");
    // AC6 — the re-run check reads these; a tick that dropped them would make every re-run a round
    assert!(reg.contains("\"attempt_run\":\"pair-run-2\"") && reg.contains("\"handed_off_head\":\"abc123\""), "{reg}");
}

/// Negative proof: the same silence while the pair is slicing is still a stall.
#[test]
fn negative_proof_a_slicing_pair_silent_200s_still_stalls() {
    let (nudges, _, reg) = tick("slicing", 0);
    assert!(nudges.starts_with("abby-normal|pair #4752"), "{nudges}");
    assert!(reg.expect("kept").contains("\"phase\":\"slicing\""));
}

/// AC11 (Silas 19:03): a land stranded unproven never keeps the pair forever.
#[test]
fn a_hand_off_older_than_a_day_ends_the_pair_on_the_spine() {
    let (nudges, spine, reg) = tick("handed-off", 90_000);
    assert!(reg.is_none(), "the stranded pair must be deregistered");
    assert!(spine.contains("pair.ended system card=4752 reason=handoff-timeout"), "{spine}");
    assert_eq!(nudges, "", "{nudges}");
}

/// Negative proof: a day-old slicing pair is not ended by the hand-off timeout.
#[test]
fn negative_proof_the_timeout_never_ends_a_slicing_pair() {
    let (_, spine, reg) = tick("slicing", 90_000);
    assert!(reg.is_some(), "a pair waiting on its navigator is the stall's business, not the timeout's");
    assert!(!spine.contains("handoff-timeout"), "{spine}");
}
