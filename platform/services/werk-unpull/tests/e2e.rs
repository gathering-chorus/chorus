//! Real end-to-end (#3299): fixture home with logging shims (cards / chorus-werk /
//! role-state / chorus-log at the absolute platform/scripts path) + a real git
//! worktree as the card werk. Proves the TS-tool parity contract: refuse-if-dirty
//! BEFORE any teardown, the happy-path step order, every typed refusal, and the
//! idempotent already-Next / already-removed branches.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use werk_unpull::unpull;

// The shims read process-global env knobs (CARDS_*/WERK_*); serialize every test.
static ENV_LOCK: Mutex<()> = Mutex::new(());
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);
fn tmp(tag: &str) -> PathBuf {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("wunpull-{}-{}-{}-{}", tag, std::process::id(), n, seq));
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
fn read(p: &Path) -> String { fs::read_to_string(p).unwrap_or_default() }

/// home with shims (cards honors CARDS_STATUS/CARDS_OWNER/CARDS_MOVE_BEHAVIOR,
/// chorus-werk honors WERK_REMOVE_BEHAVIOR; both log calls) + a real card werk
/// (worktree of a tiny repo) at werk_base/kade-<card>. Returns (home, werk_base, calls, spine).
fn scenario(card: u64) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let home = tmp("home");
    let scripts = home.join("platform/scripts");
    fs::create_dir_all(&scripts).unwrap();
    let calls = home.join("calls.txt");
    let spine = home.join("spine.txt");
    write_exec(&scripts.join("cards"), &format!(
        "#!/bin/sh\necho \"cards $*\" >> \"{c}\"\ncase \"$1\" in\n view) [ \"${{CARDS_VIEW_EXIT:-0}}\" = 0 ] || exit \"$CARDS_VIEW_EXIT\"; echo \"{{ \\\"status\\\": \\\"${{CARDS_STATUS:-WIP}}\\\", \\\"owner\\\": \\\"${{CARDS_OWNER:-Kade}}\\\" }}\" ;;\n move) case \"${{CARDS_MOVE_BEHAVIOR:-ok}}\" in\n   ok) exit 0 ;;\n   already) echo \"card already in Next\" >&2; exit 1 ;;\n   fail) echo \"board unreachable\" >&2; exit 1 ;;\n esac ;;\nesac\nexit 0\n",
        c = calls.display()));
    write_exec(&scripts.join("chorus-werk"), &format!(
        "#!/bin/sh\necho \"chorus-werk $*\" >> \"{c}\"\ncase \"${{WERK_REMOVE_BEHAVIOR:-ok}}\" in\n  ok) exit 0 ;;\n  already) echo \"already removed\" >&2; exit 1 ;;\n  fail) echo \"refusing: werk is dirty\" >&2; exit 1 ;;\nesac\n",
        c = calls.display()));
    write_exec(&scripts.join("role-state"), &format!("#!/bin/sh\necho \"role-state $*\" >> \"{c}\"\nexit 0\n", c = calls.display()));
    write_exec(&scripts.join("chorus-log"), &format!("#!/bin/sh\necho \"$@\" >> \"{s}\"\n", s = spine.display()));

    // #3431: teardown is NATIVE — home must BE the canonical repo (as in
    // production, where CHORUS_HOME is the repo and holds platform/scripts).
    // bare origin + home clone + the card werk as a real worktree of home.
    let origin = tmp("origin");
    git(&origin, &["init", "-q", "--bare", "-b", "main", "."]);
    git(&home, &["init", "-q", "-b", "main", "."]);
    git(&home, &["remote", "add", "origin", origin.to_str().unwrap()]);
    fs::write(home.join("README"), "x\n").unwrap();
    git(&home, &["add", "."]);
    git(&home, &["commit", "-q", "-m", "init"]);
    git(&home, &["push", "-q", "origin", "main"]);
    let werk_base = tmp("werkbase");
    let werk = werk_base.join(format!("kade-{}", card));
    git(&home, &["worktree", "add", "-q", "-b", &format!("kade/{}", card), werk.to_str().unwrap(), "main"]);
    (home, werk_base, calls, spine)
}

fn reset_env() {
    for k in ["CARDS_STATUS", "CARDS_OWNER", "CARDS_VIEW_EXIT", "CARDS_MOVE_BEHAVIOR", "WERK_REMOVE_BEHAVIOR"] {
        std::env::remove_var(k);
    }
}

#[test]
fn happy_path_moves_next_removes_werk_idles_and_witnesses() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    reset_env();
    let (home, werk_base, calls, spine) = scenario(9001);
    let branch = unpull(9001, "kade", &home, &werk_base).expect("unpull ok");
    assert_eq!(branch, "kade/9001", "returns the prior branch");
    let c = read(&calls);
    // the contract order: view → move Next → (native teardown) → role-state idle.
    let idx = |s: &str| c.find(s).unwrap_or_else(|| panic!("missing call {s}: {c}"));
    assert!(idx("cards view 9001") < idx("cards move 9001 Next"), "{c}");
    assert!(idx("cards move 9001 Next") < idx("role-state kade idle"), "{c}");
    // #3431: teardown is native — no chorus-werk shell-out; real git state proves it.
    assert!(!c.contains("chorus-werk"), "no chorus-werk shell-out remains: {c}");
    assert!(!werk_base.join("kade-9001").exists(), "worktree dir removed natively");
    let s = read(&spine);
    assert!(s.contains("card.branch.closed"), "teardown spines card.branch.closed: {s}");
    assert!(s.contains("card.unpulled") && s.contains("prior_branch=kade/9001"), "contract event: {s}");
}

