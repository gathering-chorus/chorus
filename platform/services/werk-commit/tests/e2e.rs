//! Real end-to-end for the commit-only verb (#3056 split): actual `git` on temp
//! repos. Proves werk-commit COMMITS LOCALLY and does NOT push — push is now the
//! separate werk-push verb. RED against the old bundled code (which pushed); GREEN
//! once commit() is refactored to commit-only.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use werk_commit::commit;

fn nanos() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
}
// cargo runs tests in parallel threads of ONE process, so process::id() is shared
// and nanos() can repeat at the clock's real resolution — two concurrent same-tag
// tmp() calls would collide on the same dir (and `git clone` then fails on a
// non-empty target). An atomic per-call counter makes every temp dir unique → the
// suite is hermetic under parallelism.
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);
fn tmp(tag: &str) -> PathBuf {
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("wc-{}-{}-{}-{}", tag, std::process::id(), nanos(), seq));
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

// #3186 — the rebase-or-refuse invariant. werk-commit closes the pull->commit
// staleness window: it rebases the card's commit onto CURRENT origin/main before
// it lands, and refuses cleanly (werk preserved) when that rebase conflicts.

#[test]
fn commit_rebases_onto_current_origin_main_when_a_peer_moved_main() {
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

    let werk_base = tmp("werk");
    let werk = werk_base.join("kade-9101");
    git(&home, &["worktree", "add", "-b", "kade/9101", werk.to_str().unwrap(), "origin/main"]);

    // my card work — a NEW file the peer will not touch.
    fs::write(werk.join("card.txt"), "card-work").unwrap();

    // a PEER advances origin/main (a DIFFERENT file) AFTER the werk was created —
    // this is the pull->commit staleness window opening.
    fs::write(origin.join("peer.txt"), "peer-work").unwrap();
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-q", "-m", "peer moved main"]);

    let sha = commit(9101, "kade", "rebase test", &home, &werk_base).expect("commit ok");

    // refresh the test's own view of origin/main, then assert against the TRUTH.
    git(&werk, &["fetch", "-q", "origin", "main"]);
    let ahead = Command::new("git")
        .args(["-C", werk.to_str().unwrap(), "rev-list", "--count", "origin/main..HEAD"])
        .output().unwrap();
    assert_eq!(String::from_utf8_lossy(&ahead.stdout).trim(), "1", "exactly one card commit on top of current main");
    let behind = Command::new("git")
        .args(["-C", werk.to_str().unwrap(), "rev-list", "--count", "HEAD..origin/main"])
        .output().unwrap();
    assert_eq!(String::from_utf8_lossy(&behind.stdout).trim(), "0", "werk base IS current origin/main — not behind");
    // the peer's change is present in the werk -> proves we rebased onto current main.
    assert!(werk.join("peer.txt").exists(), "peer's file pulled in by the rebase");
    assert!(werk.join("card.txt").exists(), "card work preserved through the rebase");
    assert!(sha.len() >= 7);
}

#[test]
fn commit_refuses_on_rebase_conflict_and_preserves_the_werk() {
    let origin = tmp("origin");
    git(&origin, &["init", "-q", "-b", "main", "."]);
    fs::write(origin.join("conflict.txt"), "base\n").unwrap();
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-q", "-m", "init"]);
    git(&origin, &["config", "receive.denyCurrentBranch", "ignore"]);

    let home = tmp("home");
    assert!(Command::new("git")
        .args(["clone", "-q", origin.to_str().unwrap(), home.to_str().unwrap()])
        .status().unwrap().success());

    let werk_base = tmp("werk");
    let werk = werk_base.join("kade-9102");
    git(&home, &["worktree", "add", "-b", "kade/9102", werk.to_str().unwrap(), "origin/main"]);

    // my work edits the line one way...
    fs::write(werk.join("conflict.txt"), "kade-line\n").unwrap();
    // ...the peer edits the SAME line the other way on origin/main.
    fs::write(origin.join("conflict.txt"), "peer-line\n").unwrap();
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-q", "-m", "peer conflicting edit"]);

    let res = commit(9102, "kade", "conflict test", &home, &werk_base);

    // refused with the typed reason — never a swallowed half-merge.
    let err = res.expect_err("must refuse on rebase conflict");
    assert!(err.contains("rebase-conflict"), "typed rebase-conflict reason, got: {}", err);

    // werk PRESERVED: clean tree (no half-applied rebase), still on the card branch,
    // and the card's work survives as a local commit.
    let status = Command::new("git")
        .args(["-C", werk.to_str().unwrap(), "status", "--porcelain"])
        .output().unwrap();
    assert!(String::from_utf8_lossy(&status.stdout).trim().is_empty(), "werk tree clean after abort (all-or-nothing)");
    let br = Command::new("git")
        .args(["-C", werk.to_str().unwrap(), "rev-parse", "--abbrev-ref", "HEAD"])
        .output().unwrap();
    assert_eq!(String::from_utf8_lossy(&br.stdout).trim(), "kade/9102", "still on the card branch");
    assert_eq!(fs::read_to_string(werk.join("conflict.txt")).unwrap(), "kade-line\n", "card work preserved verbatim");
    let log = Command::new("git")
        .args(["-C", werk.to_str().unwrap(), "log", "--oneline", "-1"])
        .output().unwrap();
    assert!(String::from_utf8_lossy(&log.stdout).contains("#9102"), "card commit preserved");
}
