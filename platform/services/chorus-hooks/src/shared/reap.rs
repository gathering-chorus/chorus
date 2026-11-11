//! #4951 — start a child we do not wait for in line, and still reap it.
//!
//! A long-lived daemon that spawns a helper and drops the `Child` leaves a zombie
//! for every helper that exits: on 2026-11-10 chorus-hooks held 2,285 of them,
//! jeffbridwell reached kern.maxprocperuid, and every fork as Jeff failed. The
//! Child is handed to a detached thread that waits on it, so the process table
//! entry is released as soon as the helper exits. Use this for every
//! fire-and-forget spawn; the reaps_children_4951 guard refuses a dropped Child.
use std::process::Command;

/// Spawn `cmd` and reap it in the background. Returns the child's pid.
pub fn spawn_reaped(cmd: &mut Command) -> std::io::Result<u32> {
    let mut child = cmd.spawn()?;
    let pid = child.id();
    std::thread::Builder::new()
        .name(format!("reap-{pid}"))
        .spawn(move || { let _ = child.wait(); })?;
    Ok(pid)
}
