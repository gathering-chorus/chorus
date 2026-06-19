//! Real end-to-end for the push-only verb (#3056 split): actual `git` on temp
//! repos + a PATH-shimmed `gh`. Proves werk-push PUSHES a locally-committed branch
//! to origin and registers the gh chorus/push status — the visible state that
//! commit-only deliberately leaves absent. One env-mutating test fn (PATH/GH_*).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use werk_push::push;

// Both e2e fns mutate process env (PATH/GH_*); serialize them (werk-deploy pattern).
static ENV_LOCK: Mutex<()> = Mutex::new(());

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
fn remote_has(home: &Path, branch: &str) -> bool {
    let out = Command::new("git")
        .args(["-C", home.to_str().unwrap(), "ls-remote", "--heads", "origin", branch])
        .output().unwrap();
    String::from_utf8_lossy(&out.stdout).contains(branch)
}

#[test]
fn push_pushes_committed_branch_and_registers_gh() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
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

    // --- #3194: re-run after a sanctioned rebase. werk-commit rebases (rewrites
    // history), so the branch diverges from its OWN earlier push; a plain push is
    // non-ff. force-with-lease (expected = the remote-tracking ref from our first
    // push) re-points our own card branch to the rebased history. ---
    git(&origin, &["checkout", "-q", "main"]);
    fs::write(origin.join("peer.txt"), "p").unwrap();
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-q", "-m", "peer work on main"]);
    git(&werk, &["fetch", "-q", "origin", "main"]);
    git(&werk, &["rebase", "-q", "origin/main"]);
    let rebased = git_out(&werk, &["rev-parse", "HEAD"]);
    assert_ne!(rebased, sha, "rebase rewrote the commit -> diverged from the pushed sha");
    let sha2 = push(9002, "kade", &home, &werk_base)
        .expect("re-push after a sanctioned rebase must succeed (force-with-lease)");
    assert_eq!(sha2, rebased, "re-push lands the rebased sha on origin");

    // --- #3194 safety: force-with-lease must REFUSE (not clobber) when origin's
    // branch moved out-of-band — a peer pushed to it and our tracking ref is stale.
    // Proves it is a lease, not a blind force. ---
    let home2 = tmp("home2");
    assert!(Command::new("git")
        .args(["clone", "-q", origin.to_str().unwrap(), home2.to_str().unwrap()])
        .status().unwrap().success());
    git(&home2, &["checkout", "-q", "-B", "kade/9002", "origin/kade/9002"]);
    fs::write(home2.join("peer2.txt"), "q").unwrap();
    git(&home2, &["add", "."]);
    git(&home2, &["commit", "-q", "-m", "peer hijacks the card branch"]);
    git(&home2, &["push", "-q", "--force", "origin", "kade/9002"]);
    // werk diverges again; its tracking ref still points at sha2, origin is now peer's.
    fs::write(werk.join("w2.txt"), "x").unwrap();
    git(&werk, &["add", "."]);
    git(&werk, &["commit", "-q", "-m", "more local work"]);

    // #3163: a rejected push must surface push.failed on the ONE spine, keyed by card +
    // the INHERITED trace (not a fresh mint). Capture-shim chorus-log at the path
    // emit_spine invokes (home/platform/scripts/chorus-log); seed the inherited trace via
    // the /tmp file carrier resolve_trace reads (no env mutation -> hermetic under parallelism).
    let log = home.join("platform/scripts/chorus-log");
    fs::create_dir_all(log.parent().unwrap()).unwrap();
    let cap = home.join("spine-capture.txt");
    write_exec(&log, &format!("#!/bin/sh\necho \"$@\" >> \"{}\"\n", cap.display()));
    fs::write("/tmp/9002-trace", "inherited-trace-9002").unwrap();

    assert!(push(9002, "kade", &home, &werk_base).is_err(),
        "force-with-lease REFUSES when origin moved out-of-band (lease guard, not blind force)");

    // #3163: the refused push reached the spine with the INHERITED trace (was fully silent before).
    let emitted = fs::read_to_string(&cap).unwrap_or_default();
    assert!(emitted.contains("card=9002"), "push failure reached the spine keyed by card: {:?}", emitted);
    assert!(emitted.contains("trace=inherited-trace-9002"),
        "carries the INHERITED trace (#3045 contract), not a fresh mint: {:?}", emitted);
    assert!(emitted.contains("push.failed"), "a push.failed event reached the spine: {:?}", emitted);

    // nothing-to-push: a fresh card whose werk has no commits ahead -> refuse.
    let werk2 = werk_base.join("kade-9003");
    git(&home, &["worktree", "add", "-b", "kade/9003", werk2.to_str().unwrap(), "origin/main"]);
    assert!(push(9003, "kade", &home, &werk_base).is_err(), "nothing to push => refuse");

    // no-werk: never pulled -> refuse, no push.
    assert!(push(9999, "kade", &home, &werk_base).is_err(), "no werk => refuse");
}

