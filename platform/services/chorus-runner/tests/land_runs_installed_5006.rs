// @test-type: unit
// @domain: services
//! #5006 — a land step runs what is on main. binary_path returned the caller's
//! werk slot ($CHORUS_WERK_BASE/<who>-bin/<name>) before the installed
//! ~/.chorus/bin/<name> whenever the slot existed, so a land step called for a
//! role ran whatever that role last built: unlanded code. On 11-18 athena-land,
//! run by hand, ran Wren's stale slot.
//!
//! Fail closed (Wren 11-19): the installed binary unless the caller opted in
//! with CHORUS_USE_SLOT=1, which only the card's own build/test/demo DAGs set
//! (chorus-make; its own test proves no land DAG does).
//! AC1: no opt-in (a land step, a hand run, the shared runner) → installed, even
//! with a trusted slot present. NEGATIVE PROOF: the same fixture with the opt-in
//! gets the slot, so the check tells the two apart; today's ordering (slot
//! first, no opt-in needed) fails the first case.
//! AC2: a demo/build call that opts in still gets its slot (#4514).
//! AC3: every resolution says which path and why, as one runner.binary.resolved line.
use chorus_runner::server::{binary_path, resolution_line, resolve_binary};
use std::os::unix::fs::PermissionsExt;
use std::sync::Mutex;

// The cases set process env (CHORUS_WERK_BASE, CHORUS_STATE_DIR, CHORUS_USE_SLOT); one at a time.
static ENV: Mutex<()> = Mutex::new(());

struct World {
    slot: String,
    installed: String,
}

fn world(tag: &str) -> World {
    let root = std::env::temp_dir().join(format!("runner-5006-{tag}-{}", std::process::id()));
    let base = root.join("werk");
    let state = root.join("state");
    std::fs::create_dir_all(base.join("wren-bin")).unwrap();
    std::fs::create_dir_all(state.join("bin")).unwrap();
    for f in [base.join("wren-bin/athena-deploy"), state.join("bin/athena-deploy")] {
        std::fs::write(&f, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    // #4853 — a trusted slot: runner-owned, writable only by it
    std::fs::set_permissions(base.join("wren-bin"), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::env::set_var("CHORUS_WERK_BASE", &base);
    std::env::set_var("CHORUS_STATE_DIR", &state);
    World {
        slot: base.join("wren-bin/athena-deploy").display().to_string(),
        installed: state.join("bin/athena-deploy").display().to_string(),
    }
}

#[test]
fn without_the_opt_in_a_trusted_slot_is_never_run() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let w = world("land");
    // a land step, a hand run, the shared runner: none set it; any other value is no opt-in
    for v in [None, Some(""), Some("0"), Some("yes")] {
        match v {
            Some(x) => std::env::set_var("CHORUS_USE_SLOT", x),
            None => std::env::remove_var("CHORUS_USE_SLOT"),
        }
        assert_eq!(binary_path("athena-deploy", "wren").unwrap(), w.installed, "CHORUS_USE_SLOT={v:?} ran the role's slot");
    }
    std::env::remove_var("CHORUS_USE_SLOT");
}

#[test]
fn negative_proof_the_same_fixture_with_the_opt_in_gets_the_slot() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let w = world("demo");
    std::env::set_var("CHORUS_USE_SLOT", "1");
    assert_eq!(binary_path("athena-deploy", "wren").unwrap(), w.slot, "a demo/build call lost the role's slot");
    // still never another role's slot (#4514)
    assert_eq!(binary_path("athena-deploy", "kade").unwrap(), w.installed);
    std::env::remove_var("CHORUS_USE_SLOT");
}

#[test]
fn every_resolution_says_which_path_and_why() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let w = world("why");
    let (slot, why_slot) = resolve_binary("athena-deploy", "wren", true).unwrap();
    let (inst, why_inst) = resolve_binary("athena-deploy", "wren", false).unwrap();
    assert_eq!((slot.as_str(), inst.as_str()), (w.slot.as_str(), w.installed.as_str()));
    for (path, why, opt) in [(&slot, why_slot, true), (&inst, why_inst, false)] {
        let line: serde_json::Value = serde_json::from_str(&resolution_line("athena-deploy", "wren", path, why, opt, "t5006")).unwrap();
        assert_eq!(line["event"], "runner.binary.resolved");
        assert_eq!(line["path"], path.as_str());
        assert_eq!(line["why"], why);
        assert_eq!(line["slot_opt_in"], opt);
        assert_eq!(line["trace"], "t5006");
    }
    assert_ne!(why_slot, why_inst, "the two choices must be told apart in the line");
}
