//! #3631 — the hook control socket + pidfile must live in the durable run dir
//! (~/.chorus/run, 0700), NOT world-writable /tmp. This is the path half of the
//! 14h-outage fix: /tmp is OS-evicted (the flap) and world-writable (the guard
//! daemon's control socket was 0o777-reachable by any local process).
//!
//! The daemon (main.rs) and shim (shim.rs) MUST resolve the same socket path or
//! they silently stop talking — so both read state_paths::hook_socket_durable().
//! These tests pin that single source of truth.

use chorus_hooks::shared::state_paths::{hook_pid_durable, hook_run_dir, hook_socket_durable};

#[test]
fn socket_is_in_the_run_dir_not_tmp() {
    let s = hook_socket_durable();
    assert!(s.ends_with("/.chorus/run/chorus-hooks.sock"), "socket path was {s}");
    assert!(!s.starts_with("/tmp/"), "socket must NOT be in world-writable /tmp: {s}");
}

#[test]
fn pidfile_is_in_the_run_dir_not_tmp() {
    let p = hook_pid_durable();
    assert!(p.ends_with("/.chorus/run/chorus-hooks.pid"), "pid path was {p}");
    assert!(!p.starts_with("/tmp/"), "pidfile must NOT be in /tmp: {p}");
}

#[test]
fn socket_and_pid_share_one_run_dir() {
    // Both resolve under the same hook_run_dir — the single source of truth the
    // daemon and shim both consult, so they can never drift onto different paths.
    let dir = hook_run_dir();
    assert!(dir.ends_with("/.chorus/run"), "run dir was {dir}");
    assert!(hook_socket_durable().starts_with(&dir));
    assert!(hook_pid_durable().starts_with(&dir));
}
