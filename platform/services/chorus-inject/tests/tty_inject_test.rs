//! #3125 — tty-matched transport. Routing by tty (exact) instead of by
//! window-title substring (fuzzy). Two failure classes this kills:
//!   - stale same-named shell tab ("wren — -zsh") false-matching the role
//!   - the focus-leak: a keystroke landing in whatever app is FOCUSED rather
//!     than the addressed window (silas's nudge appearing in wren's VS Code pane)
//! The script matches the Terminal TAB whose `tty of t` equals the target and
//! gates delivery on Terminal being the frontmost app — so a nudge can never
//! leak into a different focused application.

use chorus_inject::{build_inject_by_tty_script, dispatch, Dispatch, OsaRunner};
use std::io;
use std::process::Output;

#[test]
fn script_matches_tab_by_tty_not_title() {
    let s = build_inject_by_tty_script("/dev/ttys004", "hello");
    assert!(s.contains("/dev/ttys004"), "must reference the target tty");
    assert!(s.contains("tty of t"), "must match on the tab's tty property");
    // Must NOT fall back to the fragile title-substring match.
    assert!(!s.contains(r#"contains "claude""#), "tty path must not title-match");
}

#[test]
fn script_has_focus_gate_against_leak() {
    let s = build_inject_by_tty_script("/dev/ttys004", "hi");
    // Gate on the frontmost app being Terminal — prevents the keystroke
    // leaking into a different focused app (the observed wren-pane misroute).
    assert!(s.contains("frontmost is true"), "must read the frontmost app");
    assert!(s.contains("focus-gate-miss"), "must refuse (not type) when not frontmost");
}

#[test]
fn script_returns_ok_and_typed_miss() {
    let s = build_inject_by_tty_script("/dev/ttys009", "msg");
    assert!(s.contains(r#"return "ok""#));
    assert!(s.contains("no claude window found for tty /dev/ttys009"));
}

#[test]
fn script_keystrokes_and_submits() {
    let s = build_inject_by_tty_script("/dev/ttys004", "payload");
    assert!(s.contains(r#"keystroke "payload""#));
    assert!(s.contains("key code 36"), "must submit with Return");
    // #2277 invariant preserved: no app-level activate (focus-steal).
    assert!(!s.contains("activate"));
}

// --- dispatch: the --tty arg form routes to the tty path; name-match unchanged ---

struct FakeRunner;
impl OsaRunner for FakeRunner {
    fn run(&self, _script: &str) -> io::Result<Output> {
        // Simulate a successful inject.
        use std::os::unix::process::ExitStatusExt;
        Ok(Output { status: std::process::ExitStatus::from_raw(0), stdout: b"ok".to_vec(), stderr: vec![] })
    }
}

#[test]
fn dispatch_tty_form_dryrun_writes_tty_line() {
    let runner = FakeRunner;
    let mut buf: Vec<u8> = Vec::new();
    let args = vec!["--tty".to_string(), "/dev/ttys004".to_string(), "hello world".to_string()];
    let out = dispatch(&runner, &mut buf, &args, true /* dry_run */);
    assert_eq!(out, Dispatch::Ok);
    let written = String::from_utf8(buf).unwrap();
    assert!(written.contains("/dev/ttys004"), "dry-run must name the tty: {written:?}");
    assert!(written.to_lowercase().contains("tty"));
}

#[test]
fn dispatch_role_form_still_name_matches() {
    // Regression: the legacy `<role> <text>` form must be untouched (as-is path).
    let runner = FakeRunner;
    let mut buf: Vec<u8> = Vec::new();
    let args = vec!["silas".to_string(), "hello".to_string()];
    let out = dispatch(&runner, &mut buf, &args, true);
    assert_eq!(out, Dispatch::Ok);
    let written = String::from_utf8(buf).unwrap();
    assert!(written.contains("pattern=silas"), "legacy role form must still name-match: {written:?}");
}
