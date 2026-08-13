//! Real end-to-end: actual `git` on temp repos + PATH-shimmed `cards` and `gh`.
//! Proves pull() against the v6 diagram (gh = process holder): worktree off
//! origin/main, branch pushed so gh has a ref, gh state registered, board WIP —
//! and all-or-nothing rollback when a later step fails. One env-mutating test fn
//! so PATH / CARDS_* / GH_* can't race other tests.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::sync::Mutex;
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

// Both e2e fns mutate process env (PATH / CARDS_* / GH_*); serialize them.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn remote_has(home: &Path, branch: &str) -> bool {
    let out = Command::new("git")
        .args(["-C", home.to_str().unwrap(), "ls-remote", "--heads", "origin", branch])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).contains(branch)
}

#[test]
fn e2e_pull_to_gh_diagram() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    // --- gh shim on PATH (production keeps gh bare — system binary). cards +
    // role-state are NOT on PATH here on purpose: #3151 resolves them at
    // $CHORUS_HOME/platform/scripts, so they're shimmed there (below). Keeping
    // cards off PATH makes this a real guard — a revert to bare-PATH lookup fails. ---
    let bin = tmp("bin");
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
    // #3151: pull() now resolves `cards`/`role-state` at $CHORUS_HOME/platform/scripts
    // (absolute, PATH-independent — the chorus-mcp daemon's PATH lacks platform/scripts).
    // Place the shims where production actually looks, not just on PATH. (gh stays on
    // PATH — production keeps gh bare since it's a system binary.)
    let scripts = home.join("platform/scripts");
    fs::create_dir_all(&scripts).unwrap();
    write_exec(
        &scripts.join("cards"),
        "#!/bin/sh\ncase \"$1\" in\n view) echo \"{ \\\"status\\\": \\\"${CARDS_STATUS:-Next}\\\" }\" ;;\n move) exit \"${CARDS_MOVE_EXIT:-0}\" ;;\n *) exit 0 ;;\nesac\n",
    );
    write_exec(&scripts.join("role-state"), "#!/bin/sh\nexit 0\n");
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

// #3330 (#3324 matrix, werk-pull gaps) — the refusal taxonomy driven end-to-end:
// card-not-found, wrong-status, werk-exists-on-wrong-branch; plus the
// gh-register-fail rollback RESTORING the card's prior board status (the audit
// found e2e only checked worktree teardown), all witnessed on the spine.
#[test]
fn refusals_are_typed_spined_and_gh_fail_restores_board_status() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let bin = tmp("bin2");
    write_exec(&bin.join("gh"), "#!/bin/sh\necho \"$@\" >> \"$GH_LOG\"\nexit \"${GH_EXIT:-0}\"\n");
    std::env::set_var("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()));
    std::env::set_var("GH_LOG", tmp("ghlog2").join("calls").to_str().unwrap());
    std::env::set_var("GH_EXIT", "0");

    let origin = tmp("origin2");
    git(&origin, &["init", "-q", "-b", "main", "."]);
    fs::write(origin.join("README"), "x").unwrap();
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-q", "-m", "init"]);
    git(&origin, &["config", "receive.denyCurrentBranch", "ignore"]);
    let home = tmp("home2");
    assert!(Command::new("git")
        .args(["clone", "-q", origin.to_str().unwrap(), home.to_str().unwrap()])
        .status().unwrap().success());

    // shims at the #3151 absolute path: cards logs its MOVE calls (so the
    // restore-status assertion reads ground truth), view honors knobs.
    let scripts = home.join("platform/scripts");
    fs::create_dir_all(&scripts).unwrap();
    let moves = home.join("moves.txt");
    write_exec(&scripts.join("cards"), &format!(
        "#!/bin/sh\ncase \"$1\" in\n view) [ \"${{CARDS_VIEW_EXIT:-0}}\" = 0 ] || exit \"$CARDS_VIEW_EXIT\"; echo \"{{ \\\"status\\\": \\\"${{CARDS_STATUS:-Next}}\\\" }}\" ;;\n move) echo \"$2 $3\" >> \"{}\"; exit 0 ;;\n *) exit 0 ;;\nesac\n",
        moves.display()));
    write_exec(&scripts.join("role-state"), "#!/bin/sh\nexit 0\n");
    let cap = home.join("spine-capture.txt");
    write_exec(&scripts.join("chorus-log"), &format!("#!/bin/sh\necho \"$@\" >> \"{}\"\n", cap.display()));
    let werk_base = tmp("werk2");

    // ── card-not-found: cards view fails → typed refusal, spine'd, nothing made.
    std::env::set_var("CARDS_VIEW_EXIT", "3");
    let err = pull(9201, "kade", &home, &werk_base).expect_err("missing card must refuse");
    assert!(err.to_lowercase().contains("card"), "names the card problem: {err}");
    std::env::set_var("CARDS_VIEW_EXIT", "0");
    assert!(!werk_base.join("kade-9201").exists(), "no werk created on refusal");
    let emitted = fs::read_to_string(&cap).unwrap_or_default();
    assert!(emitted.contains("pull.refused") && emitted.contains("reason=card-not-found"),
        "card-not-found reached the spine: {emitted}");

    // ── wrong-status: a Done card can't be pulled → typed refusal, spine'd.
    std::env::set_var("CARDS_STATUS", "Done");
    let err = pull(9202, "kade", &home, &werk_base).expect_err("Done card must refuse");
    assert!(err.contains("not Next/Later"), "typed wrong-status refusal: {err}");
    std::env::set_var("CARDS_STATUS", "Next");
    assert!(fs::read_to_string(&cap).unwrap_or_default().contains("reason=wrong-status"),
        "wrong-status reached the spine");

    // ── werk exists on the WRONG branch (the #2641 same-role-wrong-card class):
    // a stale dir at the card's werk slot must refuse, never silently reuse.
    let stale = werk_base.join("kade-9203");
    git(&home, &["worktree", "add", "-q", "-b", "kade/other", stale.to_str().unwrap(), "origin/main"]);
    let err = pull(9203, "kade", &home, &werk_base).expect_err("wrong-branch werk must refuse");
    assert!(err.contains("branch") || err.contains("exists"), "names the conflict: {err}");

    // ── gh-register-fail: full rollback INCLUDING the board status restore.
    // The card was Later before the pull; gh fails after the WIP move → the move
    // log must show WIP then the RESTORE back to Later, and the werk is gone.
    std::env::set_var("CARDS_STATUS", "Later");
    std::env::set_var("GH_EXIT", "1");
    let err = pull(9204, "kade", &home, &werk_base).expect_err("gh fail must roll back");
    std::env::set_var("GH_EXIT", "0");
    assert!(err.contains("rolled back"), "refusal names the rollback: {err}");
    assert!(!werk_base.join("kade-9204").exists(), "worktree removed by rollback");
    assert!(!remote_has(&home, "kade/9204"), "no remote ref (pull never pushes)");
    let mv = fs::read_to_string(&moves).unwrap_or_default();
    assert!(mv.contains("9204 WIP"), "card was moved to WIP first: {mv}");
    assert!(mv.contains("9204 Later"), "rollback RESTORED the prior status (all-or-nothing): {mv}");
    let emitted = fs::read_to_string(&cap).unwrap_or_default();
    assert!(emitted.contains("pull.rolledback") && emitted.contains("reason=gh-register-fail"),
        "rollback reached the spine: {emitted}");
}

