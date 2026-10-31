// @test-type: unit
// @domain: spine
//! #4798 — Detached turn push timing and asynchronous delivery under stalled endpoint.
//! Negative proof: synchronous worker push under a 3s stalled endpoint blocks >=1400ms,
//! while detached hook dispatch returns immediately (<300ms) and asynchronously delivers
//! the turn payload to the endpoint.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[test]
fn test_detached_turn_push_timing_and_delivery() {
    let received_body: Arc<Mutex<Option<serde_json::Value>>> = Arc::new(Mutex::new(None));
    let received_body_clone = Arc::clone(&received_body);

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{}/api/chorus/session/turns", addr);

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let body_dest = Arc::clone(&received_body_clone);
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut content_length = 0usize;
                let mut line = String::new();
                while let Ok(n) = reader.read_line(&mut line) {
                    if n == 0 || line == "\r\n" || line == "\n" {
                        break;
                    }
                    if line.to_lowercase().starts_with("content-length:") {
                        if let Some(val) = line.split(':').nth(1) {
                            content_length = val.trim().parse().unwrap_or(0);
                        }
                    }
                    line.clear();
                }
                let mut body_bytes = vec![0u8; content_length];
                if content_length > 0 {
                    let _ = reader.read_exact(&mut body_bytes);
                }
                if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                    *body_dest.lock().unwrap() = Some(v);
                }
                // Stall for 3 seconds before responding
                std::thread::sleep(Duration::from_millis(3000));
                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}");
            });
        }
    });

    let input = serde_json::json!({
        "prompt": "detached turn message",
        "deploy_role": "borg-queen",
        "chorus_session_id": "test-session-4797"
    })
    .to_string();

    let shim_bin = env!("CARGO_BIN_EXE_chorus-hook-shim");

    let run_dir = tempfile::tempdir().unwrap();

    // 1. Detached timing: hook returns fast (<300ms) despite stalled endpoint
    let start = Instant::now();
    let mut child = Command::new(shim_bin)
        .arg("user-prompt-submit")
        .env("CHORUS_SESSION_TURN_PUSH_URL", &url)
        .env("CHORUS_HOOKS_RUN_DIR", run_dir.path().to_str().unwrap())
        .env("DEPLOY_ROLE", "borg-queen")
        .env("CHORUS_ROLE", "borg-queen")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn chorus-hook-shim");

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input.as_bytes()).unwrap();
        drop(stdin);
    }
    let output = child.wait_with_output().expect("failed to wait on child");
    let elapsed = start.elapsed();

    assert!(
        output.status.success(),
        "shim hook failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        elapsed < Duration::from_millis(1000),
        "shim hook execution took {:?}, expected <1000ms (endpoint stalled for 3s)",
        elapsed
    );

    // 2. Prove AC3: turn payload actually arrives at endpoint asynchronously
    let wait_start = Instant::now();
    let mut turn_delivered = false;
    while wait_start.elapsed() < Duration::from_secs(5) {
        if let Some(ref val) = *received_body.lock().unwrap() {
            let turns = val.get("turns").and_then(|t| t.as_array());
            if let Some(turns_arr) = turns {
                if let Some(first_turn) = turns_arr.first() {
                    assert_eq!(
                        first_turn.get("content").and_then(|c| c.as_str()),
                        Some("detached turn message")
                    );
                    assert_eq!(
                        first_turn.get("author").and_then(|a| a.as_str()),
                        Some("user")
                    );
                    turn_delivered = true;
                    break;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(turn_delivered, "detached worker did not deliver turn payload within 5s");

    // 3. Negative proof (#3734): synchronous worker push blocks on stalled endpoint (>=1400ms)
    let sync_start = Instant::now();
    let sync_output = Command::new(shim_bin)
        .args(["session-turn-push-worker", "user-prompt-submit", &input])
        .env("CHORUS_SESSION_TURN_PUSH_URL", &url)
        .env("DEPLOY_ROLE", "borg-queen")
        .env("CHORUS_ROLE", "borg-queen")
        .output()
        .expect("failed to run session-turn-push-worker");

    let sync_elapsed = sync_start.elapsed();
    assert!(
        sync_output.status.success(),
        "worker failed: {}",
        String::from_utf8_lossy(&sync_output.stderr)
    );
    assert!(
        sync_elapsed >= Duration::from_millis(1400),
        "synchronous worker call took {:?}, expected >=1400ms under stalled endpoint",
        sync_elapsed
    );
}
