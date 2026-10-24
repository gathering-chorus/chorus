// @test-type: unit
// @domain: code
// #4707 reopen — the land's borg-ask step waits for the roles door before it
// nudges Borg Queen (src/main.rs, door_answers). werk-deploy-prod restarts the
// door just before this step; on 10-23 at 18:12 #4715's land failed here with
// "Connection refused". What Jeff sees: a land does not stop at borg-ask because
// a door was mid-restart, and a door that never comes back still stops it, named.
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

fn bin() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_borg-prove")) }

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("borg-4707-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A door on `listener` that answers 200 to every request.
fn serve(listener: TcpListener) {
    std::thread::spawn(move || {
        for s in listener.incoming().flatten() {
            let mut s = s;
            let mut buf = [0u8; 1024];
            let _ = s.read(&mut buf);
            let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        }
    });
}

/// A port nothing listens on yet.
fn free_port() -> u16 { TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port() }

fn fake_nudge(d: &PathBuf) -> PathBuf {
    let n = d.join("nudge");
    std::fs::write(&n, format!("#!/bin/sh\necho \"$@\" >> {}/nudged\n", d.display())).unwrap();
    Command::new("chmod").args(["+x", n.to_str().unwrap()]).status().unwrap();
    n
}

fn ask(d: &PathBuf, port: u16, wait: &str) -> std::process::Output {
    Command::new(bin()).args(["ask", "4707", "silas"]).env("BORG_NUDGE", fake_nudge(d)).env("DAG_RUN_ID", "run1")
        .env("BORG_DOOR_URL", format!("http://127.0.0.1:{port}/v1/roles/roles?limit=1")).env("BORG_DOOR_WAIT", wait)
        .output().unwrap()
}

#[test]
fn a_door_that_comes_back_mid_wait_gets_her_nudged_once() {
    let d = dir("back");
    let port = free_port();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(3));
        serve(TcpListener::bind(("127.0.0.1", port)).unwrap());
    });
    let t = Instant::now();
    let o = ask(&d, port, "30");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(t.elapsed() >= Duration::from_secs(3), "asked before the door was up");
    let said = std::fs::read_to_string(d.join("nudged")).unwrap();
    assert_eq!(said.lines().filter(|l| l.starts_with("borg-queen ")).count(), 1, "{said}");
}

/// NEGATIVE: a door that never answers stops the step, names the door, and
/// nobody is nudged.
#[test]
fn negative_proof_a_door_that_never_answers_stops_the_step_unnudged() {
    let d = dir("down");
    let o = ask(&d, free_port(), "3");
    assert!(!o.status.success());
    let e = String::from_utf8_lossy(&o.stderr);
    assert!(e.contains("the roles door did not answer within 3s"), "{e}");
    assert!(!d.join("nudged").exists(), "Borg Queen was nudged through a closed door");
}
