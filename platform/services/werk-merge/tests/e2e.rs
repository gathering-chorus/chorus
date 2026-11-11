// @domain: builds — placed by chorus-crawl's domain rules (#4538)
//! Real end-to-end for werk-merge after #4710: actual `git` on temp repos + a PATH-shimmed
//! `gh` that logs every call. werk-merge never merges: the lander squash-merges the PR as
//! the GitHub App on Jeff's signed go. Here the land only confirms the card's content is on
//! origin/main, and refuses otherwise without a push, a PR create or a merge.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use werk_merge::merge;

// Both e2e fns mutate process env (PATH / ORIGIN / GH_*); serialize them.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn nanos() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
}
fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("wmerge-{}-{}-{}", tag, std::process::id(), nanos()));
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

/// origin (non-bare on main, accepts pushes) + home clone + a card worktree on the
/// card branch. Returns (origin, home, werk_base, werk).
fn scenario(role: &str, card: u64) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
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
    let werk = werk_base.join(format!("{}-{}", role, card));
    git(&home, &["worktree", "add", "-b", &format!("{}/{}", role, card), werk.to_str().unwrap(), "origin/main"]);
    (origin, home, werk_base, werk)
}

/// Commit a file in the werk and push the branch to origin (real git push).
fn commit_and_push(werk: &Path, branch: &str, file: &str, content: &str) -> String {
    fs::write(werk.join(file), content).unwrap();
    git(werk, &["add", "."]);
    git(werk, &["commit", "-q", "-m", &format!("add {}", file)]);
    git(werk, &["push", "-q", "origin", branch]);
    git_out(werk, &["rev-parse", "HEAD"])
}

/// #3476 — commit WITHOUT pushing (the post-present werk-commit case): local HEAD
/// advances, origin's branch stays at its old oid.
fn commit_no_push(werk: &Path, file: &str, content: &str) -> String {
    fs::write(werk.join(file), content).unwrap();
    git(werk, &["add", "."]);
    git(werk, &["commit", "-q", "-m", &format!("add {} (unpushed)", file)]);
    git_out(werk, &["rev-parse", "HEAD"])
}

fn origin_main_has(origin: &Path, file: &str) -> bool {
    Command::new("git")
        .args(["-C", origin.to_str().unwrap(), "cat-file", "-e", &format!("main:{}", file)])
        .status().unwrap().success()
}

/// #4771 AC4 — the land's deploy credential: a 0600 file this account owns.
/// GitHub (the gh shim) accepts `expect`; the file holds `token`.
fn arm_cred(token: &str, expect: &str) -> PathBuf {
    let f = tmp("cred").join("github-deploy");
    fs::write(&f, token).unwrap();
    fs::set_permissions(&f, fs::Permissions::from_mode(0o600)).unwrap();
    std::env::set_var("WERK_DEPLOY_CREDENTIAL_FILE", f.to_str().unwrap());
    std::env::set_var("GH_EXPECT_TOKEN", expect);
    f
}


/// gh shim: log every call; `pr list` answers no PR (the confirm runs on content).
const GH_SHIM: &str = "#!/bin/sh\necho \"$@\" >> \"$GH_LOG\"\n[ \"$1 $2\" = \"pr list\" ] && echo '[]'\nexit 0\n";

fn setup(tag: &str) -> PathBuf {
    std::env::set_var("CHORUS_GO_OVERRIDE", "e2e-confirm-4710");
    let bin = tmp(tag);
    let go = bin.join("chorus-go");
    write_exec(&go, "#!/bin/sh\necho 'chorus-go: no passkey registered'\n");
    std::env::set_var("CHORUS_GO_BIN", go.to_str().unwrap());
    write_exec(&bin.join("gh"), GH_SHIM);
    let log = bin.join("gh.log");
    std::env::set_var("GH_LOG", log.to_str().unwrap());
    std::env::set_var("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()));
    arm_cred("deploy-cred-e2e", "deploy-cred-e2e");
    log
}

/// The lander's squash, done to the temp origin as GitHub would: the branch's content lands
/// on main as one commit titled `#<card> (<role>) (#<pr>)`.
fn lander_squash(origin: &Path, branch: &str, card: u64, role: &str) {
    git(origin, &["merge", "--squash", "-q", branch]);
    git(origin, &["commit", "-q", "-m", &format!("#{} ({}) (#7)", card, role)]);
}

#[test]
fn the_landers_squash_on_main_is_confirmed_and_werk_merge_calls_no_merge() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let log = setup("bin-ok");
    let (origin, home, werk_base, werk) = scenario("wren", 9201);
    commit_and_push(&werk, "wren/9201", "a.txt", "card work");
    lander_squash(&origin, "wren/9201", 9201, "wren");
    let sha = merge(9201, "wren", &home, &werk_base).expect("the lander merged it: confirmed");
    assert_eq!(sha, git_out(&origin, &["rev-parse", "main"]), "returns main's sha");
    let calls = fs::read_to_string(&log).unwrap_or_default();
    assert!(!calls.contains("pr merge") && !calls.contains("pr create"), "werk-merge merged or opened a PR: {calls}");
}

// NEGATIVE PROOF: with the card not on main, werk-merge refuses `not-on-main` and does not
// push, open a PR or merge — there is no fallback to merging here.
#[test]
fn not_on_main_is_refused_and_nothing_is_pushed_opened_or_merged() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let log = setup("bin-no");
    let (origin, home, werk_base, werk) = scenario("wren", 9202);
    commit_no_push(&werk, "b.txt", "card work");
    let e = merge(9202, "wren", &home, &werk_base).unwrap_err();
    assert!(e.contains("not on main") && e.contains("never merges"), "{e}");
    assert!(!origin_main_has(&origin, "b.txt"), "the card reached main");
    let branches = git_out(&origin, &["branch", "--list", "wren/9202"]);
    assert!(branches.is_empty(), "werk-merge pushed the branch: {branches}");
    let calls = fs::read_to_string(&log).unwrap_or_default();
    assert!(!calls.contains("pr merge") && !calls.contains("pr create"), "{calls}");
    let w = fs::read_to_string(home.join("ops/logs/werk-merge.jsonl")).unwrap_or_default();
    assert!(w.contains("\"reason\":\"not-on-main\""), "{w}");
}
