//! Socket bind tests — #1939
//! Verify exclusive socket bind with orphan detection.

use std::fs;
use std::path::Path;

const SOCKET_PATH: &str = "/tmp/chorus-hooks.sock";

// #3606 — the PID file's durable home is ~/.chorus/run/ (see state_paths).
// macOS periodically evicts /tmp files not accessed for ~3 days, so a
// long-lived daemon's /tmp pid file evaporated while its active socket
// (exempt from eviction) survived — this suite then read red on any box
// where the daemon had been up >3 days, and orphan detection was genuinely
// broken. /tmp remains a best-effort mirror for `kill $(cat ...)` muscle
// memory; the durable path is the contract.
fn pid_path() -> String {
    format!(
        "{}/.chorus/run/chorus-hooks.pid",
        std::env::var("HOME").expect("HOME not set")
    )
}

#[test]
fn pid_file_exists_when_hooks_running() {
    // If chorus-hooks is running, it should have a durable PID file
    if Path::new(SOCKET_PATH).exists() {
        assert!(
            Path::new(&pid_path()).exists(),
            "Socket exists but no durable PID file — orphan detection won't work"
        );
    }
}

#[test]
fn pid_file_contains_valid_pid() {
    let p = pid_path();
    if Path::new(&p).exists() {
        let contents = fs::read_to_string(&p).expect("Failed to read PID file");
        let pid: u32 = contents.trim().parse().expect("PID file should contain a number");
        assert!(pid > 0, "PID should be positive");
    }
}

#[test]
fn stale_pid_detected_when_process_dead() {
    // Create a fake PID file with a definitely-dead PID
    let test_pid_path = "/tmp/chorus-hooks-test.pid";
    fs::write(test_pid_path, "999999999").unwrap();

    // Check if process is alive using kill -0
    let output = std::process::Command::new("kill")
        .args(["-0", "999999999"])
        .output()
        .expect("Failed to run kill");

    assert!(
        !output.status.success(),
        "PID 999999999 should not be alive"
    );

    fs::remove_file(test_pid_path).ok();
}

/// #2559: clean shutdown must remove BOTH socket and pid files.
/// shutdown_signal previously removed only the socket, leaving a stale PID
/// file that scripts running `kill $(cat /tmp/chorus-hooks.pid)` could
/// target against a recycled-PID unrelated process. Symmetry restored via
/// a shared `cleanup_runtime_files` helper.
#[test]
fn cleanup_runtime_files_removes_both_socket_and_pid() {
    let dir = std::env::temp_dir().join(format!(
        "chorus-hooks-cleanup-{}-both",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    let socket_path = dir.join("test.sock");
    let pid_path = dir.join("test.pid");
    fs::write(&socket_path, "").unwrap();
    fs::write(&pid_path, "12345").unwrap();

    chorus_hooks::cleanup_runtime_files(&socket_path, &pid_path);

    assert!(
        !socket_path.exists(),
        "socket should be removed (already worked pre-#2559)"
    );
    assert!(
        !pid_path.exists(),
        "pid file should be removed by cleanup — #2559 fix"
    );

    let _ = fs::remove_dir(&dir);
}

/// Cleanup must be idempotent — calling on already-missing files is a no-op,
/// not a panic. Matches `let _ = fs::remove_file(...)` discard pattern.
#[test]
fn cleanup_runtime_files_idempotent_when_missing() {
    let dir = std::env::temp_dir().join(format!(
        "chorus-hooks-cleanup-{}-missing",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    let socket_path = dir.join("nonexistent.sock");
    let pid_path = dir.join("nonexistent.pid");

    chorus_hooks::cleanup_runtime_files(&socket_path, &pid_path);

    let _ = fs::remove_dir(&dir);
}
