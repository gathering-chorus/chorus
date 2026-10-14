// @test-type: unit
// @domain: services
//! #4514 AC3 — a long verb runs as a Task that outlives the runner: status comes
//! from the Task's files alone, and the verb is in its own process group, so
//! stopping the runner (launchd signals the runner's group) does not stop it.
use chorus_runner::tasks::{spawn, status, tail, Status};
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("runner-tasks-4514-{}-{name}", std::process::id()));
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

fn sh(script: &str) -> Vec<String> {
    vec!["-c".into(), script.into()]
}

#[test]
fn a_task_reads_running_then_done_from_its_files_alone() {
    let d = dir("ok");
    spawn(&d, "t1", "/bin/sh", &sh("sleep 1; echo finished"), &[], "wren").unwrap();
    // a fresh read, with no state kept from the spawn: what a restarted runner sees
    assert_eq!(status(&d, "t1"), Status::Running);
    assert_eq!(wait_done(&d, "t1"), Status::Done(0));
    assert!(tail(&d, "t1", 5).contains("finished"));
}

#[test]
fn a_task_is_in_its_own_process_group_so_a_runner_stop_does_not_reach_it() {
    let d = dir("pgid");
    let rec = spawn(&d, "t2", "/bin/sh", &sh("sleep 2"), &[], "wren").unwrap();
    let pgid = |pid: u32| String::from_utf8_lossy(&std::process::Command::new("ps").args(["-o", "pgid=", "-p", &pid.to_string()]).output().unwrap().stdout).trim().to_string();
    let child = rec["pid"].as_u64().unwrap() as u32;
    assert_ne!(pgid(child), pgid(std::process::id()), "the task shares the runner's process group");
    assert_eq!(pgid(child), child.to_string(), "the task leads its own group");
}

#[test]
fn negative_proof_a_failing_verb_reads_done_with_its_code_never_success() {
    let d = dir("fail");
    spawn(&d, "t3", "/bin/sh", &sh("echo boom >&2; exit 3"), &[], "wren").unwrap();
    assert_eq!(wait_done(&d, "t3"), Status::Done(3));
    assert!(tail(&d, "t3", 5).contains("boom"), "stderr goes to the log too");
}

#[test]
fn negative_proof_a_gone_or_reused_pid_reads_lost_never_running() {
    let d = dir("lost");
    // our own pid is alive, but its start time is not the one recorded: a reused pid
    std::fs::write(d.join("t4.json"), format!(r#"{{"id":"t4","pid":{},"pidStartedAt":"Thu Jan  1 00:00:00 1970"}}"#, std::process::id())).unwrap();
    assert_eq!(status(&d, "t4"), Status::Lost);
    assert_eq!(status(&d, "nope"), Status::Unknown);
}

#[test]
fn a_hostile_argument_stays_one_argument() {
    let d = dir("argv");
    // the verb is /bin/echo; "a; rm -rf x" must arrive as one argument, never run
    spawn(&d, "t5", "/bin/echo", &["a; touch hacked".to_string(), "b".to_string()], &[], "wren").unwrap();
    assert_eq!(wait_done(&d, "t5"), Status::Done(0));
    assert_eq!(tail(&d, "t5", 1), "a; touch hacked b");
    assert!(!d.join("hacked").exists() && !std::path::Path::new("hacked").exists());
}
