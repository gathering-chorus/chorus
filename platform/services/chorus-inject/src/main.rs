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
//!
//! Caller-gate (#2804): chorus-inject is the pulse worker's delivery
//! primitive. Direct invocation is refused unless `_NUDGE_PULSE_INTERNAL=1`
//! is set — the canonical-caller marker that pulse's spawn passes through.
//! Tests + build-signed.sh verify use the same env. One canonical path,
//! no dev-only escape hatch.

use std::io;
use std::process::ExitCode;

use chorus_inject::{dispatch, Dispatch, RealOsaRunner};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if std::env::var("_NUDGE_PULSE_INTERNAL").is_err() {
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
