// @test-type: unit
// @domain: builds
//! #4925 round 3: the land proves each binary it promotes against what the demo slot held.
//! 11-22 14:34 the round-2 land stopped at deploy-prod: the demo recorded chorus-hooks by
//! its signed shim (40100777) while promote read the werk's target, where a later cargo
//! build had relinked both binaries ad hoc (shim 0f49e399, daemon a40bd4a9). Same code,
//! never the same hash. The fix reads the slot, and the demo now records every binary.
use std::fs;
use werk_deploy::{demo_bin_cdhashes, demo_slot_file, promote_plan_bins, promote_verdict_bins, PromoteVerdict};

const SHIM_SIGNED: &str = "401007778bbb63d68586588ca913c8344ebf7c92";
const SHIM_ADHOC: &str = "0f49e399e3b45ccb6cedb878f7b7044dae84eb2b";
const DAEMON: &str = "a40bd4a9f002da055c1a3459662af8fcb8230d18";
const OTHER: &str = "0123456789abcdef0123456789abcdef01234567";

fn p(c: &str, h: &str) -> (String, String) { (c.to_string(), h.to_string()) }
fn f(b: &str, h: &str) -> (String, Option<String>) { (b.to_string(), Some(h.to_string())) }

#[test]
fn round_twos_record_passes_when_the_files_are_the_demo_slots() {
    // the record round 2's demo wrote (one hash per crate) over the slot's two files
    let demo = [p("chorus-hooks", SHIM_SIGNED)];
    let files = [f("chorus-hooks", DAEMON), f("chorus-hook-shim", SHIM_SIGNED)];
    assert_eq!(promote_verdict_bins(&demo, &[], "chorus-hooks", &files), PromoteVerdict::Ship(SHIM_SIGNED.into()));
}

#[test]
fn negative_proof_the_werk_targets_relinked_files_are_still_refused() {
    // what the 14:34 land read: both ad hoc. Reading them is refused, as it was then.
    let demo = [p("chorus-hooks", SHIM_SIGNED)];
    let files = [f("chorus-hooks", DAEMON), f("chorus-hook-shim", SHIM_ADHOC)];
    assert!(matches!(promote_verdict_bins(&demo, &[], "chorus-hooks", &files), PromoteVerdict::Refuse { .. }));
}

#[test]
fn every_recorded_binary_matching_ships() {
    let demo = [p("chorus-hooks", SHIM_SIGNED)];
    let bins = [p("chorus-hooks/chorus-hooks", DAEMON), p("chorus-hooks/chorus-hook-shim", SHIM_SIGNED)];
    let files = [f("chorus-hooks", DAEMON), f("chorus-hook-shim", SHIM_SIGNED)];
    assert_eq!(promote_verdict_bins(&demo, &bins, "chorus-hooks", &files), PromoteVerdict::Ship(SHIM_SIGNED.into()));
}

#[test]
fn negative_proof_a_changed_daemon_is_refused_though_the_shim_matches() {
    // the crate-level rule ships this (the shim matches); the per-binary record must not
    let demo = [p("chorus-hooks", SHIM_SIGNED)];
    let bins = [p("chorus-hooks/chorus-hooks", DAEMON), p("chorus-hooks/chorus-hook-shim", SHIM_SIGNED)];
    let files = [f("chorus-hooks", OTHER), f("chorus-hook-shim", SHIM_SIGNED)];
    assert_eq!(promote_verdict_bins(&demo, &[], "chorus-hooks", &files), PromoteVerdict::Ship(SHIM_SIGNED.into()),
        "control: without the record the daemon goes unproven");
    assert_eq!(promote_verdict_bins(&demo, &bins, "chorus-hooks", &files),
        PromoteVerdict::Refuse { demo: format!("chorus-hooks {DAEMON}"), file: format!("chorus-hooks {OTHER}") });
    let err = promote_plan_bins(&demo, &bins, &[("chorus-hooks".into(), files.to_vec())]).unwrap_err();
    assert!(err.contains("chorus-hooks") && err.contains(DAEMON) && err.contains(OTHER), "{err}");
}

#[test]
fn negative_proof_a_record_that_names_none_of_the_files_is_refused() {
    let bins = [p("chorus-hooks/some-retired-bin", DAEMON)];
    let files = [f("chorus-hooks", DAEMON)];
    assert!(matches!(promote_verdict_bins(&[], &bins, "chorus-hooks", &files), PromoteVerdict::Refuse { .. }));
}

#[test]
fn unreadable_files_rebuild_and_a_bin_the_slot_never_held_is_not_judged() {
    let bins = [p("chorus-hooks/chorus-hooks", DAEMON)];
    assert_eq!(promote_verdict_bins(&[], &bins, "chorus-hooks", &[("chorus-hooks".into(), None)]), PromoteVerdict::Unreadable);
    let files = [f("chorus-hooks", DAEMON), f("not-in-slot", OTHER)];
    assert_eq!(promote_verdict_bins(&[], &bins, "chorus-hooks", &files), PromoteVerdict::Ship(DAEMON.into()));
}

#[test]
fn the_record_is_the_one_after_the_cards_last_rebuild() {
    let log = [
        r#"{"ts":1,"event":"rebuilt","role":"kade","card_id":4925,"trace_id":"t1","built":"chorus-hooks=x"}"#,
        r#"{"ts":2,"event":"deploy.demo.bins","role":"kade","card_id":4925,"trace_id":"t1","bins":"chorus-hooks/chorus-hooks=old"}"#,
        r#"{"ts":3,"event":"deploy.demo.bins","role":"kade","card_id":49,"trace_id":"t9","bins":"chorus-hooks/chorus-hooks=other-card"}"#,
        r#"{"ts":4,"event":"rebuilt","role":"kade","card_id":4925,"trace_id":"t2","built":"chorus-hooks=y"}"#,
        r#"{"ts":5,"event":"deploy.demo.bins","role":"kade","card_id":4925,"trace_id":"t2","bins":"chorus-hooks/chorus-hooks=new,chorus-hooks/chorus-hook-shim=s"}"#,
    ].join("\n");
    assert_eq!(demo_bin_cdhashes(&log, 4925), vec![p("chorus-hooks/chorus-hooks", "new"), p("chorus-hooks/chorus-hook-shim", "s")]);
    // NEGATIVE PROOF: a rebuild with no record after it (a demo that died before install) leaves none
    let died = format!("{log}\n{}", r#"{"ts":6,"event":"rebuilt","role":"kade","card_id":4925,"trace_id":"t3","built":"chorus-hooks=z"}"#);
    assert!(demo_bin_cdhashes(&died, 4925).is_empty());
    assert!(demo_bin_cdhashes(&log, 492).is_empty(), "card 492 must not read 4925's lines");
}

#[test]
fn the_slot_file_is_the_launchers_mach_o_and_absent_is_none() {
    let d = std::env::temp_dir().join(format!("wd-slot-4925-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("plain"), b"x").unwrap();
    fs::write(d.join("wrapped"), b"#!/bin/sh").unwrap();
    fs::write(d.join("wrapped-bin"), b"x").unwrap();
    assert_eq!(demo_slot_file(&d, "plain"), Some(d.join("plain").to_string_lossy().into_owned()));
    assert_eq!(demo_slot_file(&d, "wrapped"), Some(d.join("wrapped-bin").to_string_lossy().into_owned()));
    assert_eq!(demo_slot_file(&d, "absent"), None);
    let _ = fs::remove_dir_all(&d);
}
