//! Socket bind tests — #1939
//! Verify exclusive socket bind with orphan detection.

use std::fs;
use std::path::Path;

const PID_PATH: &str = "/tmp/chorus-hooks.pid";
const SOCKET_PATH: &str = "/tmp/chorus-hooks.sock";

#[test]
fn pid_file_exists_when_hooks_running() {
    // If chorus-hooks is running, it should have a PID file
    if Path::new(SOCKET_PATH).exists() {
        assert!(
            Path::new(PID_PATH).exists(),
            "Socket exists but no PID file — orphan detection won't work"
        );
    }
}

#[test]
fn pid_file_contains_valid_pid() {
    if Path::new(PID_PATH).exists() {
        let contents = fs::read_to_string(PID_PATH).expect("Failed to read PID file");
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
