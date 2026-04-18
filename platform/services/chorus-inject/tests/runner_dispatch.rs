//! Unit-style tests for `chorus_inject::dispatch` + `inject` + `count_windows`.
//!
//! These exist because main.rs used to spawn `osascript` directly — the old
//! framing was "bin wrapper uncoverable without real Accessibility." That
//! conflated "requires macOS permission to run for real" with "cannot be unit
//! tested." Post-#2167 the impure path routes through an `OsaRunner` trait so
//! a `FakeRunner` exercises every branch: unknown role, dry-run short-circuit,
//! spawn failure, ok stdout, non-ok stdout, argv dispatch, multiword text join.
//!
//! AC (#2167): aggregate 80% across chorus. chorus-inject's gap was main.rs.
//! Moving logic into the library with a runner seam closes that gap honestly
//! — the "structural exception" escape hatch is retired.

use chorus_inject::{count_windows, dispatch, inject, Dispatch, OsaRunner, RealOsaRunner};
use std::cell::RefCell;
use std::io;
use std::os::unix::process::ExitStatusExt;
use std::process::{ExitStatus, Output};

struct FakeRunner {
    result: RefCell<Option<io::Result<Output>>>,
    seen: RefCell<Option<String>>,
}

impl FakeRunner {
    fn ok(stdout: &str, stderr: &str) -> Self {
        FakeRunner {
            result: RefCell::new(Some(Ok(Output {
                status: ExitStatus::from_raw(0),
                stdout: stdout.as_bytes().to_vec(),
                stderr: stderr.as_bytes().to_vec(),
            }))),
            seen: RefCell::new(None),
        }
    }

    fn spawn_error() -> Self {
        FakeRunner {
            result: RefCell::new(Some(Err(io::Error::new(
                io::ErrorKind::NotFound,
                "osascript not found",
            )))),
            seen: RefCell::new(None),
        }
    }

    fn never() -> Self {
        FakeRunner {
            result: RefCell::new(None),
            seen: RefCell::new(None),
        }
    }
}

impl OsaRunner for FakeRunner {
    fn run(&self, script: &str) -> io::Result<Output> {
        *self.seen.borrow_mut() = Some(script.to_string());
        self.result
            .borrow_mut()
            .take()
            .expect("FakeRunner invoked unexpectedly (or twice)")
    }
}

