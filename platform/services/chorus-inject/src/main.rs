//! chorus-inject — Stable binary for all osascript operations.
//!
//! Separate binary from chorus-hook-shim so that TCC (Accessibility +
//! Automation) permissions survive shim rebuilds. ALL osascript goes through
//! here — one binary, one grant.
//!
//! Usage:
//!   chorus-inject <role> <text>              Inject text into role's Terminal window
//!   chorus-inject --count-windows <pattern>  Count Terminal windows matching "<pattern> + claude"
//!                                            Stdout: <count>::<first-matching-window-name>
//!
//! #2167 moved all logic into the library crate (`chorus_inject`). This binary
//! is env-gate + argv collection + Dispatch-to-ExitCode mapping. Kept small
//! enough that it doesn't need its own test surface.

use std::io;
use std::process::ExitCode;

use chorus_inject::{dispatch, Dispatch, RealOsaRunner};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // #2804 — chorus-inject is the pulse worker's delivery primitive. Reject
    // calls that didn't come from the worker. The worker sets
    // _NUDGE_PULSE_INTERNAL=1 on its child process env. --count-windows is
    // the smoke probe (called from pulse boot for startup smoke) and also
    // gets the env. Anyone running chorus-inject directly from a shell gets
    // a clear "use the MCP tool" error. CHORUS_INJECT_BYPASS_GATE=1 escape
    // hatch for tests / signing rituals / TCC re-grant flows.
    let pulse_internal = std::env::var("_NUDGE_PULSE_INTERNAL").is_ok();
    let bypass = std::env::var("CHORUS_INJECT_BYPASS_GATE").is_ok();
    if !pulse_internal && !bypass {
        eprintln!(
            "chorus-inject: not-canonical-caller. \
             This binary is the pulse worker's delivery primitive — direct \
             invocation is no longer supported. To send a nudge, use the \
             chorus_nudge_message MCP tool from a Claude session."
        );
        return ExitCode::from(2);
    }

    let dry_run = std::env::var("CHORUS_INJECT_DRY_RUN").is_ok();
    let runner = RealOsaRunner;
    let mut stdout = io::stdout().lock();

    match dispatch(&runner, &mut stdout, &args, dry_run) {
        Dispatch::PrintOut(s) => {
            println!("{}", s);
            ExitCode::SUCCESS
        }
        Dispatch::Ok => ExitCode::SUCCESS,
        Dispatch::Err(msg) => {
            eprintln!("{}", msg);
            ExitCode::from(1)
        }
    }
}