// #3842 — repo-aware pull, end to end on real git: a card labeled repo:<name>
// gets its worktree from the TARGET repo's origin/main, not chorus. AC1 + AC5
// (fixture-repo variant; the live shared-security run happens post-land).
#[test]
fn e2e_pull_into_declared_target_repo() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let bin = tmp("bin2");
    write_exec(&bin.join("gh"), "#!/bin/sh\nexit 0\n");
    std::env::set_var(
        "PATH",
        format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()),
    );

    // one projects dir holding BOTH repos, the ADR-041 sibling layout
    let projects = tmp("projects2");
    let origin_chorus = tmp("oc2");
    let origin_target = tmp("ot2");
    for (origin, marker) in [(&origin_chorus, "chorus"), (&origin_target, "shared-security")] {
        git(origin, &["init", "-q", "-b", "main", "."]);
        fs::write(origin.join("WHICH"), marker).unwrap();
        git(origin, &["add", "."]);
        git(origin, &["commit", "-q", "-m", "init"]);
        git(origin, &["config", "receive.denyCurrentBranch", "ignore"]);
    }
    let home = projects.join("chorus");
    let target = projects.join("shared-security");
    for (origin, clone) in [(&origin_chorus, &home), (&origin_target, &target)] {
        assert!(Command::new("git")
            .args(["clone", "-q", origin.to_str().unwrap(), clone.to_str().unwrap()])
            .status().unwrap().success());
    }
    let scripts = home.join("platform/scripts");
    fs::create_dir_all(&scripts).unwrap();
    // cards shim: card json carries the repo label in domains
    write_exec(
        &scripts.join("cards"),
        "#!/bin/sh\ncase \"$1\" in\n view) echo \"{ \\\"status\\\": \\\"Next\\\", \\\"domains\\\": [\\\"repo:shared-security\\\"] }\" ;;\n *) exit 0 ;;\nesac\n",
    );
    write_exec(&scripts.join("role-state"), "#!/bin/sh\nexit 0\n");
    std::env::remove_var("CARDS_STATUS");
    std::env::remove_var("CARDS_MOVE_EXIT");
    std::env::set_var("GH_EXIT", "0");

    let werk_base = tmp("werk2");
    let branch = pull(7101, "kade", &home, &werk_base).expect("target-repo pull");
    assert_eq!(branch, "kade/7101");
    let werk = werk_base.join("kade-7101");
    // the worktree's content is the TARGET repo's, not chorus's
    let which = fs::read_to_string(werk.join("WHICH")).expect("worktree has target content");
    assert_eq!(which, "shared-security", "worktree came from the target repo");
    // and the worktree is administered by the target repo, not home
    let out = Command::new("git")
        .args(["-C", target.to_str().unwrap(), "worktree", "list"])
        .output().unwrap();
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("kade-7101"),
        "target repo administers the worktree"
    );
}
