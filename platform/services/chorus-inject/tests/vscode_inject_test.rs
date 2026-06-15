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
fn vscode_script_guards_on_frontmost_and_never_steals_focus() {
    // #3439: the VS Code path must NOT activate (no focus theft) and must refuse
    // to keystroke unless Code is already frontmost — else a mis-routed nudge
    // sprays into whatever window Jeff is in (the 2026-06-15 editor-close bug).
    let s = build_inject_vscode_script("hi");
    assert!(!s.contains("activate"), "must NOT activate — never steal focus (#3439)");
    assert!(s.contains("frontmost"), "must guard on frontmost-app");
    assert!(
        s.contains("deferred:not-frontmost"),
        "must defer (deliver nothing) when Code isn't frontmost"
    );
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
