// @test-type: unit
// @domain: code
// #4691 — Borg Queen proves the land the way Abby navigates a pair
// (src/main.rs, src/lib.rs). The land asks her (ask), she runs record in her
// own session, which writes the run's verdict file as her Mac account, and
// completes her step; check lets accept run only on a verdict she wrote.
// What Jeff sees: a card is accepted on Borg Queen's proof, and on nobody
// else's: a step signed by anyone else, or a verdict file another account
// wrote, holds the land.
use borg_prove::signed_verdict;
use std::path::PathBuf;
use std::process::Command;

const Q: &str = "chorus-borg-queen";

#[test]
fn her_signature_on_her_proven_verdict_lets_accept_run() {
    assert_eq!(signed_verdict("borg-queen", Some(Q), Some("Proven"), Q), Ok(()));
    assert_eq!(signed_verdict("borg-queen", Some(Q), Some("unproven"), Q), Ok(()), "a card with no CardAC rows lands as before");
}

/// NEGATIVE: each way the proof is not hers, or not green, holds accept.
#[test]
fn negative_proof_anything_but_her_green_verdict_holds_accept() {
    for (by, owner, v, why) in [
        ("kade", Some(Q), Some("Proven"), "signed by kade, not borg-queen"),
        ("jeff", Some(Q), Some("Proven"), "signed by jeff, not borg-queen"),
        ("borg-queen", None, None, "no verdict recorded for this run"),
        ("borg-queen", Some("jeffbridwell"), Some("Proven"), "written by jeffbridwell, not chorus-borg-queen"),
        ("borg-queen", Some(Q), Some("Not met"), "verdict Not met: accept held"),
        ("borg-queen", Some(Q), Some("Unmeasured"), "verdict Unmeasured: accept held"),
    ] {
        let e = signed_verdict(by, owner, v, Q).expect_err(why);
        assert!(e.contains(why), "{e} / {why}");
    }
}

fn bin() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_borg-prove")) }

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("borg-4691-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn me() -> String { String::from_utf8(Command::new("id").arg("-un").output().unwrap().stdout).unwrap().trim().to_string() }

/// check reads the run's verdict file and its owner from disk: the file this
/// test writes is owned by the test's own account, so naming that account as
/// the queen's passes and naming chorus-borg-queen holds.
#[test]
fn check_reads_the_verdict_file_and_who_wrote_it() {
    let d = dir("check");
    std::fs::write(d.join("4691-run1.verdict"), "Proven\n").unwrap();
    let run = |queen: &str, by: &str| Command::new(bin()).args(["check", "4691", by])
        .env("BORG_RESULTS_DIR", &d).env("DAG_RUN_ID", "run1").env("BORG_QUEEN_ACCOUNT", queen).output().unwrap();
    let ok = run(&me(), "borg-queen");
    assert!(ok.status.success(), "{}", String::from_utf8_lossy(&ok.stderr));
    let held = run(Q, "borg-queen");
    assert!(!held.status.success());
    assert!(String::from_utf8_lossy(&held.stderr).contains("not chorus-borg-queen"), "{}", String::from_utf8_lossy(&held.stderr));
    let none = Command::new(bin()).args(["check", "4691", "borg-queen"])
        .env("BORG_RESULTS_DIR", &d).env("DAG_RUN_ID", "run2").env("BORG_QUEEN_ACCOUNT", me()).output().unwrap();
    assert!(!none.status.success(), "a run with no verdict file holds");
}

/// ask nudges borg-queen with the two commands her session runs, for this
/// card and this land run.
#[test]
fn ask_nudges_her_with_record_and_the_step_to_complete() {
    let d = dir("ask");
    let n = d.join("nudge");
    std::fs::write(&n, format!("#!/bin/sh\necho \"$@\" >> {}/nudged\n", d.display())).unwrap();
    Command::new("chmod").args(["+x", n.to_str().unwrap()]).status().unwrap();
    // #4707 — ask waits for the roles door first; this one is up
    let door = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1/roles/roles?limit=1", door.local_addr().unwrap());
    std::thread::spawn(move || for s in door.incoming().flatten() {
        use std::io::{Read, Write};
        let mut s = s;
        let _ = s.read(&mut [0u8; 1024]);
        let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    });
    let o = Command::new(bin()).args(["ask", "4691", "kade"]).env("BORG_NUDGE", &n).env("DAG_RUN_ID", "run7")
        .env("BORG_DOOR_URL", url).env("BORG_DOOR_WAIT", "5").output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let said = std::fs::read_to_string(d.join("nudged")).unwrap();
    assert!(said.starts_with("borg-queen "), "{said}");
    assert!(said.contains("DAG_RUN_ID=run7 borg-prove record 4691 kade"), "{said}");
    assert!(said.contains("human-task complete cicd-land --run-id run7 --step skill_borg_queen_proves --input by=borg-queen"), "{said}");
}
