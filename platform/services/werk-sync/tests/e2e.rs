//! Real end-to-end (#3300): actual `git` on temp repos. Proves Bash parity for
//! both verbs — recover (stash M/A/D to the recovery dir + manifest, untracked
//! untouched, tree clean, ff'd to origin/main, spine events) and repair
//! (detached HEAD re-attached to main at origin/main via plumbing, tree aligned).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use werk_sync::{recover, repair};

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);
fn tmp(tag: &str) -> PathBuf {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("wsync-{}-{}-{}-{}", tag, std::process::id(), n, seq));
    fs::create_dir_all(&p).unwrap();
    p
}
fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git").args(args).current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t")
        .status().unwrap().success();
    assert!(ok, "git {:?} failed in {}", args, dir.display());
}
fn git_out(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git").args(args).current_dir(dir).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}
fn write_exec(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    let mut perm = fs::metadata(path).unwrap().permissions();
    perm.set_mode(0o755);
    fs::set_permissions(path, perm).unwrap();
}

/// origin (main + a peer commit ahead) + home clone (one behind). Returns
/// (origin, home, spine_capture_path).
fn scenario() -> (PathBuf, PathBuf, PathBuf) {
    let origin = tmp("origin");
    git(&origin, &["init", "-q", "-b", "main", "."]);
    fs::write(origin.join("README"), "x\n").unwrap();
    fs::write(origin.join("tracked.md"), "base\n").unwrap();
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-q", "-m", "init"]);
    let home = tmp("home");
    assert!(Command::new("git")
        .args(["clone", "-q", origin.to_str().unwrap(), home.to_str().unwrap()])
        .status().unwrap().success());
    // peer moves origin/main ahead so a sync has something to ff to.
    fs::write(origin.join("peer.txt"), "peer\n").unwrap();
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-q", "-m", "peer ahead"]);
    // spine capture at the absolute chorus-log path.
    let scripts = home.join("platform/scripts");
    fs::create_dir_all(&scripts).unwrap();
    let cap = home.join("spine-capture.txt");
    write_exec(&scripts.join("chorus-log"), &format!("#!/bin/sh\necho \"$@\" >> \"{}\"\n", cap.display()));
    (origin, home, cap)
}

#[test]
fn recover_stashes_dirty_tracked_files_and_ffs_to_origin_main() {
    let (_origin, home, cap) = scenario();
    // dirty states: M (tracked edit), A (staged new), untracked ?? (must survive).
    fs::write(home.join("tracked.md"), "uncommitted work\n").unwrap();
    fs::write(home.join("added.md"), "staged new file\n").unwrap();
    git(&home, &["add", "added.md"]);
    fs::write(home.join("untracked.txt"), "left alone\n").unwrap();

    let base = tmp("recovery");
    let msg = recover(&home, &base, "20990101T000000").expect("recover ok");
    assert!(msg.contains("stashed 2 file(s)"), "M + A stashed (untracked excluded): {msg}");

    // recovery dir holds both contents + the manifest maps hash → path.
    let dir = base.join("20990101T000000");
    let manifest = fs::read_to_string(dir.join("manifest.tsv")).unwrap();
    assert!(manifest.contains("\ttracked.md") && manifest.contains("\tadded.md"), "manifest rows: {manifest}");
    let recovered_contents: Vec<String> = manifest.lines()
        .map(|l| fs::read_to_string(dir.join(l.split('\t').next().unwrap())).unwrap())
        .collect();
    assert!(recovered_contents.contains(&"uncommitted work\n".to_string()), "M content preserved");
    assert!(recovered_contents.contains(&"staged new file\n".to_string()), "A content preserved");

    // tree: synced to origin/main, dirty work GONE from tree, untracked UNTOUCHED.
    assert_eq!(fs::read_to_string(home.join("tracked.md")).unwrap(), "base\n", "M restored from HEAD");
    assert!(!home.join("added.md").exists(), "A-state file removed (not in HEAD)");
    assert_eq!(fs::read_to_string(home.join("untracked.txt")).unwrap(), "left alone\n", "untracked survives");
    assert!(home.join("peer.txt").exists(), "ff'd to origin/main (peer commit present)");
    assert_eq!(git_out(&home, &["rev-list", "--count", "HEAD..origin/main"]), "0", "not behind");
    // tree carries only untracked entries afterwards (the fixture's shim/lock files
    // are themselves untracked) — no M/A/D dirt remains, and untracked.txt survives.
    let status = git_out(&home, &["status", "--porcelain"]);
    assert!(status.lines().all(|l| l.starts_with("??")), "no tracked dirt remains: {status}");
    assert!(status.contains("?? untracked.txt"), "untracked file survives: {status}");

    // spine: one stashed event per file + a completed event (Bash event names).
    let emitted = fs::read_to_string(&cap).unwrap_or_default();
    assert_eq!(emitted.matches("canonical.recovery.stashed").count(), 2, "{emitted}");
    assert!(emitted.contains("canonical.recovery.completed") && emitted.contains("recovered=2"), "{emitted}");
}

#[test]
fn recover_on_clean_tree_is_a_noop_that_still_syncs() {
    let (_origin, home, cap) = scenario();
    let base = tmp("recovery");
    let msg = recover(&home, &base, "20990101T000001").expect("recover ok");
    assert!(msg.contains("no dirty files"), "no-op message: {msg}");
    assert!(home.join("peer.txt").exists(), "still ff'd to origin/main");
    assert!(!base.join("20990101T000001").exists(), "no recovery dir created for nothing");
    assert!(fs::read_to_string(&cap).unwrap_or_default().contains("recovered=0"));
}

#[test]
fn repair_reattaches_a_detached_head_to_main_at_origin_main() {
    let (_origin, home, cap) = scenario();
    // detach canonical exactly like the incident class: checkout a raw sha.
    let sha = git_out(&home, &["rev-parse", "HEAD"]);
    git(&home, &["checkout", "-q", &sha]);
    assert_eq!(git_out(&home, &["rev-parse", "--abbrev-ref", "HEAD"]), "HEAD", "precondition: detached");

    let msg = repair(&home).expect("repair ok");
    assert!(msg.contains("HEAD attached to main"), "{msg}");
    assert_eq!(git_out(&home, &["rev-parse", "--abbrev-ref", "HEAD"]), "main", "re-attached");
    assert_eq!(
        git_out(&home, &["rev-parse", "HEAD"]),
        git_out(&home, &["rev-parse", "origin/main"]),
        "main fast-forwarded to origin/main"
    );
    assert!(home.join("peer.txt").exists(), "working tree aligned to the new HEAD");
    assert!(fs::read_to_string(&cap).unwrap_or_default().contains("canonical.repaired"));
}

#[test]
fn recover_refuses_when_ff_is_impossible_and_names_repair() {
    let (_origin, home, _cap) = scenario();
    // local commit on main → diverged from origin/main → ff-only must fail.
    fs::write(home.join("local.txt"), "local\n").unwrap();
    git(&home, &["add", "."]);
    git(&home, &["commit", "-q", "-m", "local divergence"]);
    let base = tmp("recovery");
    let err = recover(&home, &base, "20990101T000002").expect_err("diverged → refuse");
    assert!(err.contains("werk-sync repair"), "refusal points at the repair verb: {err}");
}

#[test]
fn not_a_git_repo_fails_loud() {
    let bad = tmp("notrepo");
    assert!(repair(&bad).unwrap_err().contains("not a git repo"));
    assert!(recover(&bad, &tmp("r"), "t").unwrap_err().contains("not a git repo"));
}
