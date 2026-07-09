//! #3631 — BEHAVIORAL proof of the flock singleton (Kade's demo catch: the
//! path test alone doesn't prove contention). Two things must hold:
//!   1. while one holder owns the lock, a second acquire fails (real mutual
//!      exclusion — not just a path string);
//!   2. a stale pidfile whose CONTENT is a live-but-unrelated PID does NOT
//!      block acquisition — the exact kill(pid,0) false-positive that flapped
//!      for 14h. flock liveness is the lock, never the pid number in the file.

use chorus_hooks::shared::singleton::acquire_singleton;

fn tmp_pidfile(tag: &str) -> String {
    let dir = std::env::temp_dir().join(format!("chorus-sing-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("chorus-hooks.pid").to_str().unwrap().to_string()
}

#[test]
fn second_holder_is_refused_while_first_holds_then_succeeds_after_release() {
    let pidfile = tmp_pidfile("excl");

    let first = acquire_singleton(&pidfile).expect("first acquires the lock");
    // Second attempt while the first still holds → refused. This is the mutual
    // exclusion the singleton exists for.
    assert!(
        acquire_singleton(&pidfile).is_none(),
        "a second instance must be refused while the first holds the lock"
    );

    drop(first); // releasing the File releases the flock (as process exit would)
    assert!(
        acquire_singleton(&pidfile).is_some(),
        "a fresh acquire must succeed once the prior holder released"
    );
}

#[test]
fn a_recycled_live_pid_in_the_file_does_not_false_block() {
    let pidfile = tmp_pidfile("recyc");
    // Simulate the 2026-07-08 trap: a stale pidfile whose content is a PID that
    // now maps to a LIVE, unrelated process (use our own PID — definitely alive).
    // kill(pid,0) would report "holder alive" and exit-1-flap. flock must ignore
    // the content entirely and acquire cleanly.
    std::fs::write(&pidfile, std::process::id().to_string()).unwrap();

    assert!(
        acquire_singleton(&pidfile).is_some(),
        "a recycled/stale live PID in the file must NOT block flock acquisition"
    );
}
