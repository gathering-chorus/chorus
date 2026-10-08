//! #4458 — a werk is group-writable, so a role working as its own OS account
//! (Abby, group chorus) can edit the werk the message service created for her.
//! On 2026-10-08 her abby-normal-4456 werk came out 755/644 and she could not
//! write in it. Each test makes a real worktree in a temp repo; the negative
//! proof makes one the old way, under a 022 umask, and shows it is not.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use werk_pull::worktree_add_args;

fn sh(dir: &Path, script: &str) {
    let ok = Command::new("sh").arg("-c").arg(script).current_dir(dir).status().unwrap().success();
    assert!(ok, "setup failed: {}", script);
}

/// A repo with one committed file and an origin/main to branch from.
/// `name` keeps the two tests apart: they run in parallel in one process, and
/// the clock alone gave them the same folder (macOS time is microsecond-grained).
fn repo(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("werk-pull-4458-{}-{}-{}", name, std::process::id(), rand_suffix()));
    std::fs::create_dir_all(&root).unwrap();
    sh(&root, "git init -q -b main origin.git --bare && git init -q -b main repo && cd repo \
        && git -c user.email=t@t -c user.name=t commit -q --allow-empty -m init \
        && echo hi > file.txt && git add file.txt && git -c user.email=t@t -c user.name=t commit -q -m file \
        && git remote add origin ../origin.git && git push -q origin main && git fetch -q origin main");
    root
}

fn rand_suffix() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}

fn group_writes(p: &Path) -> bool {
    std::fs::metadata(p).unwrap().permissions().mode() & 0o020 != 0
}

#[test]
fn the_werk_and_its_files_are_group_writable() {
    let root = repo("new");
    let (r, w) = (root.join("repo"), root.join("werk-a"));
    let (rs, ws) = (r.to_str().unwrap(), w.to_str().unwrap());
    // run as werk-pull does, from a parent that would otherwise strip group write
    let args = worktree_add_args(rs, "a/1", ws);
    let script = format!("umask 022 && exec sh {}", args.iter().map(|a| format!("'{}'", a.replace('\'', "'\\''"))).collect::<Vec<_>>().join(" "));
    sh(&root, &script);
    assert!(group_writes(&w), "werk folder not group-writable");
    assert!(group_writes(&w.join("file.txt")), "werk file not group-writable");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn negative_proof_the_old_add_under_umask_022_is_not_group_writable() {
    let root = repo("old");
    let (r, w) = (root.join("repo"), root.join("werk-b"));
    sh(&root, &format!("umask 022 && git -C '{}' worktree add -q -b b/1 '{}' origin/main", r.display(), w.display()));
    assert!(!group_writes(&w.join("file.txt")), "the check cannot tell a 644 file from a 664 one");
    let _ = std::fs::remove_dir_all(&root);
}
