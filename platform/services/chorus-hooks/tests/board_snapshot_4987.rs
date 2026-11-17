// @domain: board
// @test-type: integration — runs the built chorus-hook-shim `pulse` against a stub `cards` CLI in a temp root
// #4987 (AC4): the board snapshot never keeps a landed card
//! #4987 — /tmp/board-wip-snapshot.json froze 2026-11-16 19:41 holding #4766 and was served
//! as the board all night (every role's context read "building #4766"). Two causes: /tmp is
//! sticky, so other accounts' renames over the file failed; and pulse wrote the snapshot only
//! when some column had cards, so the last WIP card stayed after it landed. Now the snapshot
//! sits beside the pulse file and an empty WIP column is written.
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

const SHIM: &str = env!("CARGO_BIN_EXE_chorus-hook-shim");

/// A temp chorus root whose `cards list` prints `listing`, an old snapshot holding #4766,
/// and the pulse file's path beside it. Returns (root, pulse file, snapshot file).
fn world(name: &str, listing: &str) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("board-snap-4987-{}-{name}", std::process::id()));
    let scripts = root.join("platform/scripts");
    fs::create_dir_all(&scripts).unwrap();
    let cards = scripts.join("cards");
    fs::write(&cards, format!("#!/bin/bash\ncat <<'OUT'\n{listing}OUT\n")).unwrap();
    fs::set_permissions(&cards, fs::Permissions::from_mode(0o755)).unwrap();
    let state = root.join("state");
    fs::create_dir_all(&state).unwrap();
    let snap = state.join("board-wip-snapshot.json");
    fs::write(&snap, r#"{"wip_cards":[{"id":4766,"owner":"wren","title":"old","status":"WIP"}],"swat_cards":[],"next_cards":[]}"#).unwrap();
    // older than the 10 s freshness window, so pulse refreshes it
    assert!(Command::new("touch").args(["-t", "202611161941"]).arg(&snap).status().unwrap().success());
    (root, state.join("pulse-latest.json"), snap)
}

fn run_pulse(root: &std::path::Path, pulse: &std::path::Path) {
    let out = Command::new(SHIM).arg("pulse")
        .env("CHORUS_ROOT", root)
        .env("CHORUS_PULSE_PATH", pulse)
        .env("CHORUS_ROLES_URL", "http://127.0.0.1:9/roles")
        .output().expect("shim pulse runs");
    assert!(out.status.success(), "pulse exits 0: {}", String::from_utf8_lossy(&out.stderr));
}

fn wip_ids(snap: &std::path::Path) -> Vec<u64> {
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(snap).unwrap()).unwrap();
    v["wip_cards"].as_array().unwrap().iter().filter_map(|c| c["id"].as_u64()).collect()
}

#[test]
fn a_board_with_no_wip_card_replaces_the_landed_card_in_the_snapshot() {
    let (root, pulse, snap) = world("empty-wip", "Blocked (1):\n  4464  Clean decisions [Wren|P2|domain:decisions]\n\nLater (1):\n  4988  Folded [Abby-normal|P2]\n");
    run_pulse(&root, &pulse);
    assert_eq!(wip_ids(&snap), Vec::<u64>::new(), "the landed #4766 must be gone once the board lists no WIP");
}

/// NEGATIVE PROOF: output that lists no board (an error, nothing) never overwrites the snapshot.
#[test]
fn output_that_lists_no_board_never_overwrites_the_snapshot() {
    let (root, pulse, snap) = world("no-board", "error: board API timed out\n");
    run_pulse(&root, &pulse);
    assert_eq!(wip_ids(&snap), vec![4766]);
}
