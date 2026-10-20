// @test-type: unit
// @domain: services
//! #4661 — a finished Task never reads Lost. The 10-20 nightly read a task that
//! had exited 0 as `Lost` (tasks_4514.rs:37): the shell creates the exit file
//! and then writes the code, so a read in between saw an empty file, which did
//! not parse and fell to Lost.
use chorus_runner::tasks::{spawn, status, Status};
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("runner-tasks-4661-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn sh(script: &str) -> Vec<String> {
    vec!["-c".into(), script.into()]
}

#[test]
fn negative_proof_an_exit_file_still_being_written_does_not_read_lost() {
    let d = dir("empty-exit");
    // the shape of the window: the process is alive, its exit file exists but is empty
    spawn(&d, "t1", "/bin/sh", &sh("sleep 3"), &[], "silas").unwrap();
    std::fs::write(d.join("t1.exit"), "").unwrap();
    assert_eq!(status(&d, "t1"), Status::Running, "a half-written exit file read as {:?}", status(&d, "t1"));
}

#[test]
fn the_exit_file_appears_whole_never_empty() {
    let d = dir("atomic");
    spawn(&d, "t2", "/bin/sh", &sh("true"), &[], "silas").unwrap();
    // read as fast as possible while it finishes: every read is Running or Done(0), never Lost
    let t = Instant::now();
    let mut seen_done = false;
    while t.elapsed() < Duration::from_secs(10) {
        match status(&d, "t2") {
            Status::Done(0) => { seen_done = true; break; }
            Status::Running => {}
            other => panic!("a finishing task read {other:?}"),
        }
    }
    assert!(seen_done, "the task never read Done(0)");
}
