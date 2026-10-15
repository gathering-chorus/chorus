// @test-type: integration — drives the built werk-test against temp git werks and a loopback stub door; no live service
// @domain: tests
//! #4536 — the runner's own test. Each row is a tiny throwaway werk (a temp git
//! repo, never committed) run through the real `werk-test <card> <role>`
//! against a stub door, and must end in the one result its fail route names.
//! It never runs the product's suites and finishes in seconds.
//!
//! Rows land with the card that builds their route (plan 10-15): this card owns
//! 1 pass, 2 fail, 6 ran nothing, 9 store refused, 10 no werk. #4537 adds 3, 4,
//! 5, 11; #4539 adds 7, 8; 12 lands with the membrane row.
//!
//! Hermetic: registry, quarantine, stack probe and the result store are one
//! TcpListener stub; `werk-demo` on PATH is a stub, so no badge is written;
//! CHORUS_HOME is removed, so no spine and no token mint.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

fn read_request(stream: &mut impl Read) -> String {
    let mut data = Vec::new();
    let mut buf = [0u8; 8192];
    let mut wanted = None;
    loop {
        let n = stream.read(&mut buf).unwrap_or(0);
        if n == 0 { break; }
        data.extend_from_slice(&buf[..n]);
        if wanted.is_none() {
            if let Some(pos) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&data[..pos + 4]).to_ascii_lowercase();
                let len = headers.lines()
                    .find(|l| l.starts_with("content-length:"))
                    .and_then(|l| l.split_once(':'))
                    .and_then(|(_, v)| v.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                wanted = Some(pos + 4 + len);
            }
        }
        if wanted.is_some_and(|l| data.len() >= l) { break; }
    }
    String::from_utf8_lossy(&data).into_owned()
}

/// GET /tests → the registry; PATCH /tests/<name> → `patch_status`, kept;
/// anything else (stack probe, suite-run post) → 200.
fn stub(registry: String, patch_status: u16) -> (u16, Arc<Mutex<Vec<String>>>) {
    let l = TcpListener::bind("127.0.0.1:0").expect("stub bind");
    let port = l.local_addr().unwrap().port();
    let patches = Arc::new(Mutex::new(Vec::new()));
    let p2 = Arc::clone(&patches);
    std::thread::spawn(move || {
        for c in l.incoming() {
            let Ok(mut c) = c else { continue };
            let req = read_request(&mut c);
            let first = req.lines().next().unwrap_or("").to_string();
            let resp = if first.starts_with("GET ") && first.contains("/tests") {
                format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    registry.len(), registry)
            } else if first.starts_with("PATCH ") {
                p2.lock().unwrap().push(req.clone());
                format!("HTTP/1.1 {} X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", patch_status)
            } else {
                "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_string()
            };
            let _ = c.write_all(resp.as_bytes());
        }
    });
    (port, patches)
}

fn row(file: &str, test: &str, entity: &str) -> String {
    format!("{{\"filePath\":\"{file}\",\"covers\":\"werk\",\"testType\":\"unit\",\
             \"testName\":\"{test}\",\"name\":\"{entity}\",\"hermeticity\":\"hermetic\"}}")
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git").arg("-C").arg(dir).args(args)
        .env("GIT_AUTHOR_NAME", "f").env("GIT_AUTHOR_EMAIL", "f@f")
        .env("GIT_COMMITTER_NAME", "f").env("GIT_COMMITTER_EMAIL", "f@f")
        .output().map(|o| o.status.success()).unwrap_or(false);
    assert!(ok, "git {:?} in {}", args, dir.display());
}

struct Run { code: i32, out: String, patches: Vec<String> }

