//! Real end-to-end: actual `git` on temp repos + PATH-shimmed `cards` and `gh`.
//! Proves pull() against the v6 diagram (gh = process holder): worktree off
//! origin/main, branch pushed so gh has a ref, gh state registered, board WIP —
//! and all-or-nothing rollback when a later step fails. One env-mutating test fn
//! so PATH / CARDS_* / GH_* can't race other tests.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use werk_pull::{lock, pull};

fn nanos() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
}

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("wp-{}-{}-{}", tag, std::process::id(), nanos()));
    fs::create_dir_all(&p).unwrap();
    p
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .status()
        .unwrap()
        .success();
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
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).contains(branch)
}

#[test]
fn e2e_pull_to_gh_diagram() {
    // --- shims on PATH: cards + gh ---
    let bin = tmp("bin");
    write_exec(
        &bin.join("cards"),
        "#!/bin/sh\ncase \"$1\" in\n view) echo \"{ \\\"status\\\": \\\"${CARDS_STATUS:-Next}\\\" }\" ;;\n move) exit \"${CARDS_MOVE_EXIT:-0}\" ;;\n *) exit 0 ;;\nesac\n",
    );
    write_exec(&bin.join("gh"), "#!/bin/sh\necho \"$@\" >> \"$GH_LOG\"\nexit \"${GH_EXIT:-0}\"\n");
    std::env::set_var(
        "PATH",
        format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()),
    );
    let gh_log = tmp("ghlog").join("calls");
    std::env::set_var("GH_LOG", gh_log.to_str().unwrap());

    // --- real git: origin (with a main commit) + home clone + werk base ---
    let origin = tmp("origin");
    git(&origin, &["init", "-q", "-b", "main", "."]);
    fs::write(origin.join("README"), "x").unwrap();
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-q", "-m", "init"]);
    git(&origin, &["config", "receive.denyCurrentBranch", "ignore"]);
    let home = tmp("home");
    assert!(Command::new("git")
        .args(["clone", "-q", origin.to_str().unwrap(), home.to_str().unwrap()])
        .status()
        .unwrap()
        .success());
    let werk_base = tmp("werk");

    // --- happy path ---
    std::env::set_var("CARDS_STATUS", "Next");
    std::env::set_var("CARDS_MOVE_EXIT", "0");
    std::env::set_var("GH_EXIT", "0");
    let branch = pull(7001, "kade", &home, &werk_base).expect("happy pull");
    assert_eq!(branch, "kade/7001");
    assert!(werk_base.join("kade-7001").is_dir(), "worktree created");
    assert!(!remote_has(&home, "kade/7001"), "pull does NOT push at pull — branch stays local until acp");
    assert!(
        fs::read_to_string(&gh_log).map(|s| s.contains("statuses")).unwrap_or(false),
        "gh state registered"
    );

    // --- idempotent re-pull ---
    assert_eq!(pull(7001, "kade", &home, &werk_base).expect("idempotent"), "kade/7001");

    // --- rollback on cards-move failure: worktree + remote ref undone ---
    std::env::set_var("CARDS_MOVE_EXIT", "1");
    assert!(pull(7002, "kade", &home, &werk_base).is_err(), "cards-move fail => err");
    assert!(!werk_base.join("kade-7002").exists(), "worktree rolled back");

    // --- rollback on gh failure (branch already pushed, must be undone) ---
    std::env::set_var("CARDS_MOVE_EXIT", "0");
    std::env::set_var("GH_EXIT", "1");
    assert!(pull(7003, "kade", &home, &werk_base).is_err(), "gh fail => err");
    assert!(!werk_base.join("kade-7003").exists(), "worktree rolled back on gh fail");

    // --- lock exclusivity (concurrent-pull safety) ---
    let _held = lock(&home, Duration::from_millis(100)).expect("first lock acquires");
    assert!(
        lock(&home, Duration::from_millis(200)).is_err(),
        "second lock blocked while the first is held"
    );
}
