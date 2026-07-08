//! #3431 — hermetic real-git tests for the ported `chorus-werk remove` semantics.
//! Every test brings its own world: a bare "origin", a canonical clone, and real
//! worktrees under a temp werk_base. No shims for the teardown path — the point
//! of the port is that the git behavior itself is now the verb's own domain.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use werk_teardown::{subjects_reference_card, teardown_werk, Teardown, TeardownError};

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);
fn tmp(tag: &str) -> PathBuf {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("wteardown-{}-{}-{}-{}", tag, std::process::id(), n, seq));
    fs::create_dir_all(&p).unwrap();
    p
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git").args(args).current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t")
        .output().unwrap();
    assert!(out.status.success(), "git {:?} failed in {}: {}", args, dir.display(),
        String::from_utf8_lossy(&out.stderr));
}

fn git_out(dir: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new("git").args(args).current_dir(dir).output().unwrap();
    (out.status.success(), String::from_utf8_lossy(&out.stdout).to_string())
}

/// Bare origin + canonical clone with one commit on main. Returns (origin, home).
fn scenario() -> (PathBuf, PathBuf) {
    let origin = tmp("origin");
    git(&origin, &["init", "--bare", "--initial-branch=main", "."]);
    let home = tmp("home");
    git(&home, &["clone", &origin.to_string_lossy(), "."]);
    fs::write(home.join("seed.txt"), "seed").unwrap();
    git(&home, &["add", "-A"]);
    git(&home, &["commit", "-m", "seed"]);
    git(&home, &["push", "origin", "main"]);
    (origin, home)
}

/// Create the card's werk: worktree at werk_base/<role>-<card> on branch <role>/<card>.
fn add_werk(home: &Path, werk_base: &Path, role: &str, card: u64) -> PathBuf {
    let werk = werk_base.join(format!("{}-{}", role, card));
    git(home, &["worktree", "add", "-b", &format!("{}/{}", role, card),
        &werk.to_string_lossy(), "origin/main"]);
    werk
}

fn collect_emit(events: &mut Vec<String>) -> impl FnMut(&str, &[(&str, &str)]) + '_ {
    move |event, extras| {
        let kv: Vec<String> = extras.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
        events.push(format!("{} {}", event, kv.join(" ")));
    }
}

#[test]
fn clean_removal_of_merged_branch_tier1() {
    let (_origin, home) = scenario();
    let base = tmp("base");
    let werk = add_werk(&home, &base, "kade", 42);
    // commit on the branch, then an acp-style commit referencing #42 on main (squash analogue)
    fs::write(werk.join("w.txt"), "w").unwrap();
    git(&werk, &["add", "-A"]);
    git(&werk, &["commit", "-m", "kade: #42 — work"]);
    fs::write(home.join("squash.txt"), "sq").unwrap();
    git(&home, &["add", "-A"]);
    git(&home, &["commit", "-m", "#42 (kade) (#999)"]);
    git(&home, &["push", "origin", "main"]);

    let mut events = Vec::new();
    let res = teardown_werk(&home, &base, "kade", 42, &mut collect_emit(&mut events));
    assert_eq!(res, Ok(Teardown::Removed));
    assert!(!werk.exists(), "worktree dir should be gone");
    let (branch_exists, _) = git_out(&home, &["rev-parse", "--verify", "refs/heads/kade/42"]);
    assert!(!branch_exists, "local branch should be deleted");
    assert!(events.iter().any(|e| e.starts_with("card.branch.closed") && e.contains("branch=kade/42")),
        "spine event emitted: {:?}", events);
}

#[test]
fn dirty_werk_refused_nothing_lost() {
    let (_origin, home) = scenario();
    let base = tmp("base");
    let werk = add_werk(&home, &base, "kade", 7);
    fs::write(werk.join("uncommitted.txt"), "precious").unwrap();

    let mut events = Vec::new();
    let res = teardown_werk(&home, &base, "kade", 7, &mut collect_emit(&mut events));
    assert!(matches!(res, Err(TeardownError::Dirty(_))), "got {:?}", res);
    assert!(werk.join("uncommitted.txt").exists(), "dirty werk must be untouched");
    let (branch_exists, _) = git_out(&home, &["rev-parse", "--verify", "refs/heads/kade/7"]);
    assert!(branch_exists, "branch must survive a refused remove");
}

