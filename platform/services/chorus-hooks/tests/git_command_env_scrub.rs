//! #2589 — git-spawn env-scrub helper regression test.
//!
//! Under per-role worktrees, parent processes (pre-commit hook) set GIT_DIR
//! to the worktree's `.git` pointer-file. Spawned `git` inherits this env
//! and resolves paths against the wrong index — silently producing empty
//! output. The git_command() helper returns a Command with GIT_DIR /
//! GIT_INDEX_FILE / GIT_WORK_TREE removed so the spawned git resolves
//! based on cwd alone.

use chorus_hooks::shared::git_command::git_command;

#[test]
fn helper_removes_git_dir_from_spawned_command() {
    // Set a sticky GIT_DIR that would corrupt git lookups if inherited.
    std::env::set_var("GIT_DIR", "/tmp/nonexistent-git-dir-from-test");

    // The helper must build a Command that strips GIT_DIR.
    let mut cmd = git_command();
    cmd.args(["rev-parse", "--show-toplevel"])
        .current_dir(env!("CARGO_MANIFEST_DIR"));
    let output = cmd.output().expect("git rev-parse must run");

    // Cleanup before assertion to avoid leaking sticky state.
    std::env::remove_var("GIT_DIR");

    // If GIT_DIR were honored, git would error with "fatal: Not a git
    // repository". The helper must scrub it so cwd-based resolution wins.
    assert!(
        output.status.success(),
        "git rev-parse should succeed (helper must remove sticky GIT_DIR); stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("chorus"),
        "rev-parse output should resolve to the chorus repo: got {:?}",
        stdout
    );
}

#[test]
fn helper_removes_git_index_file_too() {
    std::env::set_var("GIT_INDEX_FILE", "/tmp/nonexistent-index-from-test");
    let mut cmd = git_command();
    cmd.args(["status", "--porcelain"])
        .current_dir(env!("CARGO_MANIFEST_DIR"));
    let output = cmd.output().expect("git status must run");
    std::env::remove_var("GIT_INDEX_FILE");

    assert!(
        output.status.success(),
        "git status should succeed (helper must remove sticky GIT_INDEX_FILE); stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn helper_removes_git_work_tree_too() {
    std::env::set_var("GIT_WORK_TREE", "/tmp/nonexistent-work-tree-from-test");
    let mut cmd = git_command();
    cmd.args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(env!("CARGO_MANIFEST_DIR"));
    let output = cmd.output().expect("git rev-parse must run");
    std::env::remove_var("GIT_WORK_TREE");

    assert!(
        output.status.success(),
        "git rev-parse should succeed (helper must remove sticky GIT_WORK_TREE); stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
