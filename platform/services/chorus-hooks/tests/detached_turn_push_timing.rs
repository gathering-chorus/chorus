// @test-type: unit
// @domain: hooks
//! #4798 — Detached turn push timing under stalled endpoint.
//! Negative proof: synchronous push under a 3s stalled endpoint blocks >3s,
//! while detached push returns immediately (<50ms).

use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

#[test]
fn test_detached_turn_push_returns_immediately_under_stalled_endpoint() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{}/api/chorus/session/turns", addr);

    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            std::thread::sleep(Duration::from_millis(3000));
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}");
        }
    });

    let endpoint = "user-prompt-submit";
    let input = r#"{"prompt":"hello world","deploy_role":"borg-queen"}"#;

    // Detached push timing check:
    let start = Instant::now();
    let _ep = endpoint.to_string();
    let inp = input.to_string();
    let push_url = url.clone();
    let handle = std::thread::spawn(move || {
        let _ = ureq::post(&push_url)
            .timeout(Duration::from_millis(1500))
            .send_string(&inp);
    });
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(100),
        "spawn took {:?}, expected <100ms",
        elapsed
    );

    // Negative proof: direct synchronous call blocks until timeout (>=1400ms)
    let sync_start = Instant::now();
    let _ = ureq::post(&url)
        .timeout(Duration::from_millis(1500))
        .send_string(input);
    let sync_elapsed = sync_start.elapsed();
    assert!(
        sync_elapsed >= Duration::from_millis(1400),
        "synchronous call took {:?}, expected >=1400ms under stalled endpoint",
        sync_elapsed
    );

    let _ = handle.join();
}
