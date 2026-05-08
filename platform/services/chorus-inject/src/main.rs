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

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::process::ExitCode;

use chorus_inject::{dispatch, Dispatch, RealOsaRunner};

// #2804 — caller-gate threat model (Kade gemba 2026-05-08).
//
// This is a dev-discipline gate, not adversarial control. The threat is
// agents reaching past pulse worker to call chorus-inject directly, breaking
// the MCP-canonical contract. CHORUS_INJECT_BYPASS_GATE=1 exists because the
// legitimate cases — tests, build-signed.sh verify, TCC re-grant rituals —
// need predictable bypass. A rotating-token would not raise the bar against
// a motivated bypass (agents have full shell); it would only add friction
// to the legit cases. The gate's value is making the discipline VISIBLE —
// caller consciously sets the env. Same shape as _GIT_QUEUE_PUSH=1 in
// pre-push: marker is intent, not authorization.
//
// Legitimate bypass call sites (keep current):
//   - platform/scripts/build-signed.sh  (binary verify after sign+install)
//   - platform/services/chorus-inject/tests/inject_integration.rs  (test runner)
//   - manual TCC re-grant rituals (operator-initiated)
//
// Bypass invocations emit a spine event so illegitimate use surfaces in the
// data instead of via incident.
fn emit_bypass_spine_event(args: &[String]) {
    let chorus_log_path = std::env::var("CHORUS_LOG_FILE")
        .or_else(|_| {
            std::env::var("HOME").map(|h| format!("{}/.chorus/chorus.log", h))
        })
        .unwrap_or_else(|_| "/Users/jeffbridwell/.chorus/chorus.log".to_string());
    // RFC3339-ish timestamp via std::time (no chrono dep — Kade gemba: minimal change).
    let ts_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let ts = format!("{}-epoch-secs", ts_secs);
    let argv_summary: String = args.iter().take(2).cloned().collect::<Vec<_>>().join(" ");
    let line = format!(
        r#"{{"timestamp":"{}","event":"chorus_inject.bypass_invoked","role":"chorus-inject","argv":"{}"}}"#,
        ts,
        argv_summary.replace('"', "'"),
    );
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&chorus_log_path) {
        let _ = writeln!(f, "{}", line);
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

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
    if bypass && !pulse_internal {
        emit_bypass_spine_event(&args);
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
