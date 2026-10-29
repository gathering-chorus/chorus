// @domain: spine — placed by chorus-crawl's domain rules (#4538)
//! #4227 — the hooks daemon's run dir decides two things at once: where the
//! control socket lives and where the singleton lock lives. That is why a demo
//! has never had its own hooks — a second daemon on the same path loses the
//! flock and exits. These cover the rule that lets a variant have its own.
use chorus_hooks::shared::state_paths::run_dir_from;

#[test]
fn prod_is_unchanged_when_nothing_overrides_it() {
    assert_eq!(run_dir_from(None, Some("/Users/x")), "/Users/x/.chorus/run");
    assert_eq!(run_dir_from(Some(""), Some("/Users/x")), "/Users/x/.chorus/run");
    assert_eq!(run_dir_from(Some("   "), Some("/Users/x")), "/Users/x/.chorus/run");
}

#[test]
fn hook_run_dir_resolves_deterministic_fallback_when_env_unset() {
    let prev_run_dir = std::env::var("CHORUS_HOOKS_RUN_DIR").ok();
    let prev_home = std::env::var("HOME").ok();
    let prev_state_dir = std::env::var("CHORUS_STATE_DIR").ok();

    // 1. With explicit CHORUS_HOOKS_RUN_DIR
    std::env::set_var("CHORUS_HOOKS_RUN_DIR", "/tmp/custom-run-dir");
    assert_eq!(chorus_hooks::shared::state_paths::hook_run_dir(), "/tmp/custom-run-dir");

    // 2. With CHORUS_STATE_DIR, resolves to state_dir/run-agents
    std::env::remove_var("CHORUS_HOOKS_RUN_DIR");
    std::env::set_var("CHORUS_STATE_DIR", "/tmp/custom-state-dir");
    assert_eq!(chorus_hooks::shared::state_paths::hook_run_dir(), "/tmp/custom-state-dir/run-agents");

    // Restore
    match prev_run_dir {
        Some(v) => std::env::set_var("CHORUS_HOOKS_RUN_DIR", v),
        None => std::env::remove_var("CHORUS_HOOKS_RUN_DIR"),
    }
    match prev_home {
        Some(v) => std::env::set_var("HOME", v),
        None => std::env::remove_var("HOME"),
    }
    match prev_state_dir {
        Some(v) => std::env::set_var("CHORUS_STATE_DIR", v),
        None => std::env::remove_var("CHORUS_STATE_DIR"),
    }
}

#[test]
fn a_variant_gets_its_own_dir_and_therefore_its_own_socket_and_lock() {
    let d = run_dir_from(Some("/w/silas-4227/.chorus-demo/run"), Some("/Users/x"));
    assert_eq!(d, "/w/silas-4227/.chorus-demo/run");
    // Both paths must move together, or the variant takes prod's lock.
    assert!(d != run_dir_from(None, Some("/Users/x")));
}

#[test]
fn a_trailing_slash_does_not_make_a_second_distinct_dir() {
    assert_eq!(
        run_dir_from(Some("/w/run/"), Some("/Users/x")),
        run_dir_from(Some("/w/run"), Some("/Users/x"))
    );
}

#[test]
#[should_panic(expected = "absolute path")]
fn negative_proof_a_relative_override_is_refused_not_resolved() {
    // The failure this guards: a relative dir puts the control socket wherever
    // the daemon was started from. #3631 moved it off world-writable /tmp on
    // purpose; accepting "run" here would hand that back.
    run_dir_from(Some("run"), Some("/Users/x"));
}

#[test]
#[should_panic(expected = "HOME unset")]
fn negative_proof_no_override_and_no_home_still_refuses_to_start() {
    // #3631's rule survives: without an override there is no /tmp fallback.
    run_dir_from(None, None);
}