#[test]
fn dirty_werk_refuses_typed_before_any_teardown() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    reset_env();
    let (home, werk_base, calls, spine) = scenario(9002);
    fs::write(werk_base.join("kade-9002/uncommitted.txt"), "precious work\n").unwrap();
    git(&werk_base.join("kade-9002"), &["add", "."]);
    let err = unpull(9002, "kade", &home, &werk_base).expect_err("dirty werk must refuse");
    assert!(err.starts_with("werk-dirty"), "typed werk-dirty: {err}");
    let c = read(&calls);
    assert!(!c.contains("cards move"), "board untouched on refusal: {c}");
    assert!(werk_base.join("kade-9002").exists(), "werk NOT torn down — work preserved");
    assert!(werk_base.join("kade-9002/uncommitted.txt").exists(), "the file is still there");
    assert!(read(&spine).contains("reason=werk-dirty"), "refusal spined");
}

#[test]
fn board_state_refusals_are_typed() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    reset_env();
    let (home, werk_base, _calls, _spine) = scenario(9003);
    std::env::set_var("CARDS_STATUS", "Done");
    let err = unpull(9003, "kade", &home, &werk_base).expect_err("non-WIP refuses");
    assert!(err.starts_with("wrong-status"), "{err}");
    std::env::set_var("CARDS_STATUS", "WIP");
    std::env::set_var("CARDS_OWNER", "Silas");
    let err = unpull(9003, "kade", &home, &werk_base).expect_err("other role's card refuses");
    assert!(err.starts_with("wrong-owner"), "{err}");
    std::env::set_var("CARDS_OWNER", "Kade");
    std::env::set_var("CARDS_VIEW_EXIT", "3");
    let err = unpull(9003, "kade", &home, &werk_base).expect_err("unviewable card refuses");
    assert!(err.starts_with("card-not-found"), "{err}");
    reset_env();
}

#[test]
fn missing_werk_refuses_werk_not_initialized() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    reset_env();
    let (home, werk_base, _calls, _spine) = scenario(9004);
    fs::remove_dir_all(werk_base.join("kade-9004")).unwrap();
    let err = unpull(9004, "kade", &home, &werk_base).expect_err("no werk refuses");
    assert!(err.starts_with("werk-not-initialized"), "{err}");
}

#[test]
fn idempotent_already_next_completes() {
    // Partial-unpull re-run: the board flip already happened (already-Next), the
    // werk still stands — the re-run completes teardown and witnesses. (The
    // fully-torn-down re-run refuses at Step 2 werk-not-initialized by design —
    // covered by missing_werk_refuses_werk_not_initialized. #3431: the old
    // "werk dir present but remove says already-removed" shim state could never
    // occur natively, so that half is retired with the shell-out.)
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    reset_env();
    let (home, werk_base, calls, spine) = scenario(9005);
    std::env::set_var("CARDS_MOVE_BEHAVIOR", "already");
    let branch = unpull(9005, "kade", &home, &werk_base)
        .expect("a partial unpull re-runs to completion (idempotent)");
    reset_env();
    assert_eq!(branch, "kade/9005");
    let c = read(&calls);
    assert!(c.contains("role-state kade idle"), "ran to the end: {c}");
    assert!(!werk_base.join("kade-9005").exists(), "teardown completed natively");
    assert!(read(&spine).contains("card.unpulled"), "still witnesses completion");
}

#[test]
fn real_move_or_remove_failures_refuse_typed() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    reset_env();
    let (home, werk_base, _c, _s) = scenario(9006);
    std::env::set_var("CARDS_MOVE_BEHAVIOR", "fail");
    let err = unpull(9006, "kade", &home, &werk_base).expect_err("real move failure refuses");
    assert!(err.starts_with("move-fail"), "{err}");
    std::env::set_var("CARDS_MOVE_BEHAVIOR", "ok");
    // #3431: the real teardown-failure class is now UNMERGED WORK — a committed
    // (clean-tree) commit that never reached origin/main. The native two-tier
    // merge proof (#3014) must refuse rather than delete real work.
    let werk = werk_base.join("kade-9006");
    fs::write(werk.join("real-work.txt"), "committed, never merged\n").unwrap();
    git(&werk, &["add", "."]);
    git(&werk, &["commit", "-q", "-m", "unmerged work"]);
    let err = unpull(9006, "kade", &home, &werk_base).expect_err("unmerged branch refuses teardown");
    assert!(err.starts_with("branch-close-fail"), "{err}");
    assert!(err.contains("not on origin/main"), "refusal names the unmerged work: {err}");
    reset_env();
}
