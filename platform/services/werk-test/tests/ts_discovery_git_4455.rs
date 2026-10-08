// @domain: tests
//! #4455 — TS package discovery asks git, not the disk. The nightly's tree
//! holds untracked runtime data (platform/backups/graph-retirements, 581k
//! files); walking it made every caller take 35-47 s and the nightly ran over
//! its budget. Git gives the same packages without listing that data.
use std::path::{Path, PathBuf};
use std::process::Command;
use werk_test::{discover_ts_packages, git_ts_packages, walk_ts_packages};

fn repo_root() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join("../../..").canonicalize().unwrap()
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git").arg("-C").arg(dir).args(args).status().unwrap().success();
    assert!(ok, "git {:?} failed", args);
}

fn package(dir: &Path, rel: &str) {
    std::fs::create_dir_all(dir.join(rel)).unwrap();
    std::fs::write(dir.join(rel).join("package.json"), r#"{"scripts":{"test":"jest"}}"#).unwrap();
}

#[test]
fn on_the_real_tree_git_finds_what_the_walk_finds() {
    let root = repo_root();
    let Some(from_git) = git_ts_packages(&root) else { panic!("{} is not a git tree", root.display()) };
    assert!(!from_git.is_empty(), "no TS packages found; fail loud, not vacuous");
    let tracked_walk: Vec<String> = walk_ts_packages(&root).into_iter()
        .filter(|p| Command::new("git").arg("-C").arg(&root)
            .args(["ls-files", "--error-unmatch", &format!("{}/package.json", p)])
            .output().map(|o| o.status.success()).unwrap_or(false))
        .collect();
    assert_eq!(from_git, tracked_walk);
}

#[test]
fn a_tree_with_no_git_still_walks() {
    let t = tempfile_dir("nogit");
    package(&t, "pkg/a");
    assert_eq!(git_ts_packages(&t), None);
    assert_eq!(discover_ts_packages(&t), vec!["pkg/a".to_string()]);
}

#[test]
fn negative_proof_untracked_data_is_not_listed_and_a_tracked_package_is() {
    let t = tempfile_dir("git");
    git(&t, &["init", "-q"]);
    package(&t, "pkg/tracked");
    package(&t, "backups/untracked");
    package(&t, "pkg/node_modules/dep");
    git(&t, &["add", "pkg/tracked/package.json"]);
    git(&t, &["add", "-f", "pkg/node_modules/dep/package.json"]);
    assert_eq!(discover_ts_packages(&t), vec!["pkg/tracked".to_string()]);
    // the walk would have listed the untracked one — the two states differ
    assert!(walk_ts_packages(&t).contains(&"backups/untracked".to_string()));
}

fn tempfile_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("werk-test-4455-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}