#[test]
fn unmerged_branch_refused_tier2() {
    let (_origin, home) = scenario();
    let base = tmp("base");
    let werk = add_werk(&home, &base, "kade", 9);
    fs::write(werk.join("real-work.txt"), "not on main").unwrap();
    git(&werk, &["add", "-A"]);
    git(&werk, &["commit", "-m", "kade: real unmerged work"]); // no #9 on main

    let mut events = Vec::new();
    let res = teardown_werk(&home, &base, "kade", 9, &mut collect_emit(&mut events));
    assert!(matches!(res, Err(TeardownError::Unmerged(_))), "got {:?}", res);
    let (branch_exists, _) = git_out(&home, &["rev-parse", "--verify", "refs/heads/kade/9"]);
    assert!(branch_exists, "unmerged branch must NOT be deleted");
}

#[test]
fn empty_branch_no_commits_removes_clean() {
    // branch identical to origin/main (fresh pull, nothing committed): cherry
    // reports nothing, Tier 2 passes, teardown proceeds.
    let (_origin, home) = scenario();
    let base = tmp("base");
    let werk = add_werk(&home, &base, "kade", 11);

    let mut events = Vec::new();
    let res = teardown_werk(&home, &base, "kade", 11, &mut collect_emit(&mut events));
    assert_eq!(res, Ok(Teardown::Removed));
    assert!(!werk.exists());
}

#[test]
fn already_removed_is_idempotent() {
    let (_origin, home) = scenario();
    let base = tmp("base");
    let mut events = Vec::new();
    let res = teardown_werk(&home, &base, "kade", 99, &mut collect_emit(&mut events));
    assert_eq!(res, Ok(Teardown::AlreadyRemoved));
    assert!(events.is_empty(), "no spine event on a full no-op: {:?}", events);
}

#[test]
fn remote_only_orphan_propagates_delete() {
    // #3498: local werk + branch gone, remote ref still exists → delete propagates.
    let (_origin, home) = scenario();
    let base = tmp("base");
    let werk = add_werk(&home, &base, "kade", 55);
    git(&home, &["push", "origin", "kade/55"]);
    // tear down locally by hand (the pre-#3498 failure shape)
    git(&home, &["worktree", "remove", "--force", &werk.to_string_lossy()]);
    git(&home, &["branch", "-D", "kade/55"]);

    let mut events = Vec::new();
    let res = teardown_werk(&home, &base, "kade", 55, &mut collect_emit(&mut events));
    assert_eq!(res, Ok(Teardown::OrphanPropagated));
    let (remote_has, _) = git_out(&home, &["ls-remote", "--exit-code", "--heads", "origin", "kade/55"]);
    assert!(!remote_has, "orphaned remote ref must be deleted");
    assert!(events.iter().any(|e| e.contains("via=orphan-propagate")), "events: {:?}", events);
}

#[test]
fn remote_ref_deleted_on_normal_removal() {
    let (_origin, home) = scenario();
    let base = tmp("base");
    let werk = add_werk(&home, &base, "kade", 60);
    git(&home, &["push", "origin", "kade/60"]);
    let _ = werk;

    let mut events = Vec::new();
    let res = teardown_werk(&home, &base, "kade", 60, &mut collect_emit(&mut events));
    assert_eq!(res, Ok(Teardown::Removed));
    let (remote_has, _) = git_out(&home, &["ls-remote", "--exit-code", "--heads", "origin", "kade/60"]);
    assert!(!remote_has, "remote ref must be cleaned up on removal");
}

#[test]
fn werk_mcp_marker_cleaned_before_dirty_check() {
    // #3016: an untracked .werk-mcp dir must not trip the dirty refusal.
    // (.werk-mcp with no label file — the launchctl path needs no daemon here.)
    let (_origin, home) = scenario();
    let base = tmp("base");
    let werk = add_werk(&home, &base, "kade", 71);
    fs::create_dir_all(werk.join(".werk-mcp")).unwrap();

    let mut events = Vec::new();
    let res = teardown_werk(&home, &base, "kade", 71, &mut collect_emit(&mut events));
    assert_eq!(res, Ok(Teardown::Removed), "untracked .werk-mcp must not refuse remove");
}

#[test]
fn card_boundary_300_does_not_match_3000() {
    assert!(subjects_reference_card("#300 (kade) (#12)", 300));
    assert!(!subjects_reference_card("#3000 (kade) (#12)", 300));
    assert!(subjects_reference_card("wren: #3000 — thing", 3000));
    assert!(subjects_reference_card("end of line #300", 300));
    assert!(!subjects_reference_card("no refs here", 300));
    // #3001 then #300 later on the same line still matches 300
    assert!(subjects_reference_card("#3001 relates to #300", 300));
}