/// One fixture: a werk at <base>/fix-<card> whose last commit adds `files`.
fn run_fixture(tag: &str, card: &str, files: &[(&str, &str)], registry_rows: &[String], patch_status: u16) -> Run {
    let base = std::env::temp_dir().join(format!("werk-test-4536-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let werk = base.join(format!("fix-{}", card));
    std::fs::create_dir_all(&werk).unwrap();
    git(&werk, &["init", "-q"]);
    std::fs::write(werk.join("README"), "fixture\n").unwrap();
    git(&werk, &["add", "-A"]);
    git(&werk, &["commit", "-qm", "base"]);
    for (path, body) in files {
        let p = werk.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, body).unwrap();
    }
    git(&werk, &["add", "-A"]);
    git(&werk, &["commit", "-qm", "card"]);
    run_in(&base, card, registry_rows, patch_status)
}

fn stub_path(base: &Path) -> String {
    let bin: PathBuf = base.join("stub-bin");
    std::fs::create_dir_all(&bin).unwrap();
    let demo = bin.join("werk-demo");
    std::fs::write(&demo, "#!/bin/sh\nexit 0\n").unwrap();
    Command::new("chmod").arg("+x").arg(&demo).status().unwrap();
    format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default())
}

fn run_in(base: &Path, card: &str, registry_rows: &[String], patch_status: u16) -> Run {
    std::fs::create_dir_all(base).unwrap();
    let registry = format!("{{\"data\":[{}]}}", registry_rows.join(","));
    let (port, patches) = stub(registry, patch_status);
    let door = format!("http://127.0.0.1:{}", port);
    let out = Command::new(env!("CARGO_BIN_EXE_werk-test"))
        .args([card, "fix"])
        .env("PATH", stub_path(base))
        .env("CHORUS_WERK_BASE", base)
        .env_remove("WERK_TEST_TREE")
        .env_remove("CHORUS_HOME")
        .env("CHORUS_WRITE_TOKEN", "fixture")
        .env("OWL_API_TESTS", format!("{door}/tests?limit=10000"))
        .env("OWL_API_TESTSUITERUNS", format!("{door}/testsuiteruns"))
        .env("WERK_STACK_PROBES", format!("door={door}/"))
        .output()
        .expect("spawn werk-test");
    let _ = std::fs::remove_dir_all(base);
    let written = patches.lock().unwrap().clone();
    Run {
        code: out.status.code().unwrap_or(-1),
        out: format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)),
        patches: written,
    }
}

const PASS: &str = "#!/usr/bin/env bats\n@test \"p\" {\n  true\n}\n";
const FAIL: &str = "#!/usr/bin/env bats\n@test \"f\" {\n  false\n}\n";
const EMPTY: &str = "#!/usr/bin/env bats\n# no cases\n";

#[test]
fn row_01_one_passing_case_is_clean_exit_0() {
    let r = run_fixture("r1", "91001", &[("platform/tests/pass.bats", PASS)],
        &[row("platform/tests/pass.bats", "p", "test-pass")], 200);
    assert_eq!(r.code, 0, "{}", r.out);
    assert!(r.out.contains("werk-test: CLEAN (exit 0)"), "{}", r.out);
}

#[test]
fn row_02_a_failing_assert_is_red_exit_1() {
    let r = run_fixture("r2", "91002", &[("platform/tests/fail.bats", FAIL)],
        &[row("platform/tests/fail.bats", "f", "test-fail")], 200);
    assert_eq!(r.code, 1, "{}", r.out);
    assert!(r.out.contains("werk-test: RED (exit 1)"), "{}", r.out);
}

#[test]
fn row_06_a_suite_with_zero_cases_is_unmeasured_ran_nothing() {
    let r = run_fixture("r6", "91006", &[("platform/tests/empty.bats", EMPTY)],
        &[row("platform/tests/empty.bats", "none", "test-empty")], 200);
    assert_eq!(r.code, 2, "{}", r.out);
    assert!(r.out.contains("ran nothing"), "{}", r.out);
}

#[test]
fn row_10_no_werk_is_refused_exit_3() {
    let base = std::env::temp_dir().join(format!("werk-test-4536-r10-{}", std::process::id()));
    let r = run_in(&base, "91010", &[], 200);
    assert_eq!(r.code, 3, "{}", r.out);
    assert!(r.out.contains("REFUSED — werk not found"), "{}", r.out);
}

