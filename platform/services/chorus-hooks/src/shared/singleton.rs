//! #3631 — PID-reuse-proof hook singleton via flock.
//!
//! The 2026-07-08 outage: a stale `/tmp/chorus-hooks.pid` held a PID the OS had
//! recycled to an unrelated live process; the old `kill(pid,0)==0` liveness
//! check read "holder alive" and exited on every kickstart (14h flap). flock
//! fixes this by construction: the lock is held by the ACTUAL running process
//! and released by the kernel the instant it dies, so the pidfile's *content*
//! is never trusted for liveness.

use std::fs::File;
use std::os::unix::io::AsRawFd;
use std::path::Path;

/// Acquire the exclusive hook-singleton lock on `pidfile_path`. Returns the
/// locked `File` on success — the caller MUST hold it for the process lifetime
/// (dropping it releases the lock). Returns `None` if another live process
/// already holds the lock. A recycled/stale PID written in the file cannot
/// false-block: the lock, not the number, is the liveness signal.
pub fn acquire_singleton(pidfile_path: &str) -> Option<File> {
    if let Some(dir) = Path::new(pidfile_path).parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let f = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        // Never truncate on open: a pidfile being flock'd must keep its contents
        // until the lock is HELD (truncating before the flock races a live holder's
        // pid off disk). Explicit disposition also satisfies suspicious_open_options.
        .truncate(false)
        .open(pidfile_path)
        .ok()?;
    // LOCK_EX | LOCK_NB: exclusive, non-blocking — fail immediately if held.
    let got = unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) == 0 };
    if got {
        Some(f)
    } else {
        None
    }
}
