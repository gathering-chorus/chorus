//! #4030 AC3 — results are stored PER UNIT, so a run killed mid-way keeps
//! everything it finished. Negative proof (#3734): the violating state — the
//! runner dies while a later suite is still running — is produced for real
//! (SIGKILL), and the store has already received the finished suite's
//! results. Under the old one-batch-at-the-end write-back this test reads
//! `stored 0`, which is exactly what 2026-08-30 03:00 showed Jeff: 1,127
//! cases executed, none saved, after the 7200s lane cap killed the run.
//!
//! Hermetic: the tests domain and the results door are one std TcpListener
//! stub; the "suites" are two throwaway bats files in a temp root. Needs
//! `bats` and `jq` on PATH — the same tools the nightly itself needs.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
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
                let header_end = pos + 4;
                let headers = String::from_utf8_lossy(&data[..header_end]).to_ascii_lowercase();
                let len = headers.lines()
                    .find(|l| l.starts_with("content-length:"))
                    .and_then(|l| l.split_once(':'))
                    .and_then(|(_, v)| v.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                wanted = Some(header_end + len);
            }
        }
        if wanted.is_some_and(|l| data.len() >= l) { break; }
    }
    String::from_utf8_lossy(&data).into_owned()
}

/// GET /tests → the registry fixture; POST /testresults/batch → 201 and the
/// body is kept; anything else → 201 (suite-run posts are best-effort).
fn start_stub(registry: String) -> (u16, Arc<AtomicUsize>, Arc<Mutex<Vec<String>>>) {
    let l = TcpListener::bind("127.0.0.1:0").expect("stub bind");
    let port = l.local_addr().unwrap().port();
    let posts = Arc::new(AtomicUsize::new(0));
    let bodies = Arc::new(Mutex::new(Vec::new()));
    let (p2, b2) = (Arc::clone(&posts), Arc::clone(&bodies));
    std::thread::spawn(move || {
        for c in l.incoming() {
            let Ok(mut c) = c else { continue };
            let req = read_request(&mut c);
            let first = req.lines().next().unwrap_or("").to_string();
            let resp = if first.starts_with("GET ") && first.contains("/tests") {
                format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    registry.len(), registry)
            } else {
                if first.starts_with("POST ") && first.contains("/testresults/batch") {
                    p2.fetch_add(1, Ordering::SeqCst);
                    b2.lock().unwrap().push(req.clone());
                }
                "HTTP/1.1 201 Created\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
            };
            let _ = c.write_all(resp.as_bytes());
        }
    });
    (port, posts, bodies)
}

fn row(file: &str, test: &str, entity: &str) -> String {
    format!("{{\"filePath\":\"{file}\",\"covers\":\"chorus:nightly\",\"pyramidLayer\":\"unit\",\
             \"testName\":\"{test}\",\"name\":\"{entity}\",\"hermeticity\":\"hermetic\",\"testConcern\":\"\"}}")
}

#[test]
fn a_run_killed_mid_way_has_already_stored_the_suites_it_finished() {
    for tool in ["bats", "jq"] {
        assert!(std::process::Command::new("which").arg(tool).output().map(|o| o.status.success()).unwrap_or(false),
            "{tool} must be on PATH — the nightly needs it and so does this proof");
    }
    let root = std::env::temp_dir().join(format!("werk-test-4030-root-{}", std::process::id()));
    let suites = root.join("platform/tests");
    std::fs::create_dir_all(&suites).unwrap();
    std::fs::create_dir_all(root.join("platform/scripts")).unwrap();
    // registry order = plan order (sorted by path): "fast" runs before "slow"
    std::fs::write(suites.join("fast-4030.bats"),
        "#!/usr/bin/env bats\n@test \"fast passes\" {\n  true\n}\n").unwrap();
    std::fs::write(suites.join("slow-4030.bats"),
        "#!/usr/bin/env bats\n@test \"slow never finishes\" {\n  sleep 97\n}\n").unwrap();
    let registry = format!("{{\"data\":[{},{}]}}",
        row("platform/tests/fast-4030.bats", "fast passes", "test-fast-4030"),
        row("platform/tests/slow-4030.bats", "slow never finishes", "test-slow-4030"));
    let (port, posts, bodies) = start_stub(registry);
    let base = format!("http://127.0.0.1:{}", port);

    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_werk-test"))
        .arg("--nightly")
        .env("CHORUS_ROOT", &root)
        .env_remove("CHORUS_HOME")            // no spine, no token mint: the door takes the env token
        .env("CHORUS_WRITE_TOKEN", "fresh")
        .env("OWL_API_TESTS", format!("{base}/tests?limit=10000"))
        .env("OWL_API_TESTRESULTS_BATCH", format!("{base}/testresults/batch"))
        .env("OWL_API_TESTSUITERUNS", format!("{base}/testsuiteruns"))
        .env("NIGHTLY_SUITE_WORKERS", "1")    // fast, then slow — one at a time
        .env("NIGHTLY_LOAD_CAP", "100000")    // the load gate is not under test
        .env("NIGHTLY_SUITE_TIMEOUT", "600")  // the slow suite outlives this test on purpose
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn werk-test --nightly");

    // wait for the fast suite's results to land — under per-unit storage that
    // happens while the slow suite is still sleeping
    let started = std::time::Instant::now();
    while posts.load(Ordering::SeqCst) == 0 && started.elapsed() < std::time::Duration::from_secs(90) {
        if let Ok(Some(_)) = child.try_wait() { break; }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    let still_running = matches!(child.try_wait(), Ok(None));
    // the violating state: the runner dies with a suite in flight
    let _ = child.kill();
    let _ = child.wait();
    // reap the sleeper the runner no longer owns (it lives in its own group)
    let _ = std::process::Command::new("pkill").args(["-f", "sleep 97"]).status();
    let _ = std::fs::remove_dir_all(&root);

    assert!(still_running, "the runner must have been mid-run (slow suite in flight) when killed");
    assert!(posts.load(Ordering::SeqCst) >= 1,
        "the finished suite's results must already be in the store when the run dies — got 0 posts");
    let all = bodies.lock().unwrap().join("\n");
    assert!(all.contains("test-fast-4030"), "the stored result carries the fast suite's registered identity:\n{all}");
    assert!(!all.contains("test-slow-4030"), "nothing was fabricated for the suite that never finished");
}
