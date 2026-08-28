//! #4025 — who killed chorus-hooks?
//!
//! Three SIGTERM deaths in 24h (exit -15 at 13:53, 21:03, 06:50 on 2026-08-27/28),
//! every one during a heavy run, none with a crash report and none with a
//! terminator on record. tokio's `signal()` wakes the shutdown future but throws
//! the `siginfo_t` away, so the daemon could say *that* it was told to die, never
//! *by whom*.
//!
//! This module installs an `SA_SIGINFO` handler for SIGTERM/SIGINT BEFORE tokio
//! registers its own. tokio goes through `signal-hook-registry`, which keeps the
//! previous `sigaction` and chains to it, so both run: ours records `si_pid` /
//! `si_uid` into atomics (async-signal-safe: a store, nothing else), tokio's wakes
//! the graceful-shutdown future, and `main.rs` emits `hooks.terminating` with the
//! sender before exit.
//!
//! What this can NOT see, stated honestly (AC2): SIGKILL is uncatchable — a hard
//! kill leaves no `hooks.terminating` at all. The *absence* of the event after a
//! death is itself the signal: nobody asked nicely. The negative-proof bats
//! (`4025-hooks-terminating-witness.bats`) shows both halves.

use std::sync::atomic::{AtomicI32, Ordering};

static SIGNO: AtomicI32 = AtomicI32::new(0);
static SENDER_PID: AtomicI32 = AtomicI32::new(-1);
static SENDER_UID: AtomicI32 = AtomicI32::new(-1);

/// What the witness saw. `sender_pid == -1` means the handler never fired with
/// siginfo (or the signal never arrived), which is a state the emit names rather
/// than hides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Witness {
    pub signo: i32,
    pub sender_pid: i32,
    pub sender_uid: i32,
}

impl Witness {
    pub fn signal_name(&self) -> &'static str {
        match self.signo {
            libc::SIGTERM => "SIGTERM",
            libc::SIGINT => "SIGINT",
            libc::SIGHUP => "SIGHUP",
            0 => "none",
            _ => "other",
        }
    }
}

extern "C" fn witness(signo: libc::c_int, info: *mut libc::siginfo_t, _ctx: *mut libc::c_void) {
    // Async-signal-safe by construction: three atomic stores, no allocation,
    // no locks, no I/O. The emit happens later on the tokio thread.
    SIGNO.store(signo, Ordering::SeqCst);
    if !info.is_null() {
        // SAFETY: the kernel hands a valid siginfo_t when SA_SIGINFO is set.
        let (pid, uid) = unsafe { ((*info).si_pid, (*info).si_uid) };
        SENDER_PID.store(pid, Ordering::SeqCst);
        SENDER_UID.store(uid as i32, Ordering::SeqCst);
    }
}

/// Install the witness for SIGTERM + SIGINT. Must run before tokio's
/// `signal()` registration (i.e. before the shutdown future is first polled)
/// so signal-hook-registry captures us as the handler to chain to.
pub fn install() {
    for signo in [libc::SIGTERM, libc::SIGINT] {
        // SAFETY: zeroed sigaction is a valid starting point; we set the two
        // fields sigaction() reads for an SA_SIGINFO handler.
        unsafe {
            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = witness as extern "C" fn(libc::c_int, *mut libc::siginfo_t, *mut libc::c_void) as usize;
            sa.sa_flags = libc::SA_SIGINFO | libc::SA_RESTART;
            libc::sigemptyset(&mut sa.sa_mask);
            libc::sigaction(signo, &sa, std::ptr::null_mut());
        }
    }
}

/// Read what the witness recorded (call from the shutdown path).
pub fn observed() -> Witness {
    Witness {
        signo: SIGNO.load(Ordering::SeqCst),
        sender_pid: SENDER_PID.load(Ordering::SeqCst),
        sender_uid: SENDER_UID.load(Ordering::SeqCst),
    }
}

/// Best-effort name for the sender: `ps -o comm=` on the recorded pid. Runs
/// on the normal thread at shutdown, never inside the handler. A sender that
/// already exited (a one-shot `kill`, a finished shell) reads as "gone".
pub fn sender_comm(pid: i32) -> String {
    if pid <= 0 {
        return "unknown".to_string();
    }
    let out = std::process::Command::new("ps")
        .args(["-o", "comm=", "-p", &pid.to_string()])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() { "gone".to_string() } else { s }
        }
        _ => "gone".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_names_cover_the_two_we_install() {
        assert_eq!(Witness { signo: libc::SIGTERM, sender_pid: 1, sender_uid: 0 }.signal_name(), "SIGTERM");
        assert_eq!(Witness { signo: libc::SIGINT, sender_pid: 1, sender_uid: 0 }.signal_name(), "SIGINT");
        assert_eq!(Witness { signo: 0, sender_pid: -1, sender_uid: -1 }.signal_name(), "none");
    }

    #[test]
    fn sender_comm_names_unknown_and_gone_states_distinctly() {
        // -1 = handler never recorded a sender; a huge pid = nobody there.
        assert_eq!(sender_comm(-1), "unknown");
        assert_eq!(sender_comm(i32::MAX - 7), "gone");
        // and a live pid resolves to a real name
        let me = std::process::id() as i32;
        assert_ne!(sender_comm(me), "gone");
        assert_ne!(sender_comm(me), "unknown");
    }
}
