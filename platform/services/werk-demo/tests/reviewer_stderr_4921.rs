// @test-type: unit — a fake claude script on disk; no model, no network.
// @domain: cicd
// @card: #4921
//! #4921: a reviewer that crashes leaves its real error in the finding. On 11-07
//! 12:08 gate-quality's claude exited 1 under jeffbridwell and the finding kept
//! only the first 160 chars of stderr — "Permission allow rule 'NotebookEdit(*)' …
//! Older vers" — a settings warning, so nobody could see why it crashed.
use std::os::unix::fs::PermissionsExt;
use werk_demo::{crashed_reviewer, review_once};

fn warning() -> String {
    format!("Permission allow rule 'NotebookEdit(*)' is invalid. Older versions {}", "x".repeat(240))
}

fn fake_claude(dir: &std::path::Path, stderr: &str, code: i32) -> std::path::PathBuf {
    let p = dir.join("claude");
    let body = format!("#!/bin/sh\ncat >/dev/null\ncat >&2 <<'EOF'\n{stderr}\nEOF\nexit {code}\n");
    std::fs::write(&p, body).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}

#[test]
fn a_crashed_claude_names_its_last_error_not_its_first_warning() {
    let dir = std::env::temp_dir().join(format!("wd-4921-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let stderr = format!("{}\nError: rate limited", warning());
    // negative proof of the fixture: the old 160-char cut cannot see the error
    assert!(stderr.len() > 300);
    assert!(!stderr.chars().take(160).collect::<String>().contains("rate limited"));

    let bin = fake_claude(&dir, &stderr, 1);
    std::env::remove_var("CHORUS_GATE_PROFILE");
    std::env::set_var("CHORUS_CLAUDE_BIN", &bin);
    std::env::set_var("CHORUS_GATE_TIMEOUT_SECS", "20");
    let (verdict, finding) = review_once("sys", "ctx", "round-1", &[]);
    std::fs::remove_dir_all(&dir).ok();

    assert_eq!(verdict, "error");
    assert!(finding.contains("rate limited"), "finding lost the real error: {finding}");
    assert!(finding.starts_with("claude exit Some(1): "), "{finding}");
}

#[test]
fn the_finding_keeps_the_tail_and_marks_the_cut() {
    let long = format!("{}\n{}\nError: the real one", warning(), "y".repeat(900));
    let f = crashed_reviewer("claude", "exit Some(1)", &long);
    assert!(f.ends_with("Error: the real one"), "{f}");
    assert!(f.contains("… "), "a cut finding must say it was cut: {f}");
    assert!(!f.contains("NotebookEdit"), "the head is dropped, not the tail: {f}");

    let short = crashed_reviewer("chorus-agent", "failed", "Error: boom\n");
    assert_eq!(short, "chorus-agent failed: Error: boom");
}
