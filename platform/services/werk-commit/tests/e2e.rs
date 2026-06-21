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
// #3528 — the werk-commit binary spawns its OWN `git commit` (not via this helper),
// so it inherits the TEST PROCESS env, not the per-Command .env() below. On a CI runner
// with no global git identity, that commit died "Author identity unknown". Set a
// deterministic identity process-wide ONCE (mirrors what `chorus-werk add` does per-werk
// in production) so every spawned git — helper or binary — has an author.
fn ensure_git_identity() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        std::env::set_var("GIT_AUTHOR_NAME", "t");
        std::env::set_var("GIT_AUTHOR_EMAIL", "t@t");
        std::env::set_var("GIT_COMMITTER_NAME", "t");
        std::env::set_var("GIT_COMMITTER_EMAIL", "t@t");
    });
}
fn git(dir: &Path, args: &[&str]) {
    ensure_git_identity();
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

    // #3304 — the conflict HOLDS (replaces #3186's abort-and-refuse): typed message
    // naming the in-verb follow-ups, markers left in the werk, rebase in progress.
    let err = res.expect_err("must hold on rebase conflict");
    assert!(err.contains("rebase-conflict"), "typed rebase-conflict reason, got: {}", err);
    assert!(err.contains("--continue") && err.contains("--abort"),
        "held message names both verb follow-ups, got: {}", err);

    let f = fs::read_to_string(werk.join("conflict.txt")).unwrap();
    assert!(f.contains("<<<<<<<") && f.contains(">>>>>>>"),
        "conflict markers left in the werk for the human: {f}");
    // the card's commit is preserved on the branch ref while the rebase is held.
    let log = Command::new("git")
        .args(["-C", werk.to_str().unwrap(), "log", "--oneline", "-1", "kade/9102"])
        .output().unwrap();
    assert!(String::from_utf8_lossy(&log.stdout).contains("#9102"), "card commit preserved on the branch ref");
}

// #3162 — werk-commit must surface its failures on the ONE spine, carrying the
// INHERITED trace (the #3045 verb contract). RED now: werk-commit is jsonl-only
// and fresh-mints its trace, so a failed commit emits NOTHING to chorus-log.
#[test]
fn commit_emits_failure_to_the_spine_with_the_inherited_trace() {
    let origin = tmp("origin");
    git(&origin, &["init", "-q", "-b", "main", "."]);
    fs::write(origin.join("README"), "x").unwrap();
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-q", "-m", "init"]);

    let home = tmp("home");
    assert!(Command::new("git")
        .args(["clone", "-q", origin.to_str().unwrap(), home.to_str().unwrap()])
        .status().unwrap().success());

    let werk_base = tmp("werk");
    let werk = werk_base.join("kade-9301");
    git(&home, &["worktree", "add", "-q", "-b", "kade/9301", werk.to_str().unwrap(), "origin/main"]);

    // capture-shim chorus-log at the path emit_spine invokes (home/platform/scripts/chorus-log).
    let log = home.join("platform/scripts/chorus-log");
    fs::create_dir_all(log.parent().unwrap()).unwrap();
    let cap = home.join("spine-capture.txt");
    fs::write(&log, format!("#!/bin/sh\necho \"$@\" >> \"{}\"\n", cap.display())).unwrap();
    assert!(Command::new("chmod").args(["+x", log.to_str().unwrap()]).status().unwrap().success());

    // Seed the INHERITED trace via the /tmp file carrier (resolve_trace reads it) —
    // no env mutation, so the test stays hermetic under parallelism.
    fs::write("/tmp/9301-trace", "inherited-trace-9301").unwrap();

    // FAILURE path: clean werk, nothing to commit → commit() returns Err. Today that
    // return is fully silent (the exact #3162 bug).
    let res = commit(9301, "kade", "noop", &home, &werk_base);
    assert!(res.is_err(), "nothing-to-commit must be an Err");

    let emitted = fs::read_to_string(&cap).unwrap_or_default();
    assert!(emitted.contains("card=9301"), "the failure reached the spine, keyed by card: {:?}", emitted);
    assert!(
        emitted.contains("trace=inherited-trace-9301"),
        "carries the INHERITED trace (#3045 contract), not a fresh mint: {:?}", emitted
    );
    assert!(
        emitted.to_lowercase().contains("commit"),
        "a commit.* lifecycle/failure event reached the spine: {:?}", emitted
    );
}

