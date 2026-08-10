//! #3808 — the wire-back posting loop against a REAL HTTP door (hermetic stub):
//! an expired token mid-stream re-mints once and resumes; a token the door
//! still refuses after re-mint fails loudly for the rest of the run.
//!
//! The stub speaks just enough HTTP: Authorization carrying "fresh" → 201,
//! anything else → 401. That is the exact behaviour of the live door on
//! #3802 (2026-08-10): 594 cases posted, then chorus-identity-token's ~600s
//! cache expired and 534 posts died 401 with no recovery.

use std::io::{Read, Write};
use std::net::TcpListener;

fn start_stub() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("stub bind");
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for c in l.incoming() {
            let Ok(mut c) = c else { continue };
            std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
                let n = c.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
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

#[test]
fn expired_token_reminting_resumes_and_real_401_fails_loudly() {
    let port = start_stub();
    let endpoint = format!("http://127.0.0.1:{}/testresults", port);
    let payloads: Vec<String> = (0..4).map(|i| format!("{{\"n\":{}}}", i)).collect();

    // ── AC1: token expired mid-stream (starts stale), re-mint yields a fresh
    // one → the SAME case retries and the run completes with zero failures.
    let stats = werk_test::post_results_loop(&endpoint, "stale", &payloads, &|| {
        Some("fresh".to_string())
    });
    assert_eq!(stats.posted, 4, "all cases posted after recovery: {:?}", stats);
    assert_eq!(stats.failed, 0, "{:?}", stats);
    assert_eq!(stats.remint_attempts, 1, "exactly one re-mint: {:?}", stats);

    // ── AC2 negative proof (#3734): the door refuses even the re-minted token
    // (revoked principal). One re-mint attempt, then every case counts FAILED
    // with the 401 named — never an infinite retry.
    let stats = werk_test::post_results_loop(&endpoint, "stale", &payloads, &|| {
        Some("still-stale".to_string())
    });
    assert_eq!(stats.posted, 0, "{:?}", stats);
    assert_eq!(stats.failed, 4, "{:?}", stats);
    assert_eq!(stats.remint_attempts, 1, "re-mint tried exactly once: {:?}", stats);
    assert_eq!(stats.first_fail_code.as_deref(), Some("401"));

    // ── mint itself refuses (no identity available) — same loud failure.
    let stats = werk_test::post_results_loop(&endpoint, "stale", &payloads, &|| None);
    assert_eq!(stats.posted, 0, "{:?}", stats);
    assert_eq!(stats.remint_attempts, 1, "refused mint still counts the ATTEMPT — the field measures expiry, not success: {:?}", stats);
    assert_eq!(stats.failed, 4, "{:?}", stats);
    assert_eq!(stats.first_fail_code.as_deref(), Some("401"));

    // ── healthy path untouched: fresh token from the start, no re-mint.
    let stats = werk_test::post_results_loop(&endpoint, "fresh", &payloads, &|| {
        panic!("mint must not be called when the token works")
    });
    assert_eq!(stats.posted, 4, "{:?}", stats);
    assert_eq!(stats.remint_attempts, 0, "{:?}", stats);
}
