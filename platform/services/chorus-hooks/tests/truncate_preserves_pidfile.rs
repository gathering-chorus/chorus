//! #3632 (intent from Silas's retired #3633) — the flock singleton must NEVER
//! zero an existing pidfile on open: truncating before the lock is held races a
//! live holder's pid off disk. Pins `.truncate(false)` so a future
//! `.truncate(true)` (or a removed disposition re-tripping the lint) regresses
//! loudly instead of silently.

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn acquire_singleton_does_not_zero_an_existing_pidfile() {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("sgl-{}-{}", std::process::id(), n));
    fs::create_dir_all(&dir).unwrap();
    let pidfile = dir.join("chorus-hooks.pid");

    // A prior holder's pid is on disk (the daemon-crash state, #3631).
    fs::write(&pidfile, "99999\n").unwrap();

    // Acquiring must leave those bytes intact at open time — the content is only
    // ever REWRITTEN by the caller AFTER the flock is held.
    let lock = chorus_hooks::shared::singleton::acquire_singleton(pidfile.to_str().unwrap());
    assert!(lock.is_some(), "lock acquirable in a fresh dir");
    let content = fs::read_to_string(&pidfile).unwrap();
    assert_eq!(content, "99999\n", "open must not truncate the existing pidfile");

    fs::remove_dir_all(&dir).ok();
}
