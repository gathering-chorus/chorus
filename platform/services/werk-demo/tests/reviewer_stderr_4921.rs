// @test-type: unit — a fake claude script on disk; no model, no network.
// @domain: cicd
// @card: #4921
//! #4921: a reviewer that crashes leaves its real error in the finding. On 11-07
//! 12:08 gate-quality's claude exited 1 under jeffbridwell and the finding kept
//! only the first 160 chars of stderr — "Permission allow rule 'NotebookEdit(*)' …
//! Older vers". At 12:33, with all of stderr kept, stderr held ONLY that warning
//! (285 bytes): claude -p prints its error on stdout, which the finding dropped.
use std::os::unix::fs::PermissionsExt;
use werk_demo::{crashed_reviewer, review_once};

const WARNING: &str = "Permission allow rule \"NotebookEdit(*)\" lets NotebookEdit run without asking only in the working directories. \"NotebookEdit\" alone allows every path. Older versions of Claude Code treat \"NotebookEdit(*)\" as \"NotebookEdit\", and this version will too if Anthropic turns this change off.";

fn fake_claude(dir: &std::path::Path, stdout: &str, stderr: &str, code: i32) -> std::path::PathBuf {
    let p = dir.join("claude");
    let body = format!("#!/bin/sh\ncat >/dev/null\ncat <<'EOF'\n{stdout}\nEOF\ncat >&2 <<'EOF'\n{stderr}\nEOF\nexit {code}\n");
    std::fs::write(&p, body).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}

#[test]
fn a_crashed_claude_names_its_real_error_on_either_stream() {
    let dir = std::env::temp_dir().join(format!("wd-4921-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::env::remove_var("CHORUS_GATE_PROFILE");
    std::env::set_var("CHORUS_GATE_TIMEOUT_SECS", "20");

    // 12:33 shape: the warning alone on stderr, the error on stdout
    std::env::set_var("CHORUS_CLAUDE_BIN", fake_claude(&dir, "Error: rate limited", WARNING, 1));
    let (verdict, finding) = review_once("sys", "ctx", "round-1", &[], "");
    assert_eq!(verdict, "error");
    assert!(finding.contains("rate limited"), "finding lost the stdout error: {finding}");
    assert!(finding.starts_with("claude as ") && finding.contains(" exit Some(1): "), "{finding}");

    // 12:08 shape: a long warning, then the error, both on stderr
    let stderr = format!("{WARNING} {}\nError: auth expired", "x".repeat(200));
    assert!(!stderr.chars().take(160).collect::<String>().contains("auth expired"), "fixture must defeat the old cut");
    std::env::set_var("CHORUS_CLAUDE_BIN", fake_claude(&dir, "", &stderr, 1));
    let (_, finding) = review_once("sys", "ctx", "round-1", &[], "");
    std::fs::remove_dir_all(&dir).ok();
    assert!(finding.contains("auth expired"), "finding lost the stderr error: {finding}");
    assert!(finding.contains("stdout: (empty)"), "an empty stream is named, not hidden: {finding}");
}

#[test]
fn the_finding_keeps_each_tail_and_marks_the_cut() {
    let long = format!("{WARNING}\n{}\nError: the real one", "y".repeat(900));
    let f = crashed_reviewer("claude", "exit Some(1)", &long, "");
    assert!(f.ends_with("Error: the real one"), "{f}");
    assert!(f.contains("stderr: … "), "a cut stream must say it was cut: {f}");
    assert!(!f.contains("NotebookEdit"), "the head is dropped, not the tail: {f}");

    let short = crashed_reviewer("chorus-agent", "failed", "Error: boom\n", "partial reply");
    assert_eq!(short, "chorus-agent failed: stdout: partial reply · stderr: Error: boom");
}