// #3295 — commit --atomic = commit-WITHOUT-rebase. The escape from the #3223
// rebase-conflict deadlock: a werk that conflicts with current origin/main can
// still COMMIT LOCALLY (no rebase), so work is never trapped behind the deadlock.
// commit_atomic() is the pure-core entry; flow commit() = the same core + rebase
// (ADR-037 D#5: one implementation, two entry points). This is the EXACT scenario
// commit_refuses_on_rebase_conflict refuses — here --atomic must SUCCEED.
#[test]
fn commit_atomic_commits_without_rebase_through_a_conflict() {
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
    let werk = werk_base.join("kade-9201");
    git(&home, &["worktree", "add", "-b", "kade/9201", werk.to_str().unwrap(), "origin/main"]);

    // my work edits the line one way; the peer edits the SAME line on origin/main
    // — the conflict the flow commit() refuses with rebase-conflict.
    fs::write(werk.join("conflict.txt"), "kade-line\n").unwrap();
    fs::write(origin.join("conflict.txt"), "peer-line\n").unwrap();
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-q", "-m", "peer conflicting edit"]);

    // --atomic: commit WITHOUT rebase. Must SUCCEED where flow commit() deadlocks.
    let sha = werk_commit::commit_atomic(9201, "kade", "atomic escape", &home, &werk_base)
        .expect("commit --atomic must succeed without rebasing");
    assert!(sha.len() >= 7, "returns the commit sha");

    // card work committed LOCALLY, verbatim — no rebase applied.
    assert_eq!(fs::read_to_string(werk.join("conflict.txt")).unwrap(), "kade-line\n",
        "card work committed verbatim, no rebase");
    // deliberately did NOT pull the peer's change → still behind current main.
    git(&werk, &["fetch", "-q", "origin", "main"]);
    let behind = Command::new("git")
        .args(["-C", werk.to_str().unwrap(), "rev-list", "--count", "HEAD..origin/main"])
        .output().unwrap();
    assert_eq!(String::from_utf8_lossy(&behind.stdout).trim(), "1",
        "--atomic deliberately did not rebase — still one behind current main");
    // clean tree, on the card branch (no half-rebase, no deadlock).
    let status = Command::new("git")
        .args(["-C", werk.to_str().unwrap(), "status", "--porcelain"])
        .output().unwrap();
    assert!(String::from_utf8_lossy(&status.stdout).trim().is_empty(), "clean after the atomic commit");
}

// #3306 — fail LOUD when CHORUS_HOME isn't a git repo. Root-caused by Wren witnessing
// #3295: a bad CHORUS_HOME makes the verb die in lock() with a cryptic `os error 2`
// (the deployed binary was seen to exit 0 + emit nothing — self-masking). It must
// return a CLEAR, actionable Err BEFORE doing work, so the failure is visible — the
// reason "#3295 looked broken but wasn't". Verbs today only check CHORUS_HOME is SET,
// never that it's a git repo; this extends that guard.
#[test]
fn commit_fails_loud_when_home_is_not_a_git_repo() {
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
    let werk = werk_base.join("kade-9401");
    git(&home, &["worktree", "add", "-q", "-b", "kade/9401", werk.to_str().unwrap(), "origin/main"]);
    fs::write(werk.join("work.txt"), "card work").unwrap(); // dirty → it will try to commit

    // CHORUS_HOME pointed at a dir that is NOT a git repo (no .git) — Wren's ~/.chorus case.
    let bad_home = tmp("not-a-repo");

    let res = commit(9401, "kade", "msg", &bad_home, &werk_base);

    // MUST fail loud: a non-zero exit (Err), never a silent Ok/exit-0.
    let err = res.expect_err("must Err when CHORUS_HOME is not a git repo (never silently succeed)");
    // clear + actionable: names the CHORUS_HOME / not-a-git-repo problem, not just `os error 2`.
    let lc = err.to_lowercase();
    assert!(
        lc.contains("chorus_home") || lc.contains("not a git repo") || lc.contains("git repo"),
        "message must clearly name the CHORUS_HOME/git-repo problem, got: {}", err
    );
}

