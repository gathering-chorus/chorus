//! #4514 AC3 — a long verb runs as a Task that outlives the runner.
//!
//! Silas + Wren, 2026-10-13: spawn the verb detached, in its own process group,
//! with stdout and stderr to `<id>.log` (a pipe back to the runner would break
//! the verb when the runner dies) and its exit code written to `<id>.exit` by a
//! tiny shell around it. `<id>.json` records what was started and by whom. Status
//! is read from those files only, so a restarted runner reads exactly what the
//! old one would have: the exit file means done; the recorded pid still alive
//! with the same start time means running (the /cws identity rule, #3781);
//! anything else is lost, never running.

use serde_json::{json, Value};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Running,
    Done(i32),
    /// The pid is gone (or reused) and no exit code was written.
    Lost,
    /// No run file for this id.
    Unknown,
}

impl Status {
    pub fn word(&self) -> String {
        match self {
            Status::Running => "running".into(),
            Status::Done(c) => format!("done (exit {c})"),
            Status::Lost => "lost: the process is gone and wrote no exit code".into(),
            Status::Unknown => "unknown task".into(),
        }
    }
}

/// Where Tasks keep their files: `$CHORUS_RUNNER_TASKS`, else `~/.chorus/runner-tasks`.
pub fn tasks_dir() -> PathBuf {
    std::env::var("CHORUS_RUNNER_TASKS").map(PathBuf::from).unwrap_or_else(|_| {
        PathBuf::from(format!("{}/.chorus/runner-tasks", std::env::var("HOME").unwrap_or_default()))
    })
}

fn file(dir: &Path, id: &str, ext: &str) -> PathBuf {
    dir.join(format!("{id}.{ext}"))
}

/// A process's start time as the OS reports it, so a reused pid is not mistaken for ours.
pub fn pid_started(pid: u32) -> Option<String> {
    let out = Command::new("ps").args(["-o", "lstart=", "-p", &pid.to_string()]).output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !s.is_empty()).then_some(s)
}

/// Start `binary argv` detached. Returns the run record written to `<id>.json`.
pub fn spawn(dir: &Path, id: &str, binary: &str, argv: &[String], env: &[(&'static str, String)], caller: &str) -> Result<Value, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("tasks dir {}: {e}", dir.display()))?;
    let log = std::fs::File::create(file(dir, id, "log")).map_err(|e| format!("log: {e}"))?;
    let err = log.try_clone().map_err(|e| format!("log: {e}"))?;
    let exit = file(dir, id, "exit");
    // `$0 "$@"` runs the verb with its argv exactly as given (one element each,
    // never re-split); the exit code is written once it finishes, runner or not
    let mut cmd = Command::new("/bin/sh");
    cmd.arg("-c")
        .arg("\"$0\" \"$@\"; echo $? > \"$CHORUS_TASK_EXIT\"")
        .arg(binary)
        .args(argv)
        .env("CHORUS_TASK_EXIT", &exit)
        .envs(env.iter().map(|(k, v)| (*k, v.as_str())))
        .stdin(Stdio::null())
        .stdout(log)
        .stderr(err)
        .process_group(0);
    let child = cmd.spawn().map_err(|e| format!("{binary} did not start: {e}"))?;
    let pid = child.id();
    let record = json!({
        "id": id, "binary": binary, "argv": argv, "caller": caller, "pid": pid,
        "pidStartedAt": pid_started(pid), "startedAt": now_secs(),
    });
    std::fs::write(file(dir, id, "json"), record.to_string()).map_err(|e| format!("run file: {e}"))?;
    // the child is not waited on here: if this runner stops, it keeps going
    std::mem::forget(child);
    Ok(record)
}

/// What a Task is doing now, from its files alone.
pub fn status(dir: &Path, id: &str) -> Status {
    if let Ok(code) = std::fs::read_to_string(file(dir, id, "exit")) {
        return code.trim().parse().map(Status::Done).unwrap_or(Status::Lost);
    }
    let Some(rec) = std::fs::read_to_string(file(dir, id, "json")).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()) else {
        return Status::Unknown;
    };
    let pid = rec["pid"].as_u64().unwrap_or(0) as u32;
    match (pid_started(pid), rec["pidStartedAt"].as_str()) {
        (Some(now), Some(then)) if now == then => Status::Running,
        _ => Status::Lost,
    }
}

/// The last lines of a Task's output.
pub fn tail(dir: &Path, id: &str, lines: usize) -> String {
    let text = std::fs::read_to_string(file(dir, id, "log")).unwrap_or_default();
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}