#[test]
fn a_card_run_writes_no_test_row() {
    // #4536 (Silas 07:55): a card run tests unmerged code, so its result is
    // not the Test's latest. Loki only; only the canonical nightly writes.
    let r = run_fixture("rc", "91012", &[("platform/tests/fail.bats", FAIL)],
        &[row("platform/tests/fail.bats", "f", "test-fail")], 200);
    assert_eq!(r.code, 1, "{}", r.out);
    assert!(r.patches.is_empty(), "a card run wrote a Test row: {:?}", r.patches);
}

/// The nightly against a temp root holding `files`, with the door stubbed.
fn run_nightly(root_name: &str, files: &[(&str, &str)], registry_rows: &[String], patch_status: u16) -> Run {
    let base = std::env::temp_dir().join(format!("werk-test-4536-n-{}-{}", root_name.replace('/', "_"), std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join(root_name);
    std::fs::create_dir_all(root.join("platform/scripts")).unwrap();
    for (path, body) in files {
        let p = root.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, body).unwrap();
    }
    let registry = format!("{{\"data\":[{}]}}", registry_rows.join(","));
    let (port, patches) = stub(registry, patch_status);
    let door = format!("http://127.0.0.1:{}", port);
    let out = Command::new(env!("CARGO_BIN_EXE_werk-test"))
        .arg("--nightly")
        .env("PATH", stub_path(&base))
        .env("CHORUS_ROOT", &root)
        .env_remove("CHORUS_HOME")
        .env("CHORUS_WRITE_TOKEN", "fixture")
        .env("OWL_API_TESTS", format!("{door}/tests?limit=10000"))
        .env("OWL_API_TESTSUITERUNS", format!("{door}/testsuiteruns"))
        .env("WERK_STACK_PROBES", format!("door={door}/"))
        .env("NIGHTLY_SUITE_WORKERS", "1")
        .env("NIGHTLY_LOAD_CAP", "100000")
        .output()
        .expect("spawn werk-test --nightly");
    let _ = std::fs::remove_dir_all(&base);
    let written = patches.lock().unwrap().clone();
    Run {
        code: out.status.code().unwrap_or(-1),
        out: format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)),
        patches: written,
    }
}

#[test]
fn row_09_a_refused_store_never_changes_the_nightly_verdict() {
    let r = run_nightly("chorus", &[("platform/tests/pass.bats", PASS)],
        &[row("platform/tests/pass.bats", "p", "test-pass")], 403);
    assert_eq!(r.patches.len(), 1, "the flipped case was written once: {:?}\n{}", r.patches, r.out);
    assert!(r.patches[0].starts_with("PATCH /tests/test-pass "), "{:?}", r.patches);
    assert!(r.out.contains("results not stored: 1 of 1"), "{}", r.out);
    assert!(!r.out.contains("werk-test: RED"), "storage is not the product: {}", r.out);
}

#[test]
fn the_nightly_writes_only_the_cases_that_flipped() {
    let prior = "{\"filePath\":\"platform/tests/pass.bats\",\"covers\":\"werk\",\"testType\":\"unit\",\
                 \"testName\":\"p\",\"name\":\"test-pass\",\"hermeticity\":\"hermetic\",\"result\":\"pass\"}".to_string();
    let r = run_nightly("chorus", &[("platform/tests/pass.bats", PASS)], &[prior], 200);
    assert!(r.patches.is_empty(), "nothing flipped, nothing written: {:?}\n{}", r.patches, r.out);
}

#[test]
fn a_werk_root_nightly_writes_nothing_even_pointed_at_a_door() {
    // NEGATIVE PROOF: OWL_API_TESTS points at a live door and the case flipped;
    // the werk root alone withholds it.
    let r = run_nightly("chorus-werk/kade-0000", &[("platform/tests/pass.bats", PASS)],
        &[row("platform/tests/pass.bats", "p", "test-pass")], 200);
    assert!(r.patches.is_empty(), "a werk nightly wrote a Test row: {:?}\n{}", r.patches, r.out);
    assert!(r.out.contains("only the canonical nightly writes Test rows"), "{}", r.out);
}
