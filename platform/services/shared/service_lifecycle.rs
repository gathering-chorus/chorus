// #4446 — a com.chorus.* service logs its own start, stop and failure.
//
// Jeff, 2026-10-06: "all services must be rigorous about structured logging of
// starts stops and failures". Each daemon calls this at startup and shutdown
// and writes the events through its own logger. std only, so any crate can
// `include!` it.
//
// A process cannot log being SIGKILLed. So at startup the service asks launchd
// how its previous run ended (`launchctl print` keeps the last exit code or
// terminating signal) and, when that run did not end cleanly, reports it as
// service.failed before reporting its own start.
//
// The label comes from XPC_SERVICE_NAME, which launchd sets for every agent it
// starts. A binary run by hand has no label and reports nothing.

/// One event for the service's logger: (event name, fields).
pub type LifecycleEvent = (&'static str, Vec<(&'static str, String)>);

/// How the previous run ended, read from `launchctl print`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LastExit {
    pub exit_code: Option<i32>,
    pub signal: Option<String>,
}

impl LastExit {
    /// exit 0, or SIGTERM (bootout, kickstart -k, a deploy restart).
    pub fn clean(&self) -> bool {
        match &self.signal {
            Some(sig) => sig.starts_with("Terminated"),
            None => self.exit_code.unwrap_or(0) == 0,
        }
    }
}

/// The top-level `last exit code` / `last terminating signal` of a
/// `launchctl print` block. None when launchd has no previous run on record.
pub fn parse_last_exit(print: &str) -> Option<LastExit> {
    let mut last = LastExit::default();
    let mut seen = false;
    for line in print.lines() {
        // first-level fields only: nested blocks are indented twice
        let Some(rest) = line.strip_prefix('\t') else { continue };
        if rest.starts_with('\t') {
            continue;
        }
        let Some((k, v)) = rest.split_once(" = ") else { continue };
        match k {
            // "1", or "78: EX_CONFIG"
            "last exit code" => {
                last.exit_code = v.split(':').next().and_then(|n| n.trim().parse().ok());
                seen = true;
            }
            "last terminating signal" => {
                last.signal = Some(v.trim().to_string());
                seen = true;
            }
            _ => {}
        }
    }
    seen.then_some(last)
}

/// What a service reports as it starts: the previous run's failure (if it had
/// one), then its own start.
pub fn start_events(label: &str, pid: u32, version: &str, previous: Option<&LastExit>) -> Vec<LifecycleEvent> {
    let mut out = Vec::new();
    if let Some(prev) = previous.filter(|p| !p.clean()) {
        let mut f = vec![("service", label.to_string()), ("reason", "previous run ended abnormally".to_string())];
        match &prev.signal {
            Some(sig) => f.push(("signal", sig.clone())),
            None => f.push(("exit_code", prev.exit_code.unwrap_or(0).to_string())),
        }
        out.push(("service.failed", f));
    }
    out.push((
        "service.started",
        vec![("service", label.to_string()), ("pid", pid.to_string()), ("version", version.to_string())],
    ));
    out
}

/// A clean shutdown (a signal it handled, or a normal return).
pub fn stop_event(label: &str, pid: u32, reason: &str) -> LifecycleEvent {
    (
        "service.stopped",
        vec![("service", label.to_string()), ("pid", pid.to_string()), ("reason", reason.to_string())],
    )
}

/// An error the service is about to exit on.
pub fn failed_event(label: &str, pid: u32, reason: &str, exit_code: i32) -> LifecycleEvent {
    (
        "service.failed",
        vec![
            ("service", label.to_string()),
            ("pid", pid.to_string()),
            ("reason", reason.to_string()),
            ("exit_code", exit_code.to_string()),
        ],
    )
}

/// The launchd label this process runs under, if launchd started it. The
/// parent must be launchd (pid 1): everything a service starts inherits
/// XPC_SERVICE_NAME, and a chorus-log call from inside chorus-api is not
/// chorus-api starting.
pub fn launchd_label() -> Option<String> {
    if std::os::unix::process::parent_id() != 1 {
        return None;
    }
    std::env::var("XPC_SERVICE_NAME").ok().filter(|l| l.starts_with("com.chorus."))
}

/// Ask launchd how this label's previous run ended.
pub fn previous_run(label: &str) -> Option<LastExit> {
    let uid = std::process::Command::new("/usr/bin/id").arg("-u").output().ok()?;
    let uid = String::from_utf8_lossy(&uid.stdout).trim().to_string();
    let out = std::process::Command::new("/bin/launchctl")
        .args(["print", &format!("gui/{uid}/{label}")])
        .output()
        .ok()?;
    parse_last_exit(&String::from_utf8_lossy(&out.stdout))
}

