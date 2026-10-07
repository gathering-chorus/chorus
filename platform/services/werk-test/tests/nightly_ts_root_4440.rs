//! #4440 reopen — prod audit 2026-10-07: the 03:00 nightly planned 0 npm in
//! every stage. launchd starts it at cwd `/` and nothing named the tree, so
//! ts_packages() found no TS package and no TS test ran (mcp-server included).
use std::path::{Path, PathBuf};
use werk_test::{discover_ts_packages, npm_lane_unmeasured, resolve_repo_root};

fn repo() -> PathBuf {
    // run-time, never env!() — the #4030 guard
    let m = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    m.ancestors().find(|d| d.join(".git").exists()).unwrap().to_path_buf()
}

#[test]
fn launchd_cwd_with_chorus_root_finds_the_ts_packages() {
    let root = resolve_repo_root(None, Some(repo().to_string_lossy().into()), Path::new("/")).expect("a root");
    let pkgs = discover_ts_packages(&root);
    assert!(pkgs.iter().any(|p| p == "platform/mcp-server"), "{:?}", pkgs);
    assert!(pkgs.iter().any(|p| p == "platform/api"), "{:?}", pkgs);
}

#[test]
fn negative_proof_cwd_slash_and_no_root_finds_nothing() {
    // the prod state before the fix: no named root, no env, cwd `/`
    assert_eq!(resolve_repo_root(None, None, Path::new("/")), None);
    // an env root that is not a repo does not count either
    assert_eq!(resolve_repo_root(None, Some("/tmp".into()), Path::new("/")), None);
}

#[test]
fn a_named_root_wins() {
    let named = PathBuf::from("/x/named");
    assert_eq!(resolve_repo_root(Some(named.clone()), Some(repo().to_string_lossy().into()), Path::new("/")), Some(named));
}

#[test]
fn an_npm_lane_with_nothing_planned_on_a_ts_tree_is_unmeasured() {
    let found = vec!["platform/api".to_string(), "platform/mcp-server".to_string()];
    let line = npm_lane_unmeasured(&found, &[]).expect("unmeasured");
    assert!(line.contains("UNMEASURED") && line.contains("0 of 2"), "{line}");
}

#[test]
fn negative_proof_a_planned_lane_or_a_tree_without_ts_is_not_flagged() {
    let found = vec!["platform/api".to_string()];
    assert_eq!(npm_lane_unmeasured(&found, &found), None);
    assert_eq!(npm_lane_unmeasured(&[], &[]), None);
}
