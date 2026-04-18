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