/// launchd's stderr path for a label (`stderr path = …` in `launchctl print`).
pub fn stderr_path(print: &str) -> Option<String> {
    print.lines().find_map(|l| l.strip_prefix("\tstderr path = ").map(|p| p.trim().to_string()))
}

/// #4446 reopen — prod had 121 failures that all said "exited N". The reason
/// is the last non-blank line the run wrote to its stderr log, taken only from
/// what THIS run appended (`since` = the log's text after the run's start), so
/// an older run's line is never borrowed.
pub fn failure_reason(code: i32, since: &str) -> String {
    match since.lines().map(str::trim).rfind(|l| !l.is_empty()) {
        Some(l) => format!("exited {code}: {}", l.chars().take(200).collect::<String>()),
        None => format!("exited {code}"),
    }
}

fn launchctl_print(label: &str) -> String {
    let uid = std::process::Command::new("/usr/bin/id").arg("-u").output().map(|o| o.stdout).unwrap_or_default();
    let uid = String::from_utf8_lossy(&uid).trim().to_string();
    std::process::Command::new("/bin/launchctl")
        .args(["print", &format!("gui/{uid}/{label}")])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

fn appended_since(path: &Option<String>, mark: u64) -> String {
    use std::io::{Read, Seek, SeekFrom};
    let Some(p) = path else { return String::new() };
    let Ok(mut f) = std::fs::File::open(p) else { return String::new() };
    let mut buf = Vec::new();
    if f.seek(SeekFrom::Start(mark)).is_ok() {
        let _ = f.read_to_end(&mut buf);
    }
    String::from_utf8_lossy(&buf).into_owned()
}

/// First 12 hex of the running binary's sha256 — names exactly what ran.
/// Uses `shasum` so this file needs no crate dependencies.
pub fn binary_version() -> String {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return "unknown".into(),
    };
    std::process::Command::new("/usr/bin/shasum")
        .args(["-a", "256"])
        .arg(&exe)
        .output()
        .ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).split_whitespace().next().map(|h| h[..12].to_string()))
        .unwrap_or_else(|| "unknown".into())
}


/// A scheduled job's start, end and failure, reported by the job itself. Call
/// first in main. When launchd started this process, it logs service.started,
/// runs the rest of the program as a child, and logs service.stopped on exit 0
/// or the child's non-zero exit or killing signal as service.failed, then exits with the same code — so every `exit(n)` and
/// panic in the program is covered without touching them. Run by hand, or by
/// anything other than launchd, it returns at once and nothing changes.
pub fn run_as_job() {
    // the child's parent is this process, not launchd, so it runs on through
    let Some(label) = launchd_label() else { return };
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return,
    };
    // #4446 round 2 (Jeff: "rigorous about ... starts stops and failures"):
    // every run is a start and an end, not only a failure
    let pid = std::process::id();
    let err = stderr_path(&launchctl_print(&label));
    let mark = err.as_ref().and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len()).unwrap_or(0);
    emit_via_chorus_log("service.started", &[
        ("service", label.clone()), ("pid", pid.to_string()), ("version", binary_version())]);
    let status = std::process::Command::new(exe)
        .args(std::env::args_os().skip(1))
        .status();
    let (code, fields) = match status {
        Ok(s) if s.success() => {
            emit_via_chorus_log("service.stopped", &[
                ("service", label.clone()), ("pid", pid.to_string()), ("reason", "exit 0".to_string())]);
            std::process::exit(0)
        }
        Ok(s) => {
            use std::os::unix::process::ExitStatusExt;
            let mut f = vec![("service", label.clone()), ("pid", std::process::id().to_string())];
            match (s.code(), s.signal()) {
                (Some(c), _) => {
                    f.push(("reason", failure_reason(c, &appended_since(&err, mark))));
                    f.push(("exit_code", c.to_string()));
                    (c, f)
                }
                (None, sig) => {
                    f.push(("reason", "killed".to_string()));
                    f.push(("signal", sig.map(|n| n.to_string()).unwrap_or_default()));
                    (1, f)
                }
            }
        }
        Err(e) => (1, vec![("service", label.clone()), ("reason", format!("could not start: {e}")), ("exit_code", "1".into())]),
    };
    emit_via_chorus_log("service.failed", &fields);
    std::process::exit(code);
}

/// Through the chorus-log script, for crates with no spine writer of their own.
pub fn emit_via_chorus_log(event: &str, fields: &[(&str, String)]) {
    let home = std::env::var("CHORUS_HOME")
        .unwrap_or_else(|_| format!("{}/CascadeProjects/chorus", std::env::var("HOME").unwrap_or_default()));
    let mut cmd = std::process::Command::new("bash");
    cmd.arg(format!("{home}/platform/scripts/chorus-log")).arg(event).arg("system");
    for (k, v) in fields {
        cmd.arg(format!("{k}={v}"));
    }
    if event == "service.failed" {
        cmd.arg("--level=error");
    }
    let _ = cmd.output();
}

