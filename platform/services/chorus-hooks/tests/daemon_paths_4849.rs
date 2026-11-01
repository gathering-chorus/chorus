// @domain: services
//! #4849 — a daemon's run dir is never an agent dir. env-setup and werk-test
//! export CHORUS_HOOKS_RUN_DIR=~/.chorus/run-agents for clients; a daemon that
//! took it as its own home clobbered the live agent socket on 11-01.
use chorus_hooks::shared::state_paths::{daemon_run_dir_from, is_agent_dir, run_dir_from};

#[test]
fn a_client_agent_dir_override_does_not_move_the_daemon() {
    assert_eq!(
        daemon_run_dir_from(Some("/Users/x/.chorus/run-agents"), Some("/tmp/h")),
        "/tmp/h/.chorus/run"
    );
    assert_eq!(daemon_run_dir_from(Some("/Users/x/.chorus/run-agents/"), Some("/tmp/h")), "/tmp/h/.chorus/run");
}

#[test]
fn negative_proof_the_client_rule_would_have_moved_it() {
    // The rule clients use still lands on the agent dir — what the daemon used to do.
    assert_eq!(run_dir_from(Some("/Users/x/.chorus/run-agents"), Some("/tmp/h")), "/Users/x/.chorus/run-agents");
}

#[test]
fn the_demo_variant_override_still_moves_the_daemon() {
    // #4227: a card's variant gets its own run dir from one value.
    assert_eq!(daemon_run_dir_from(Some("/tmp/demo-4227/run"), Some("/tmp/h")), "/tmp/demo-4227/run");
    assert_eq!(daemon_run_dir_from(None, Some("/tmp/h")), "/tmp/h/.chorus/run");
}

#[test]
fn agent_dirs_are_named_for_what_they_serve() {
    assert!(is_agent_dir("/Users/x/.chorus/run-agents"));
    assert!(!is_agent_dir("/Users/x/.chorus/run"));
    assert!(!is_agent_dir("/tmp/agents-demo/run"));
}
