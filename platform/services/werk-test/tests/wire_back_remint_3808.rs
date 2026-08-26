//! Batch wire-back against a REAL HTTP door (hermetic stub): an expired token
//! re-mints once and retries the same atomic chunk; a token the door still
//! refuses fails every entity in that chunk loudly.
//!
//! The stub speaks just enough HTTP: Authorization carrying "fresh" → 201,
//! anything else → 401. That is the exact behaviour of the live door on
//! #3802 (2026-08-10): 594 cases posted, then chorus-identity-token's ~600s
//! cache expired and 534 posts died 401 with no recovery.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

fn read_request(stream: &mut impl Read) -> String {
    let mut data = Vec::new();
    let mut buf = [0u8; 8192];
    let mut wanted = None;
    loop {
        let n = stream.read(&mut buf).unwrap_or(0);
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
        if wanted.is_none() {
            if let Some(pos) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                let header_end = pos + 4;
                let headers = String::from_utf8_lossy(&data[..header_end]).to_ascii_lowercase();
                let content_length = headers
                    .lines()
                    .find(|line| line.starts_with("content-length:"))
                    .and_then(|line| line.split_once(':'))
                    .and_then(|(_, value)| value.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                wanted = Some(header_end + content_length);
            }
        }
        if wanted.is_some_and(|length| data.len() >= length) {
            break;
        }
    }
    String::from_utf8_lossy(&data).into_owned()
}

fn start_stub() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("stub bind");
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for c in l.incoming() {
            let Ok(mut c) = c else { continue };
            std::thread::spawn(move || {
                let req = read_request(&mut c);
                let ok = req.lines().any(|l| {
                    l.to_ascii_lowercase().starts_with("authorization:") && l.contains("fresh")
                });
                let resp = if ok {
                    "HTTP/1.1 201 Created\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                } else {
                    "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                };
                let _ = c.write_all(resp.as_bytes());
            });
        }
    });
    port
}

fn start_accepting_stub(expected: usize) -> (u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("stub bind");
    let port = listener.local_addr().unwrap().port();
    let count = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&count);
    std::thread::spawn(move || {
        for connection in listener.incoming().take(expected) {
            let Ok(mut stream) = connection else { continue };
            let _ = read_request(&mut stream);
            seen.fetch_add(1, Ordering::SeqCst);
            let _ = stream.write_all(
                b"HTTP/1.1 201 Created\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
        }
    });
    (port, count)
}

#[test]
fn expired_token_reminting_resumes_and_real_401_fails_loudly() {
    let port = start_stub();
    let endpoint = format!("http://127.0.0.1:{}/testresults/batch", port);
    let payloads: Vec<String> = (0..4).map(|i| format!("{{\"n\":{}}}", i)).collect();
    let chunks = werk_test::chunk_json_payloads(&payloads, werk_test::TESTRESULT_BATCH_MAX_BYTES).chunks;
    assert_eq!(chunks.len(), 1, "small payloads share one HTTP transaction");

    // ── AC1: token expired mid-stream (starts stale), re-mint yields a fresh
    // one → the SAME case retries and the run completes with zero failures.
    let stats = werk_test::post_results_loop(&endpoint, "stale", &chunks, &|| {
        Some("fresh".to_string())
    });
    assert_eq!(stats.posted, 4, "all cases posted after recovery: {:?}", stats);
    assert_eq!(stats.failed, 0, "{:?}", stats);
    assert_eq!(stats.remint_attempts, 1, "exactly one re-mint: {:?}", stats);
    assert_eq!(stats.chunks_attempted, 1, "retry does not fabricate a second logical chunk");

    // ── AC2 negative proof (#3734): the door refuses even the re-minted token
    // (revoked principal). One re-mint attempt, then every case counts FAILED
    // with the 401 named — never an infinite retry.
    let stats = werk_test::post_results_loop(&endpoint, "stale", &chunks, &|| {
        Some("still-stale".to_string())
    });
    assert_eq!(stats.posted, 0, "{:?}", stats);
    assert_eq!(stats.failed, 4, "{:?}", stats);
    assert_eq!(stats.remint_attempts, 1, "re-mint tried exactly once: {:?}", stats);
    assert_eq!(stats.first_fail_code.as_deref(), Some("401"));

    // ── mint itself refuses (no identity available) — same loud failure.
    let stats = werk_test::post_results_loop(&endpoint, "stale", &chunks, &|| None);
    assert_eq!(stats.posted, 0, "{:?}", stats);
    assert_eq!(stats.remint_attempts, 1, "refused mint still counts the ATTEMPT — the field measures expiry, not success: {:?}", stats);
    assert_eq!(stats.failed, 4, "{:?}", stats);
    assert_eq!(stats.first_fail_code.as_deref(), Some("401"));

    // ── healthy path untouched: fresh token from the start, no re-mint.
    let stats = werk_test::post_results_loop(&endpoint, "fresh", &chunks, &|| {
        panic!("mint must not be called when the token works")
    });
    assert_eq!(stats.posted, 4, "{:?}", stats);
    assert_eq!(stats.remint_attempts, 0, "{:?}", stats);
}

#[test]
fn two_thousand_results_complete_in_bounded_http_calls() {
    let payloads: Vec<String> = (0..2000)
        .map(|i| {
            werk_test::test_result_payload(
                "platform/services/werk-test/tests/wire_back_remint_3808.rs",
                &format!("batch_case_{}", i),
                "pass",
                &format!("test-batch-case-{}", i),
                "4000",
                "kade",
                "trace",
                1_700_000_000_123,
                i,
            )
        })
        .collect();
    let packed =
        werk_test::chunk_json_payloads(&payloads, werk_test::TESTRESULT_BATCH_MAX_BYTES);
    assert!(packed.chunks.len() <= 25, "{} chunks", packed.chunks.len());
    let (port, calls) = start_accepting_stub(packed.chunks.len());
    let endpoint = format!("http://127.0.0.1:{}/testresults/batch", port);
    let started = std::time::Instant::now();
    let stats = werk_test::post_results_loop(&endpoint, "fresh", &packed.chunks, &|| {
        panic!("healthy batch run must not remint")
    });
    assert_eq!(stats.posted, 2000, "{:?}", stats);
    assert_eq!(stats.failed, 0, "{:?}", stats);
    assert_eq!(calls.load(Ordering::SeqCst), packed.chunks.len());
    assert!(started.elapsed() < std::time::Duration::from_secs(30));
}
