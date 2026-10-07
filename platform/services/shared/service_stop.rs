// #4446 — a clean stop logged by the daemon itself (service_lifecycle.rs is the
// event half). Separate file because it calls the C signal API; it declares the
// three calls it needs itself rather than adding the libc crate to each daemon.
// Darwin only, like launchd: sigset_t is a u32 bit mask there.

#[cfg(target_os = "macos")]
mod sys {
    pub type SigSet = u32;
    pub const SIGINT: i32 = 2;
    pub const SIGTERM: i32 = 15;
    pub const SIG_BLOCK: i32 = 1;
    extern "C" {
        pub fn pthread_sigmask(how: i32, set: *const SigSet, old: *mut SigSet) -> i32;
        pub fn sigwait(set: *const SigSet, sig: *mut i32) -> i32;
    }
    /// The mask darwin's sigaddset macro builds: bit (signo - 1).
    pub fn mask(signals: &[i32]) -> SigSet {
        signals.iter().fold(0, |m, s| m | (1 << (s - 1)))
    }
}

/// Run `on_stop(signal name)` when SIGTERM or SIGINT arrives, then exit 0.
/// For daemons without an async runtime: the signals are blocked in every
/// thread and one thread waits for them with sigwait, so the callback runs as
/// ordinary code, not inside a signal handler. Call it first in main, before
/// any other thread starts (threads inherit the blocked mask).
#[cfg(target_os = "macos")]
pub fn on_stop(on_stop: impl FnOnce(&'static str) + Send + 'static) {
    let set = sys::mask(&[sys::SIGTERM, sys::SIGINT]);
    unsafe {
        sys::pthread_sigmask(sys::SIG_BLOCK, &set, std::ptr::null_mut());
    }
    std::thread::spawn(move || {
        let mut sig: i32 = 0;
        unsafe {
            sys::sigwait(&set, &mut sig);
        }
        on_stop(if sig == sys::SIGINT { "SIGINT" } else { "SIGTERM" });
        std::process::exit(0);
    });
}
