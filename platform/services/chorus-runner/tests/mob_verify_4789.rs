// @test-type: unit
// @domain: services
//! #4789 AC3 — `mob verify` takes WHO verified only from the runner's own record
//! of the call. It finds that record by CHORUS_TASK_EXIT and proves it is its
//! own: the record's pid is its parent (the runner's `sh -c`) and the record's
//! pidStartedAt is that parent's start time. Silas (06:27): both are unproven
//! until the real runner's spawn shows them. This drives the real `spawn` and
//! reads what a task's child actually sees.
use chorus_runner::tasks::{spawn, status, tail, Status};
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("runner-tasks-4789-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn wait_done(d: &PathBuf, id: &str) -> Status {
    let t = Instant::now();
    loop {
        let s = status(d, id);
        if s != Status::Running || t.elapsed() > Duration::from_secs(10) {
            return s;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// a stand-in verb that reports what `mob verify` reads about itself
fn probe(d: &PathBuf) -> String {
    let p = d.join("probe.sh");
    std::fs::write(
        &p,
        "#!/bin/sh\necho \"ppid=$PPID\"\necho \"pstart=$(ps -o lstart= -p $PPID | sed 's/^ *//; s/ *$//')\"\necho \"exit=$CHORUS_TASK_EXIT\"\n",
    )
    .unwrap();
    std::process::Command::new("chmod").args(["+x", p.to_str().unwrap()]).status().unwrap();
    p.to_string_lossy().into_owned()
}

fn field<'a>(log: &'a str, k: &str) -> &'a str {
    log.lines().find_map(|l| l.strip_prefix(&format!("{k}="))).unwrap_or("")
}

#[test]
fn a_task_child_sees_its_record_pid_as_its_parent_and_its_exit_path() {
    let d = dir("ppid");
    let bin = probe(&d);
    let rec = spawn(&d, "mob-verify-t1", &bin, &["verify".into(), "4789".into()], &[], "silas").unwrap();
    assert_eq!(wait_done(&d, "mob-verify-t1"), Status::Done(0));
    let log = tail(&d, "mob-verify-t1", 10);
    assert_eq!(field(&log, "ppid"), rec["pid"].to_string(), "the record's pid is not the verb's parent:\n{log}");
    assert_eq!(field(&log, "pstart"), rec["pidStartedAt"].as_str().unwrap(), "the record's start time is not the parent's:\n{log}");
    assert_eq!(field(&log, "exit"), d.join("mob-verify-t1.exit").to_string_lossy(), "CHORUS_TASK_EXIT does not name the record's .exit");
    assert_eq!(rec["caller"], "silas");
}

#[test]
fn negative_proof_a_verb_started_outside_the_runner_has_no_record_to_claim() {
    // the same probe run from a shell: no CHORUS_TASK_EXIT, and its parent is not a record's pid
    let d = dir("shell");
    let bin = probe(&d);
    let out = std::process::Command::new(&bin).env_remove("CHORUS_TASK_EXIT").output().unwrap();
    let log = String::from_utf8_lossy(&out.stdout);
    assert_eq!(field(&log, "exit"), "", "a shell call carried a task exit path");
    assert_eq!(field(&log, "ppid"), std::process::id().to_string(), "the shell-run probe's parent should be this test, not a runner sh");
}
