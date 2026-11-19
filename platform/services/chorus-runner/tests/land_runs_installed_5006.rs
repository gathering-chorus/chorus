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
//! AC3 (the runner.binary.resolved line) is binary_resolved_line_5006.rs.
use chorus_runner::server::binary_path;
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

/// Wren 11-19: an opt-in is read from the env of the process that resolves the
/// binary, so a call forwarded to the shared runner (:3342) drops it and gets
/// the installed verb. That is deliberate: the forward carries no opt-in, so a
/// role session cannot claim one on the shared runner. Every card/build/demo
/// step runs under dagu as the installer's account (dagu start-all is
/// jeffbridwell's, the owner of ~/.chorus/bin), and the installer's own call
/// never forwards: it resolves in its own process, where the DAG's opt-in is.
/// NEGATIVE PROOF: any other account's call does forward.
#[test]
fn a_pipeline_steps_call_resolves_in_process_so_its_opt_in_holds() {
    use chorus_runner::forward::forwards;
    let installer = 501;
    assert!(!forwards(installer, Some(installer)), "the installer's own call forwarded: a card DAG's opt-in would be lost");
    assert!(forwards(503, Some(installer)), "another account's call ran in process");
    assert!(!forwards(503, None), "no known installer: nothing forwards (the caller fails closed on its own)");
}
