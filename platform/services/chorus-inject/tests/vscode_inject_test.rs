//! #3130 layer 2 — VS Code inject path. A session in VS Code's integrated
//! terminal is hosted by the "Code" app (Electron), not Terminal.app, and its
//! tty is a pseudo-tty that Terminal can't see — so the Terminal `--tty` match
//! returns no-window-found. The vscode path targets the Code app and types into
//! its focused window (Electron exposes no per-pane/tty selection via AppleScript).

use chorus_inject::{build_inject_vscode_script, dispatch, Dispatch, OsaRunner};
use std::io;
use std::process::Output;

#[test]
fn vscode_script_targets_code_app_not_terminal() {
    let s = build_inject_vscode_script("hello");
    assert!(s.contains(r#"process "Code""#), "must target the Code process");
    assert!(
        !s.contains(r#"application "Terminal""#),
        "must NOT target Terminal.app — that's the no-window-found bug"
    );
}

#[test]
fn vscode_script_keystrokes_and_submits() {
    let s = build_inject_vscode_script("payload");
    assert!(s.contains(r#"keystroke "payload""#), "must type the text");
    assert!(s.contains("key code 36"), "must submit with Return");
}

#[test]
fn vscode_script_activates_and_delivers_with_no_frontmost_guard() {
    // #3439 → REVERTED by #3499. The frontmost-guard (deliver only if Code is
    // already frontmost, else `deferred:not-frontmost`) was a REGRESSION: nudges
    // arriving while Code wasn't the front window silently never landed. The
    // proven delivery is restored: `activate` Code, then keystroke. Jeff's ruling
    // — DELIVER; a focus-blip beats a dead nudge. The guard must NOT come back.
    let s = build_inject_vscode_script("hi");
    assert!(s.contains(r#"tell application "Code" to activate"#), "must activate Code to deliver");
    assert!(!s.contains("frontmost"), "#3499: must NOT guard on frontmost — that dropped delivery");
    assert!(!s.contains("deferred"), "#3499: must NOT defer — the vscode path delivers");
}

struct FakeRunner;
impl OsaRunner for FakeRunner {
    fn run(&self, _script: &str) -> io::Result<Output> {
        use std::os::unix::process::ExitStatusExt;
        Ok(Output { status: std::process::ExitStatus::from_raw(0), stdout: b"ok".to_vec(), stderr: vec![] })
    }
}

#[test]
fn dispatch_vscode_form_routes_to_vscode_inject() {
    let runner = FakeRunner;
    let mut buf: Vec<u8> = Vec::new();
    let args = vec!["--vscode".to_string(), "hello world".to_string()];
    let out = dispatch(&runner, &mut buf, &args, true /* dry_run */);
    assert_eq!(out, Dispatch::Ok);
    let written = String::from_utf8(buf).unwrap();
    assert!(written.to_lowercase().contains("vscode"), "dry-run must name the vscode path: {written:?}");
}

// #3499: the vscode focus-guard/defer path is GONE — chorus-inject delivers
// (activate + keystroke) and returns "ok", or errors. There is no more
// "deferred:not-frontmost" token to surface, so the old DeferRunner test that
// pinned defer-as-clean-outcome is removed with the branch it covered.
