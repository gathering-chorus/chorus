//! Chat behavioral test suite (#2283)
//!
//! Tests real chat.sh behavior: file creation, message append, tick marker,
//! atomic read, and cleanup. Every test sends real commands to the real script.

use std::fs;
use std::process::Command;

const CHAT_SCRIPT: &str = "/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chat.sh";
const CHAT_DIR: &str = "/tmp/chorus-chat";

fn chat_id(from: &str, to: &str, ts: u64) -> String {
    format!("{}-{}-{}", from, to, ts)
}

fn run_chat(args: &[&str]) -> (String, String, bool) {
    let out = Command::new("bash")
        .arg(CHAT_SCRIPT)
        .args(args)
        .env("CHAT_DRY_RUN", "1") // skip nudge delivery in tests
        .output()
        .expect("chat.sh must run");
    let stdout = String::from_utf8_lossy(&out.stdout);
    // start emits a spine event line then the ID on the last line — take last non-empty line
    let last_line = stdout.lines().filter(|l| !l.trim().is_empty()).last()
        .unwrap_or("").trim().to_string();
    (
        last_line,
        String::from_utf8_lossy(&out.stderr).trim().to_string(),
        out.status.success(),
    )
}

/// start creates the chat file and returns a valid chat ID.
#[test]
fn start_creates_chat_file() {
    let (id, _, ok) = run_chat(&["start", "silas", "wren", "test-topic"]);
    assert!(ok, "chat.sh start must succeed");
    assert!(!id.is_empty(), "start must return a chat ID");

    let file = format!("{}/{}.md", CHAT_DIR, id);
    assert!(fs::metadata(&file).is_ok(), "chat file must exist at {}", file);

    let content = fs::read_to_string(&file).unwrap();
    assert!(content.contains("test-topic"), "chat file must contain the topic");
    assert!(content.contains("silas"), "chat file must contain from-role");
    assert!(content.contains("wren"), "chat file must contain to-role");

    let _ = fs::remove_file(&file);
}

/// say appends a message and returns the line count.
#[test]
fn say_appends_message_and_returns_line_count() {
    let (id, _, _) = run_chat(&["start", "silas", "wren", "say-test"]);
    let (line_str, _, ok) = run_chat(&["say", &id, "silas", "hello from silas"]);
    assert!(ok, "chat.sh say must succeed");

    let line_count: usize = line_str.parse().expect("say must return a line count number");
    assert!(line_count > 0, "line count must be > 0");

    let content = fs::read_to_string(format!("{}/{}.md", CHAT_DIR, id)).unwrap();
    assert!(content.contains("hello from silas"), "message must appear in chat file");

    let _ = fs::remove_file(format!("{}/{}.md", CHAT_DIR, id));
}

/// say writes the tick marker on first call. Marker contains line count and other role.
#[test]
fn say_writes_tick_marker() {
    let (id, _, _) = run_chat(&["start", "silas", "wren", "tick-test"]);
    let tick_file = format!("{}/tick-{}", CHAT_DIR, id);
    let _ = fs::remove_file(&tick_file); // clean slate

    run_chat(&["say", &id, "silas", "trigger tick marker"]);

    assert!(fs::metadata(&tick_file).is_ok(), "tick marker must be written by say");

    let content = fs::read_to_string(&tick_file).unwrap();
    let parts: Vec<&str> = content.trim().split('|').collect();
    assert_eq!(parts.len(), 2, "tick marker must be line_count|other_role, got: {}", content);

    let line_count: usize = parts[0].parse().expect("first field must be a line count");
    assert!(line_count > 0, "tick marker line count must be > 0");
    assert_eq!(parts[1], "wren", "tick marker other role must be wren");

    let _ = fs::remove_file(format!("{}/{}.md", CHAT_DIR, id));
    let _ = fs::remove_file(&tick_file);
}

/// say does NOT overwrite the tick marker on subsequent calls.
#[test]
fn say_tick_marker_not_overwritten() {
    let (id, _, _) = run_chat(&["start", "silas", "wren", "tick-idempotent"]);
    let tick_file = format!("{}/tick-{}", CHAT_DIR, id);

    run_chat(&["say", &id, "silas", "first message"]);
    let first_content = fs::read_to_string(&tick_file).unwrap();

    run_chat(&["say", &id, "silas", "second message"]);
    let second_content = fs::read_to_string(&tick_file).unwrap();

    assert_eq!(
        first_content, second_content,
        "tick marker must not be overwritten on subsequent say calls — line count anchors the cron tick"
    );

    let _ = fs::remove_file(format!("{}/{}.md", CHAT_DIR, id));
    let _ = fs::remove_file(&tick_file);
}

/// read --since N returns only lines after N.
#[test]
fn read_since_returns_new_lines_only() {
    let (id, _, _) = run_chat(&["start", "silas", "wren", "read-since-test"]);

    let (line_str, _, _) = run_chat(&["say", &id, "silas", "first"]);
    let after_first: usize = line_str.parse().unwrap();

    run_chat(&["say", &id, "wren", "second"]);

    let (content, _, ok) = run_chat(&["read", &id, "--since", &after_first.to_string()]);
    assert!(ok, "read --since must succeed");
    assert!(!content.contains("first"), "read --since must not return earlier messages");
    assert!(content.contains("second"), "read --since must return newer messages");

    let _ = fs::remove_file(format!("{}/{}.md", CHAT_DIR, id));
    let _ = fs::remove_file(format!("{}/tick-{}", CHAT_DIR, id));
}

/// end closes the chat and deletes the tick marker.
#[test]
fn end_deletes_tick_marker() {
    let (id, _, _) = run_chat(&["start", "silas", "wren", "end-test"]);
    run_chat(&["say", &id, "silas", "a message"]);

    let tick_file = format!("{}/tick-{}", CHAT_DIR, id);
    assert!(fs::metadata(&tick_file).is_ok(), "tick marker must exist before end");

    let (out, _, ok) = run_chat(&["end", &id]);
    assert!(ok, "chat.sh end must succeed");
    assert!(out.contains("ended"), "end must confirm chat closed");

    assert!(
        !fs::metadata(&tick_file).is_ok(),
        "tick marker must be deleted by end"
    );

    let _ = fs::remove_file(format!("{}/{}.md", CHAT_DIR, id));
}
