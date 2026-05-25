//! Real end-to-end for the push-only verb (#3056 split): actual `git` on temp
//! repos + a PATH-shimmed `gh`. Proves werk-push PUSHES a locally-committed branch
//! to origin and registers the gh chorus/push status — the visible state that
//! commit-only deliberately leaves absent. One env-mutating test fn (PATH/GH_*).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use werk_push::push;

fn nanos() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
}
fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("wpush-{}-{}-{}", tag, std::process::id(), nanos()));
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
fn write_exec(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    let mut perm = fs::metadata(path).unwrap().permissions();
    perm.set_mode(0o755);
    fs::set_permissions(path, perm).unwrap();
}
fn remote_has(home: &Path, branch: &str) -> bool {
    let out = Command::new("git")
        .args(["-C", home.to_str().unwrap(), "ls-remote", "--heads", "origin", branch])
        .output().unwrap();
    String::from_utf8_lossy(&out.stdout).contains(branch)
}

#[test]
fn push_pushes_committed_branch_and_registers_gh() {
    // gh shim on PATH — logs calls, succeeds.
    let bin = tmp("bin");
    write_exec(&bin.join("gh"), "#!/bin/sh\necho \"$@\" >> \"$GH_LOG\"\nexit \"${GH_EXIT:-0}\"\n");
    std::env::set_var("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()));
    let gh_log = tmp("ghlog").join("calls");
    std::env::set_var("GH_LOG", gh_log.to_str().unwrap());
    std::env::set_var("GH_EXIT", "0");

    // origin + home clone + a card worktree with a LOCAL commit ready to push.
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
    let werk = werk_base.join("kade-9002");
    git(&home, &["worktree", "add", "-b", "kade/9002", werk.to_str().unwrap(), "origin/main"]);
    fs::write(werk.join("w.txt"), "x").unwrap();
    git(&werk, &["add", "."]);
    git(&werk, &["commit", "-q", "-m", "local commit"]);

    // before push: origin does NOT have the branch.
    assert!(!remote_has(&home, "kade/9002"), "precondition: not yet pushed");

    let sha = push(9002, "kade", &home, &werk_base).expect("push ok");

    // pushed: origin now has the branch at our sha; gh chorus/push registered.
    assert!(remote_has(&home, "kade/9002"), "push lands the branch on origin");
    assert!(sha.len() >= 7, "returns the pushed sha");
    let gh = fs::read_to_string(&gh_log).unwrap_or_default();
    assert!(gh.contains("chorus/push/9002"), "registers chorus/push gh status");

    // idempotent: re-push the same already-pushed branch -> no-op success, same sha.
    assert_eq!(push(9002, "kade", &home, &werk_base).expect("idempotent re-push"), sha);

    // nothing-to-push: a fresh card whose werk has no commits ahead -> refuse.
    let werk2 = werk_base.join("kade-9003");
    git(&home, &["worktree", "add", "-b", "kade/9003", werk2.to_str().unwrap(), "origin/main"]);
    assert!(push(9003, "kade", &home, &werk_base).is_err(), "nothing to push => refuse");

    // no-werk: never pulled -> refuse, no push.
    assert!(push(9999, "kade", &home, &werk_base).is_err(), "no werk => refuse");
}