// ═══ #3304 — interactive in-verb conflict resolution ═══
// Shared scaffold: origin with file F, home clone, card werk that edits F, then a
// peer edits the SAME lines of F on origin/main → the rebase MUST conflict.
fn conflict_repo(card: u64) -> (PathBuf, PathBuf, PathBuf) {
    let origin = tmp("origin");
    git(&origin, &["init", "-q", "-b", "main", "."]);
    fs::write(origin.join("f.txt"), "base\n").unwrap();
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-q", "-m", "init"]);
    git(&origin, &["config", "receive.denyCurrentBranch", "ignore"]);

    let home = tmp("home");
    assert!(Command::new("git")
        .args(["clone", "-q", origin.to_str().unwrap(), home.to_str().unwrap()])
        .status().unwrap().success());

    let werk_base = tmp("werk");
    let werk = werk_base.join(format!("kade-{}", card));
    git(&home, &["worktree", "add", "-q", "-b", &format!("kade/{}", card), werk.to_str().unwrap(), "origin/main"]);
    fs::write(werk.join("f.txt"), "card-version\n").unwrap();

    // peer rewrites the same line on origin/main AFTER the werk forked.
    fs::write(origin.join("f.txt"), "peer-version\n").unwrap();
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-q", "-m", "peer moved main"]);
    (home, werk_base, werk)
}

fn rebase_in_progress(werk: &Path) -> bool {
    let out = Command::new("git")
        .args(["-C", werk.to_str().unwrap(), "rev-parse", "--git-path", "rebase-merge"])
        .output().unwrap();
    Path::new(String::from_utf8_lossy(&out.stdout).trim()).exists() || {
        let out = Command::new("git")
            .args(["-C", werk.to_str().unwrap(), "rev-parse", "--git-path", "rebase-apply"])
            .output().unwrap();
        Path::new(String::from_utf8_lossy(&out.stdout).trim()).exists()
    }
}

// AC1: a rebase conflict HOLDS — markers left in the werk for the human, no
// abort-and-refuse. The Err names the in-verb follow-ups, never raw git.
// AC2 + AC5: the human edits f.txt, then `--continue` finishes the rebase
// INTERNALLY (sentinel set) and the commit lands on current main — end to end
// through the verb.
#[test]
fn conflict_holds_markers_then_continue_lands_through_the_verb() {
    let (home, werk_base, werk) = conflict_repo(9501);

    let err = commit(9501, "kade", "conflict test", &home, &werk_base)
        .expect_err("conflicting rebase must hold, not succeed");
    assert!(err.contains("--continue") && err.contains("--abort"),
        "held message names both verb follow-ups: {err}");
    assert!(!err.to_lowercase().contains("git rebase"), "never instructs raw git: {err}");

    // the werk HOLDS the conflict: markers present, rebase in progress.
    let f = fs::read_to_string(werk.join("f.txt")).unwrap();
    assert!(f.contains("<<<<<<<") && f.contains(">>>>>>>"), "conflict markers left for the human: {f}");
    assert!(rebase_in_progress(&werk), "rebase held in progress, not aborted");

    // the human resolves by EDITING THE FILE (no git commands).
    fs::write(werk.join("f.txt"), "resolved-version\n").unwrap();

    let sha = werk_commit::commit_continue(9501, "kade", &home, &werk_base)
        .expect("--continue finishes the rebase through the verb");
    assert!(sha.len() >= 7);
    assert!(!rebase_in_progress(&werk), "rebase finished");
    let f = fs::read_to_string(werk.join("f.txt")).unwrap();
    assert_eq!(f, "resolved-version\n", "resolution content landed");
    // landed ON current main: exactly one card commit ahead, zero behind.
    git(&werk, &["fetch", "-q", "origin", "main"]);
    let ahead = Command::new("git")
        .args(["-C", werk.to_str().unwrap(), "rev-list", "--count", "origin/main..HEAD"])
        .output().unwrap();
    assert_eq!(String::from_utf8_lossy(&ahead.stdout).trim(), "1");
    let behind = Command::new("git")
        .args(["-C", werk.to_str().unwrap(), "rev-list", "--count", "HEAD..origin/main"])
        .output().unwrap();
    assert_eq!(String::from_utf8_lossy(&behind.stdout).trim(), "0");
}