/// #4446 tests, kept beside the code (#4462): every crate that includes this
/// file runs them, so each crate's coverage counts the helper it ships.
/// Fixtures are real `launchctl print` output, 2026-10-06.
#[cfg(test)]
mod tests {
    use super::*;

    const KILLED: &str = "gui/501/com.chorus.hooks = {
\tstate = running
\truns = 48
\tpid = 38837
\tlast terminating signal = Killed: 9
\tendpoints = {
\t\tlast exit code = 0
\t}
}";
    const TERMINATED: &str = "gui/501/com.chorus.athena-make = {\n\truns = 20\n\tlast terminating signal = Terminated: 15\n}";
    const EX_CONFIG: &str = "gui/501/com.chorus.fixture-ex-config = {\n\truns = 3325\n\tlast exit code = 78: EX_CONFIG\n}";
    const FIRST_RUN: &str = "gui/501/com.chorus.api = {\n\truns = 1\n\tpid = 52021\n}";

    fn names(v: &[LifecycleEvent]) -> Vec<&str> {
        v.iter().map(|(e, _)| *e).collect()
    }

    #[test]
    fn reads_the_previous_runs_end_from_launchctl() {
        assert_eq!(parse_last_exit(KILLED), Some(LastExit { exit_code: None, signal: Some("Killed: 9".into()) }));
        assert_eq!(parse_last_exit(EX_CONFIG).unwrap().exit_code, Some(78));
        assert_eq!(parse_last_exit(FIRST_RUN), None, "no previous run on record");
    }

    #[test]
    fn a_start_after_a_kill_reports_the_kill_first() {
        let prev = parse_last_exit(KILLED);
        let ev = start_events("com.chorus.hooks", 4242, "abc123def456", prev.as_ref());
        assert_eq!(names(&ev), ["service.failed", "service.started"]);
        assert!(ev[0].1.contains(&("signal", "Killed: 9".into())));
        assert!(ev[1].1.contains(&("pid", "4242".into())));
        assert!(ev[1].1.contains(&("version", "abc123def456".into())));
    }

    #[test]
    fn a_start_after_an_error_exit_reports_the_exit_code() {
        let ev = start_events("com.chorus.fixture-ex-config", 1, "v", parse_last_exit(EX_CONFIG).as_ref());
        assert!(ev[0].1.contains(&("exit_code", "78".into())));
    }

    #[test]
    fn negative_proof_a_clean_previous_end_is_not_a_failure() {
        // SIGTERM is a deploy or a kickstart: a start, nothing failed.
        assert_eq!(names(&start_events("x", 1, "v", parse_last_exit(TERMINATED).as_ref())), ["service.started"]);
        assert_eq!(names(&start_events("x", 1, "v", None)), ["service.started"]);
        // a first run: launchd prints "(never exited)" — not a failure
        let never = parse_last_exit("x = {\n\tlast exit code = (never exited)\n}");
        assert_eq!(names(&start_events("x", 1, "v", never.as_ref())), ["service.started"]);
    }

    #[test]
    fn stop_and_failure_carry_the_reason() {
        let (e, f) = stop_event("com.chorus.hooks", 7, "SIGTERM");
        assert_eq!(e, "service.stopped");
        assert!(f.contains(&("reason", "SIGTERM".into())));
        let (e, f) = failed_event("com.chorus.hooks", 7, "bind: address in use", 1);
        assert_eq!(e, "service.failed");
        assert!(f.contains(&("exit_code", "1".into())));
    }

    #[test]
    fn negative_proof_an_inherited_label_is_not_launchd_starting_us() {
        // Everything a service starts inherits XPC_SERVICE_NAME; only a process
        // whose parent is launchd is that service. This test's parent is cargo.
        // (The positive half runs under real launchd jobs in 4446-service-lifecycle.bats.)
        std::env::set_var("XPC_SERVICE_NAME", "com.chorus.api");
        assert_eq!(launchd_label(), None);
        std::env::remove_var("XPC_SERVICE_NAME");
    }

    #[test]
    fn a_failure_reason_is_the_runs_last_stderr_line() {
        assert_eq!(failure_reason(4, "starting\nboom: disk full\n\n"), "exited 4: boom: disk full");
    }

    #[test]
    fn negative_proof_a_run_with_no_stderr_says_only_its_exit_code() {
        assert_eq!(failure_reason(4, ""), "exited 4");
        assert_eq!(failure_reason(4, "  \n\n"), "exited 4");
    }

    #[test]
    fn reads_the_stderr_path_from_launchctl() {
        let p = "gui/501/com.chorus.alert-runner = {\n\tstdout path = /a.log\n\tstderr path = /b.log\n}";
        assert_eq!(stderr_path(p).as_deref(), Some("/b.log"));
        assert_eq!(stderr_path(FIRST_RUN), None);
    }
}