// #3330 (#3324 matrix, werk-push gaps) — the three uncovered seams:
// wrong-branch (the #2580 cross-role guard's ONLY uncovered taxonomy entry),
// the _GIT_QUEUE_PUSH sanctioned-pusher sentinel actually reaching git, and the
// gh-register-fail rollback (the just-pushed ref deleted, no orphan).
#[test]
fn wrong_branch_refuses_sentinel_reaches_git_and_gh_fail_rolls_back() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let bin = tmp("bin2");
    write_exec(&bin.join("gh"), "#!/bin/sh\necho \"$@\" >> \"$GH_LOG\"\nexit \"${GH_EXIT:-0}\"\n");
    std::env::set_var("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()));
    let gh_log = tmp("ghlog2").join("calls");
    std::env::set_var("GH_LOG", gh_log.to_str().unwrap());
    std::env::set_var("GH_EXIT", "0");

    let origin = tmp("origin2");
    git(&origin, &["init", "-q", "-b", "main", "."]);
    fs::write(origin.join("README"), "x").unwrap();
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-q", "-m", "init"]);
    git(&origin, &["config", "receive.denyCurrentBranch", "ignore"]);
    let home = tmp("home3");
    assert!(Command::new("git")
        .args(["clone", "-q", origin.to_str().unwrap(), home.to_str().unwrap()])
        .status().unwrap().success());

    // spine capture (same seam the #3163 test uses).
    let log = home.join("platform/scripts/chorus-log");
    fs::create_dir_all(log.parent().unwrap()).unwrap();
    let cap = home.join("spine-capture.txt");
    write_exec(&log, &format!("#!/bin/sh\necho \"$@\" >> \"{}\"\n", cap.display()));

    let werk_base = tmp("werk3");
    let werk = werk_base.join("kade-9101");
    git(&home, &["worktree", "add", "-q", "-b", "kade/9101", werk.to_str().unwrap(), "origin/main"]);
    fs::write(werk.join("w.txt"), "x").unwrap();
    git(&werk, &["add", "."]);
    git(&werk, &["commit", "-q", "-m", "kade: #9101 — work"]);

    // ── wrong-branch: the werk drifts onto another branch → typed refusal, spine'd,
    // and NOTHING pushed (the #2580 cross-role class: a kade werk must only ever
    // push kade/<card>).
    git(&werk, &["checkout", "-q", "-b", "silas/999"]);
    let err = push(9101, "kade", &home, &werk_base).expect_err("wrong branch must refuse");
    assert!(err.contains("refusing to push"), "typed wrong-branch refusal: {err}");
    assert!(!remote_has(&home, "kade/9101") && !remote_has(&home, "silas/999"), "nothing pushed");
    let emitted = fs::read_to_string(&cap).unwrap_or_default();
    assert!(emitted.contains("push.refused") && emitted.contains("reason=wrong-branch"),
        "wrong-branch refusal reached the spine: {emitted}");
    // #3513 — RUNTIME PROOF: a REAL refusal carries the failureClass discriminator
    // (change-vs-tooling) in the emitted witness, not just in a unit test. wrong-branch
    // is pipeline mechanics → tooling. This is the live "show it in the wild" evidence.
    assert!(emitted.contains("failureClass=tooling"),
        "real push.refused emits failureClass=tooling at runtime (#3513 DORA discriminator): {emitted}");
    git(&werk, &["checkout", "-q", "kade/9101"]);

    // ── sentinel: a client-side pre-push hook that REJECTS unless the sanctioned-
    // pusher sentinel is in env (the real #2598 hook's contract). Raw git fails;
    // the verb passes — proof _GIT_QUEUE_PUSH=1 reaches the git subprocess.
    let hooks = home.join(".git/hooks");
    fs::create_dir_all(&hooks).unwrap();
    write_exec(&hooks.join("pre-push"),
        "#!/bin/sh\n[ \"$_GIT_QUEUE_PUSH\" = \"1\" ] || { echo 'BLOCKED: not sanctioned' >&2; exit 1; }\nexit 0\n");
    let raw = Command::new("git").args(["push", "origin", "kade/9101"]).current_dir(&werk).output().unwrap();
    assert!(!raw.status.success(), "raw git push (no sentinel) must be blocked by the hook");
    let sha = push(9101, "kade", &home, &werk_base).expect("verb push carries the sentinel through to git");
    assert!(remote_has(&home, "kade/9101"), "verb push landed");
    assert!(sha.len() >= 7);

    // ── gh-register-fail rollback: new commit so the push isn't idempotent, gh
    // fails → the just-pushed ref is DELETED (delete carries the sentinel too —
    // it must pass the same hook), refusal names the rollback, spine witnesses it.
    fs::write(werk.join("w2.txt"), "y").unwrap();
    git(&werk, &["add", "."]);
    git(&werk, &["commit", "-q", "-m", "kade: #9101 — more"]);
    std::env::set_var("GH_EXIT", "1");
    let err = push(9101, "kade", &home, &werk_base).expect_err("gh fail must refuse");
    std::env::set_var("GH_EXIT", "0");
    assert!(err.contains("deleted the pushed ref"), "refusal names the rollback: {err}");
    assert!(!remote_has(&home, "kade/9101"), "no orphan ref — the pushed branch was rolled back");
    let emitted = fs::read_to_string(&cap).unwrap_or_default();
    assert!(emitted.contains("push.rolledback") && emitted.contains("reason=gh-register-fail"),
        "rollback reached the spine: {emitted}");
}
