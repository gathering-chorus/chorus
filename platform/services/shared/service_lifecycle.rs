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


/// A scheduled job's failure, reported by the job itself. Call first in main.
/// When launchd started this process, it runs the rest of the program as a
/// child and reports the child's non-zero exit or killing signal as
/// service.failed, then exits with the same code — so every `exit(n)` and
/// panic in the program is covered without touching them. Run by hand, or by
/// anything other than launchd, it returns at once and nothing changes.
pub fn run_as_job() {
    // the child's parent is this process, not launchd, so it runs on through
    let Some(label) = launchd_label() else { return };
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return,
    };
    let status = std::process::Command::new(exe)
        .args(std::env::args_os().skip(1))
        .status();
    let (code, fields) = match status {
        Ok(s) if s.success() => std::process::exit(0),
        Ok(s) => {
            use std::os::unix::process::ExitStatusExt;
            let mut f = vec![("service", label.clone()), ("pid", std::process::id().to_string())];
            match (s.code(), s.signal()) {
                (Some(c), _) => {
                    f.push(("reason", format!("exited {c}")));
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
