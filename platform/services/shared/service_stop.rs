// #4446 — a clean stop logged by the daemon itself (service_lifecycle.rs is the
// event half). Separate file because it needs the `libc` crate and
// service_lifecycle.rs stays std-only.

/// Run `on_stop(signal name)` when SIGTERM or SIGINT arrives, then exit 0.
/// For daemons without an async runtime: the signals are blocked in every
/// thread and one thread waits for them with sigwait, so the callback runs as
/// ordinary code, not inside a signal handler. Call it first in main, before
/// any other thread starts (threads inherit the blocked mask).
pub fn on_stop(on_stop: impl FnOnce(&'static str) + Send + 'static) {
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGTERM);
        libc::sigaddset(&mut set, libc::SIGINT);
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
        std::thread::spawn(move || {
            let mut sig: libc::c_int = 0;
            libc::sigwait(&set, &mut sig);
            on_stop(if sig == libc::SIGINT { "SIGINT" } else { "SIGTERM" });
            std::process::exit(0);
        });
    }
}