// AC3: `--abort` restores the pre-rebase state through the verb — card commit
// preserved, markers gone, no rebase in progress.
#[test]
fn abort_restores_pre_rebase_state_through_the_verb() {
    let (home, werk_base, werk) = conflict_repo(9502);

    let _ = commit(9502, "kade", "conflict test", &home, &werk_base)
        .expect_err("conflicting rebase must hold");
    assert!(rebase_in_progress(&werk));
    let pre = Command::new("git")
        .args(["-C", werk.to_str().unwrap(), "rev-parse", "kade/9502"])
        .output().unwrap();
    let pre_sha = String::from_utf8_lossy(&pre.stdout).trim().to_string();

    let msg = werk_commit::commit_abort(9502, "kade", &home, &werk_base)
        .expect("--abort restores through the verb");
    assert!(!msg.is_empty());
    assert!(!rebase_in_progress(&werk), "rebase gone after abort");
    let now = Command::new("git")
        .args(["-C", werk.to_str().unwrap(), "rev-parse", "HEAD"])
        .output().unwrap();
    assert_eq!(String::from_utf8_lossy(&now.stdout).trim(), pre_sha,
        "HEAD restored to the pre-rebase card commit");
    let f = fs::read_to_string(werk.join("f.txt")).unwrap();
    assert_eq!(f, "card-version\n", "card content restored, markers gone");
    let status = Command::new("git")
        .args(["-C", werk.to_str().unwrap(), "status", "--porcelain"])
        .output().unwrap();
    assert_eq!(String::from_utf8_lossy(&status.stdout).trim(), "", "werk clean after abort");
}

// #3304 — continue/abort without a held rebase is a typed refusal, not a crash.
#[test]
fn continue_and_abort_refuse_when_no_rebase_is_in_progress() {
    let (home, werk_base, _werk) = conflict_repo(9503);
    // no commit() ran — no rebase in progress.
    let e = werk_commit::commit_continue(9503, "kade", &home, &werk_base)
        .expect_err("--continue without a held rebase must refuse");
    assert!(e.contains("no rebase"), "typed no-rebase refusal: {e}");
    let e = werk_commit::commit_abort(9503, "kade", &home, &werk_base)
        .expect_err("--abort without a held rebase must refuse");
    assert!(e.contains("no rebase"), "typed no-rebase refusal: {e}");
}

