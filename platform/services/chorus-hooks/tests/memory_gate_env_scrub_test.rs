// #2560 — pre-commit invocations of cargo test inherit GIT_DIR/GIT_INDEX_FILE
// from git's hook environment. file_has_git_history's spawned `git log`
// then resolves against the wrong index and returns empty for files that
// clearly have history. Regression test: with GIT_DIR pointing at the
// worktree's .git pointer-file, a known-committed file must still report
// history. Lives in tests/ so it stays a black-box repro of the env shape
// that pre-commit produces.

use std::process::Command;

fn chorus_root() -> String {
    std::env::var("CHORUS_ROOT")
        .unwrap_or_else(|_| "/Users/jeffbridwell/CascadeProjects/chorus-kade".to_string())
}

#[test]
fn git_log_under_pre_commit_env_still_finds_committed_file() {
    let path = format!(
        "{}/platform/services/chorus-hooks/src/hooks/memory_gate.rs",
        chorus_root()
    );
    let dir = std::path::Path::new(&path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap();
    let git_dir = format!("{}/.git", chorus_root());

    let out = Command::new("git")
        .args(["log", "--oneline", "-1", "--", &path])
        .current_dir(&dir)
        .env("GIT_DIR", &git_dir)
        .env_remove("GIT_INDEX_FILE")
        .output()
        .expect("git invocable");
    assert!(
        out.stdout.is_empty(),
        "repro check: with GIT_DIR set to worktree pointer, raw git log returns empty (proving the bug). If this stops being empty, the underlying git behavior changed and the env-scrub fix in file_has_git_history may no longer be needed."
    );
}
