//! Real end-to-end for the commit-only verb (#3056 split): actual `git` on temp
//! repos. Proves werk-commit COMMITS LOCALLY and does NOT push — push is now the
//! separate werk-push verb. RED against the old bundled code (which pushed); GREEN
//! once commit() is refactored to commit-only.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use werk_commit::commit;

fn nanos() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
}
fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("wc-{}-{}-{}", tag, std::process::id(), nanos()));
    fs::create_dir_all(&p).unwrap();
    p
}
fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t")
        .status().unwrap().success();
    assert!(ok, "git {:?} failed in {}", args, dir.display());
}
fn remote_has(home: &Path, branch: &str) -> bool {
    let out = Command::new("git")
        .args(["-C", home.to_str().unwrap(), "ls-remote", "--heads", "origin", branch])
        .output().unwrap();
    String::from_utf8_lossy(&out.stdout).contains(branch)
}

#[test]
fn commit_only_commits_locally_does_not_push() {
    // origin (with a main commit) + home clone + a card worktree.
    let origin = tmp("origin");
    git(&origin, &["init", "-q", "-b", "main", "."]);
    fs::write(origin.join("README"), "x").unwrap();
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-q", "-m", "init"]);
    git(&origin, &["config", "receive.denyCurrentBranch", "ignore"]);

    let home = tmp("home");
    assert!(Command::new("git")
        .args(["clone", "-q", origin.to_str().unwrap(), home.to_str().unwrap()])
        .status().unwrap().success());

    // card worktree on kade/9001 off origin/main, with a change to commit.
    let werk_base = tmp("werk");
    let werk = werk_base.join("kade-9001");
    git(&home, &["worktree", "add", "-b", "kade/9001", werk.to_str().unwrap(), "origin/main"]);
    fs::write(werk.join("new.txt"), "work").unwrap();

    let sha = commit(9001, "kade", "atomic commit test", &home, &werk_base).expect("commit ok");

    // committed LOCALLY: werk HEAD is one ahead of origin/main, with our file.
    let ahead = Command::new("git")
        .args(["-C", werk.to_str().unwrap(), "rev-list", "--count", "origin/main..HEAD"])
        .output().unwrap();
    assert_eq!(String::from_utf8_lossy(&ahead.stdout).trim(), "1", "one local commit");
    assert!(sha.len() >= 7, "returns the commit sha");

    // the WHOLE POINT of the split: commit does NOT push.
    assert!(!remote_has(&home, "kade/9001"), "commit-only must NOT push the branch to origin");

    // idempotent: re-run on the clean, already-committed werk returns the same sha.
    assert_eq!(commit(9001, "kade", "atomic commit test", &home, &werk_base).unwrap(), sha);
}
