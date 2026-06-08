//! #3278 — chorus.log append atomicity (Mechanism 1 of the error-undercount card).
//!
//! RED on the two-write append (`write_all(line)` then `write_all("\n")`): under
//! concurrency another task's append lands between the two writes, fusing two JSON
//! objects onto one line (~6% of chorus.log, measured 2026-06-07). GREEN once the
//! line+newline is a single atomic O_APPEND write. 300-byte padding + 200 racers
//! makes the race fire reliably.

// AC1: chorus.log appends are atomic — concurrent writers never interleave.
use chorus_hooks::append_log;

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concurrent_appends_never_interleave() {
    let path = std::env::temp_dir().join(format!("chorus-append-3278-{}.log", std::process::id()));
    let _ = std::fs::remove_file(&path);

    let n: usize = 200;
    let mut handles = Vec::new();
    for i in 0..n {
        let p = path.clone();
        handles.push(tokio::spawn(async move {
            let line = serde_json::json!({ "i": i, "pad": "x".repeat(300) }).to_string();
            append_log(&p, &line).await;
        }));
    }
    for h in handles {
        let _ = h.await;
    }

    let content = std::fs::read_to_string(&path).unwrap_or_default();
    let lines: Vec<&str> = content.lines().filter(|l| !l.is_empty()).collect();
    let bad = lines
        .iter()
        .filter(|l| serde_json::from_str::<serde_json::Value>(l).is_err())
        .count();
    let _ = std::fs::remove_file(&path);

    assert_eq!(bad, 0, "{} of {} lines were corrupted (fused/truncated by interleaving)", bad, lines.len());
    assert_eq!(lines.len(), n, "expected {} clean lines, got {}", n, lines.len());
}
