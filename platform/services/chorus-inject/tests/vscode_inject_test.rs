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
    assert!(s.contains(r#"application "Code""#), "must target the Code app");
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
fn vscode_script_activates_code() {
    // Keystroke lands in the frontmost app, so we must make Code frontmost.
    let s = build_inject_vscode_script("hi");
    assert!(s.contains("activate"), "must activate Code so the keystroke lands in it");
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