// #3330 (#3324 matrix, werk-commit gaps) — no-werk / wrong-branch refusals driven,
// the success-path spine emit captured (was only asserted on refusals), and the
// #3304 continue-RE-conflict branch: a multi-commit replay where a LATER commit
// conflicts again after the first --continue → holds again, same contract.
#[test]
fn refusals_success_spine_and_continue_reconflict() {
    // origin with f.txt + g.txt; card werk edits BOTH in two commits; peer rewrites
    // both on main → replaying commit 1 conflicts, then commit 2 conflicts again.
    let origin = tmp("origin");
    git(&origin, &["init", "-q", "-b", "main", "."]);
    fs::write(origin.join("f.txt"), "base-f\n").unwrap();
    fs::write(origin.join("g.txt"), "base-g\n").unwrap();
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-q", "-m", "init"]);
    git(&origin, &["config", "receive.denyCurrentBranch", "ignore"]);
    let home = tmp("home");
    assert!(Command::new("git")
        .args(["clone", "-q", origin.to_str().unwrap(), home.to_str().unwrap()])
        .status().unwrap().success());
    let werk_base = tmp("werk");

    // spine capture at the absolute chorus-log path.
    let log = home.join("platform/scripts/chorus-log");
    fs::create_dir_all(log.parent().unwrap()).unwrap();
    let cap = home.join("spine-capture.txt");
    fs::write(&log, format!("#!/bin/sh\necho \"$@\" >> \"{}\"\n", cap.display())).unwrap();
    let mut perm = fs::metadata(&log).unwrap().permissions();
    use std::os::unix::fs::PermissionsExt;
    perm.set_mode(0o755);
    fs::set_permissions(&log, perm).unwrap();

    // ── no-werk: never pulled → typed refusal.
    let err = commit(9301, "kade", "x", &home, &werk_base).expect_err("no werk must refuse");
    assert!(err.contains("pull the card first"), "typed no-werk refusal: {err}");

    // ── wrong-branch: werk on another branch → typed refusal (the #2641 class).
    let werk = werk_base.join("kade-9302");
    git(&home, &["worktree", "add", "-q", "-b", "kade/other", werk.to_str().unwrap(), "origin/main"]);
    fs::write(werk.join("w.txt"), "x").unwrap();
    let err = commit(9302, "kade", "x", &home, &werk_base).expect_err("wrong branch must refuse");
    assert!(err.contains("refusing to commit"), "typed wrong-branch refusal: {err}");

    // ── success-path spine: a clean commit emits commit.completed with the sha.
    let werk3 = werk_base.join("kade-9303");
    git(&home, &["worktree", "add", "-q", "-b", "kade/9303", werk3.to_str().unwrap(), "origin/main"]);
    fs::write(werk3.join("ok.txt"), "y").unwrap();
    let sha = commit(9303, "kade", "clean", &home, &werk_base).expect("clean commit ok");
    let emitted = fs::read_to_string(&cap).unwrap_or_default();
    assert!(emitted.contains("commit.completed") && emitted.contains(&format!("sha={}", sha)),
        "success path reached the spine with the sha: {emitted}");

    // ── continue-re-conflict: TWO card commits each touching a file the peer
    // rewrote → first hold, edit f, --continue → the SECOND replayed commit
    // conflicts on g → holds AGAIN with the same in-verb instruction; edit g,
    // --continue → lands with both resolutions, 0 behind main.
    let werk4 = werk_base.join("kade-9304");
    git(&home, &["worktree", "add", "-q", "-b", "kade/9304", werk4.to_str().unwrap(), "origin/main"]);
    fs::write(werk4.join("f.txt"), "card-f\n").unwrap();
    git(&werk4, &["add", "."]);
    git(&werk4, &["commit", "-q", "-m", "kade: #9304 — f"]);
    fs::write(werk4.join("g.txt"), "card-g\n").unwrap();
    git(&werk4, &["add", "."]);
    git(&werk4, &["commit", "-q", "-m", "kade: #9304 — g"]);
    fs::write(origin.join("f.txt"), "peer-f\n").unwrap();
    fs::write(origin.join("g.txt"), "peer-g\n").unwrap();
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-q", "-m", "peer rewrites both"]);

    let err = commit(9304, "kade", "two commits", &home, &werk_base)
        .expect_err("first replay conflict holds");
    assert!(err.contains("--continue") && err.contains("f.txt"), "first hold names f.txt: {err}");
    fs::write(werk4.join("f.txt"), "resolved-f\n").unwrap();

    let err = werk_commit::commit_continue(9304, "kade", &home, &werk_base)
        .expect_err("the SECOND replayed commit must hold again");
    assert!(err.contains("--continue") && err.contains("g.txt"),
        "re-conflict holds with the same in-verb contract, naming g.txt: {err}");
    fs::write(werk4.join("g.txt"), "resolved-g\n").unwrap();

    let sha = werk_commit::commit_continue(9304, "kade", &home, &werk_base)
        .expect("second --continue finishes the whole replay");
    assert!(sha.len() >= 7);
    assert_eq!(fs::read_to_string(werk4.join("f.txt")).unwrap(), "resolved-f\n");
    assert_eq!(fs::read_to_string(werk4.join("g.txt")).unwrap(), "resolved-g\n");
    git(&werk4, &["fetch", "-q", "origin", "main"]);
    let behind = Command::new("git")
        .args(["-C", werk4.to_str().unwrap(), "rev-list", "--count", "HEAD..origin/main"])
        .output().unwrap();
    assert_eq!(String::from_utf8_lossy(&behind.stdout).trim(), "0", "landed on current main");
}