fn svec(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

// --- inject ---

#[test]
fn inject_unknown_role_errors_without_invoking_runner() {
    let runner = FakeRunner::never();
    let mut out = Vec::new();
    let err = inject(&runner, &mut out, "nobody", "hi", false).unwrap_err();
    assert!(err.contains("unknown role"), "got: {}", err);
    assert!(err.contains("nobody"));
    assert!(runner.seen.borrow().is_none(), "runner must not be called");
}

#[test]
fn inject_dry_run_prints_and_skips_runner() {
    let runner = FakeRunner::never();
    let mut out = Vec::new();
    inject(&runner, &mut out, "silas", "hello", true).expect("dry-run ok");
    let printed = String::from_utf8(out).unwrap();
    assert!(printed.contains("DRY-RUN inject role=silas pattern=silas"));
    assert!(printed.contains("escaped=hello"));
    assert!(runner.seen.borrow().is_none());
}

#[test]
fn inject_dry_run_applies_escaping_to_printed_text() {
    let runner = FakeRunner::never();
    let mut out = Vec::new();
    inject(&runner, &mut out, "kade", "em\u{2014}dash", true).unwrap();
    let printed = String::from_utf8(out).unwrap();
    assert!(printed.contains("escaped=em--dash"), "got: {}", printed);
}

#[test]
fn inject_ok_stdout_returns_ok() {
    let runner = FakeRunner::ok("ok", "");
    let mut out = Vec::new();
    inject(&runner, &mut out, "wren", "hi", false).expect("should succeed");
    let script = runner.seen.borrow().clone().unwrap();
    assert!(script.contains("keystroke"));
}

#[test]
fn inject_nonok_stdout_returns_err_with_stderr() {
    let runner = FakeRunner::ok("no claude window found for wren", "osascript noise");
    let mut out = Vec::new();
    let err = inject(&runner, &mut out, "wren", "hi", false).unwrap_err();
    assert!(err.contains("no claude window found"));
    assert!(err.contains("stderr: osascript noise"), "got: {}", err);
}

#[test]
fn inject_spawn_failure_maps_to_error() {
    let runner = FakeRunner::spawn_error();
    let mut out = Vec::new();
    let err = inject(&runner, &mut out, "kade", "hi", false).unwrap_err();
    assert!(err.starts_with("osascript spawn failed:"), "got: {}", err);
    assert!(err.contains("osascript not found"));
}

#[test]
fn inject_script_gets_escaped_text_not_raw_text() {
    let runner = FakeRunner::ok("ok", "");
    let mut out = Vec::new();
    inject(&runner, &mut out, "silas", r#"has "quote""#, false).unwrap();
    let script = runner.seen.borrow().clone().unwrap();
    assert!(
        script.contains(r#"keystroke "has \"quote\"""#),
        "got: {}",
        script
    );
}

// --- count_windows ---

#[test]
fn count_windows_trims_stdout() {
    let runner = FakeRunner::ok("  3::silas - claude  \n", "");
    let result = count_windows(&runner, "silas").unwrap();
    assert_eq!(result, "3::silas - claude");
}

#[test]
fn count_windows_empty_stdout_is_empty_string() {
    let runner = FakeRunner::ok("", "");
    let result = count_windows(&runner, "nobody").unwrap();
    assert_eq!(result, "");
}

#[test]
fn count_windows_spawn_failure_maps_to_error() {
    let runner = FakeRunner::spawn_error();
    let err = count_windows(&runner, "silas").unwrap_err();
    assert!(err.starts_with("osascript spawn failed:"), "got: {}", err);
}

#[test]
fn count_windows_script_embeds_pattern() {
    let runner = FakeRunner::ok("0::", "");
    count_windows(&runner, "silas").unwrap();
    let script = runner.seen.borrow().clone().unwrap();
    assert!(script.contains(r#"contains "silas""#));
}

// --- dispatch ---

#[test]
fn dispatch_count_windows_returns_stdout() {
    let runner = FakeRunner::ok("2::wren - claude", "");
    let mut out = Vec::new();
    let d = dispatch(
        &runner,
        &mut out,
        &svec(&["--count-windows", "wren"]),
        false,
    );
    assert_eq!(d, Dispatch::PrintOut("2::wren - claude".to_string()));
}

#[test]
fn dispatch_count_windows_spawn_error() {
    let runner = FakeRunner::spawn_error();
    let mut out = Vec::new();
    let d = dispatch(
        &runner,
        &mut out,
        &svec(&["--count-windows", "wren"]),
        false,
    );
    match d {
        Dispatch::Err(e) => assert!(e.starts_with("osascript spawn failed:")),
        other => panic!("expected Err, got {:?}", other),
    }
}

#[test]
fn dispatch_no_args_returns_usage_error() {
    let runner = FakeRunner::never();
    let mut out = Vec::new();
    let d = dispatch(&runner, &mut out, &svec(&[]), false);
    match d {
        Dispatch::Err(e) => {
            assert!(e.contains("Usage:"));
            assert!(e.contains("--count-windows"));
        }
        other => panic!("expected Err, got {:?}", other),
    }
}

#[test]
fn dispatch_one_arg_returns_usage_error() {
    let runner = FakeRunner::never();
    let mut out = Vec::new();
    let d = dispatch(&runner, &mut out, &svec(&["silas"]), false);
    assert!(matches!(d, Dispatch::Err(_)));
}

#[test]
fn dispatch_inject_success_returns_ok() {
    let runner = FakeRunner::ok("ok", "");
    let mut out = Vec::new();
    let d = dispatch(&runner, &mut out, &svec(&["kade", "hello"]), false);
    assert_eq!(d, Dispatch::Ok);
}

#[test]
fn dispatch_inject_joins_multiword_text() {
    let runner = FakeRunner::ok("ok", "");
    let mut out = Vec::new();
    dispatch(
        &runner,
        &mut out,
        &svec(&["silas", "hello", "there", "friend"]),
        false,
    );
    let script = runner.seen.borrow().clone().unwrap();
    assert!(
        script.contains(r#"keystroke "hello there friend""#),
        "got: {}",
        script
    );
}

#[test]
fn dispatch_inject_dry_run_writes_to_writer_and_returns_ok() {
    let runner = FakeRunner::never();
    let mut out = Vec::new();
    let d = dispatch(&runner, &mut out, &svec(&["wren", "hi"]), true);
    assert_eq!(d, Dispatch::Ok);
    let printed = String::from_utf8(out).unwrap();
    assert!(printed.contains("DRY-RUN inject role=wren"));
}

#[test]
fn dispatch_inject_unknown_role_returns_err() {
    let runner = FakeRunner::never();
    let mut out = Vec::new();
    let d = dispatch(&runner, &mut out, &svec(&["nobody", "hi"]), false);
    match d {
        Dispatch::Err(e) => assert!(e.contains("unknown role")),
        other => panic!("expected Err, got {:?}", other),
    }
}

#[test]
fn dispatch_inject_runner_failure_returns_err() {
    let runner = FakeRunner::ok("no claude window found for kade", "no tty");
    let mut out = Vec::new();
    let d = dispatch(&runner, &mut out, &svec(&["kade", "hi"]), false);
    match d {
        Dispatch::Err(e) => {
            assert!(e.contains("no claude window found"));
            assert!(e.contains("stderr: no tty"));
        }
        other => panic!("expected Err, got {:?}", other),
    }
}

#[test]
fn dispatch_count_windows_alone_is_usage_error() {
    // `--count-windows` without a pattern (len=1) falls through to usage.
    let runner = FakeRunner::never();
    let mut out = Vec::new();
    let d = dispatch(&runner, &mut out, &svec(&["--count-windows"]), false);
    assert!(matches!(d, Dispatch::Err(_)));
}

// --- RealOsaRunner (touches real osascript — pure AS return, no side effects) ---

#[cfg(target_os = "macos")]
#[test]
fn real_osa_runner_executes_pure_applescript() {
    // "return \"hello\"" is a pure AS expression — no Terminal activation, no
    // keystroke, no focus change. Just exercises the Command::new spawn path.
    let runner = RealOsaRunner;
    let output = runner.run("return \"hello\"").expect("osascript spawn");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.trim() == "hello", "got stdout: {:?}", stdout);
}

// --- write failure branch (writer that errors on every write) ---

struct FailingWriter;
impl io::Write for FailingWriter {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::new(io::ErrorKind::BrokenPipe, "pipe broken"))
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn inject_dry_run_write_failure_maps_to_error() {
    // The writeln! in the dry-run branch can fail — make sure we map to Err
    // rather than panic. Covers the `.map_err(|e| format!("write failed: ...")` leg.
    let runner = FakeRunner::never();
    let mut out = FailingWriter;
    let err = inject(&runner, &mut out, "silas", "hi", true).unwrap_err();
    assert!(err.starts_with("write failed:"), "got: {}", err);
}

#[test]
fn dispatch_count_windows_with_extra_arg_is_inject_unknown_role() {
    // len=3 with --count-windows first falls through to inject (unknown role).
    let runner = FakeRunner::never();
    let mut out = Vec::new();
    let d = dispatch(
        &runner,
        &mut out,
        &svec(&["--count-windows", "silas", "extra"]),
        false,
    );
    match d {
        Dispatch::Err(e) => assert!(e.contains("unknown role: --count-windows")),
        other => panic!("expected Err, got {:?}", other),
    }
}
