//! Real end-to-end for the acp orchestration verb (#3176): actual `git` on a temp
//! repo + PATH-shimmed sub-verbs (werk-commit/push/deploy/accept). werk-acp's job
//! is COMPOSITION + the gating contract, so the shims stand in for the real verbs
//! (which have their own tests) and record the order + the DEPLOY_ROLE each saw.
//!
//! The centerpiece (AC2/AC6): a failing werk-deploy must fail the acp BEFORE
//! werk-accept runs — accept never fires, so the source never merges. That is the
//! merged≠live fix, proven by the absence of the accept marker.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use werk_acp::acp;

// PATH is process-global; cargo runs tests in parallel threads of one process, so
// serialize the PATH-mutating bodies under one lock.
static ENV_LOCK: Mutex<()> = Mutex::new(());
// Atomic per-call counter → unique temp dirs even at the same nanosecond (the
// parallel-collision class fixed on #3186).
static SEQ: AtomicU64 = AtomicU64::new(0);

fn nanos() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
}
fn tmp(tag: &str) -> PathBuf {
    let s = SEQ.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("wa-{}-{}-{}-{}", tag, std::process::id(), nanos(), s));
    fs::create_dir_all(&p).unwrap();
    p
}
fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(args).current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t")
        .status().unwrap().success();
    assert!(ok, "git {:?} failed in {}", args, dir.display());
}

/// A shim sub-verb: records "<name>:<DEPLOY_ROLE>" to the seq file (so the test
/// can assert order AND which identity each step ran as), then exits `code`.
fn shim(bin_dir: &Path, name: &str, seq_file: &Path, code: i32) {
    let body = format!(
        "#!/bin/sh\necho \"{name}:$DEPLOY_ROLE\" >> \"{seq}\"\nexit {code}\n",
        name = name, seq = seq_file.display(), code = code
    );
    let p = bin_dir.join(name);
    fs::write(&p, body).unwrap();
    let mut perm = fs::metadata(&p).unwrap().permissions();
    perm.set_mode(0o755);
    fs::set_permissions(&p, perm).unwrap();
}

/// origin + home clone + a card worktree on builder/<card>. Returns (home, werk_base).
fn setup(card: u64, builder: &str) -> (PathBuf, PathBuf) {
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
    let werk = werk_base.join(format!("{}-{}", builder, card));
    git(&home, &["worktree", "add", "-q", "-b", &format!("{}/{}", builder, card), werk.to_str().unwrap(), "origin/main"]);
    (home, werk_base)
}

/// Run acp with `bin` prepended to PATH (shims resolve there; real git still found).
fn acp_with_shims(card: u64, builder: &str, accepter: &str, home: &Path, werk_base: &Path, bin: &Path)
    -> werk_acp::R<String>
{
    let orig = std::env::var("PATH").unwrap();
    std::env::set_var("PATH", format!("{}:{}", bin.display(), orig));
    let r = acp(card, builder, accepter, home, werk_base);
    std::env::set_var("PATH", &orig);
    r
}

fn ran(seq: &Path) -> Vec<String> {
    fs::read_to_string(seq).unwrap_or_default().split_whitespace().map(|s| s.to_string()).collect()
}

#[test]
fn acp_composes_all_four_verbs_in_order_with_two_identities() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let card = 7701;
    let (home, werk_base) = setup(card, "kade");
    let bin = tmp("bin");
    let seqd = tmp("seq");
    let seq = seqd.join("seq.txt");
    for v in ["werk-commit", "werk-push", "werk-deploy", "werk-accept"] {
        shim(&bin, v, &seq, 0);
    }

    let r = acp_with_shims(card, "kade", "jeff", &home, &werk_base, &bin);
    assert!(r.is_ok(), "happy-path acp should compose cleanly: {:?}", r);

    // order is the contract; commit/push/deploy run as the BUILDER, accept as the
    // ACCEPTER (DEC-048 authority threading).
    assert_eq!(
        ran(&seq),
        vec!["werk-commit:kade", "werk-push:kade", "werk-deploy:kade", "werk-accept:jeff"],
        "verbs compose in order; accept runs as the accepter, the rest as the builder"
    );
}

#[test]
fn acp_fails_at_deploy_and_never_reaches_accept() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let card = 7702;
    let (home, werk_base) = setup(card, "kade");
    let bin = tmp("bin");
    let seqd = tmp("seq");
    let seq = seqd.join("seq.txt");
    shim(&bin, "werk-commit", &seq, 0);
    shim(&bin, "werk-push", &seq, 0);
    shim(&bin, "werk-deploy", &seq, 1); // prod did NOT come up == built → non-zero
    shim(&bin, "werk-accept", &seq, 0); // must NEVER run

    let r = acp_with_shims(card, "kade", "jeff", &home, &werk_base, &bin);

    let e = r.expect_err("a failed deploy must fail the acp");
    assert!(e.contains("deploy"), "fails at the deploy step: {}", e);
    assert!(e.contains("no merge") || e.contains("WIP"), "names that the merge did not happen: {}", e);

    // THE GATING PROOF: accept never fired → source never merged → no merged≠live.
    let order: Vec<String> = ran(&seq).iter().map(|s| s.split(':').next().unwrap().to_string()).collect();
    assert_eq!(order, vec!["werk-commit", "werk-push", "werk-deploy"], "stops at the failed deploy");
    assert!(!order.contains(&"werk-accept".to_string()), "ACCEPT NEVER RUNS on a failed deploy — the merged≠live fix");
}

#[test]
fn acp_refuses_when_no_werk() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let card = 7703;
    let (home, _wb) = setup(card, "kade");
    let empty_base = tmp("empty"); // no <kade-7703> werk under here
    let bin = tmp("bin");
    let seq = tmp("seq").join("seq.txt");
    for v in ["werk-commit", "werk-push", "werk-deploy", "werk-accept"] {
        shim(&bin, v, &seq, 0);
    }
    let r = acp_with_shims(card, "kade", "jeff", &home, &empty_base, &bin);
    let e = r.expect_err("no werk must refuse");
    assert!(e.contains("no werk"), "typed no-werk refusal: {}", e);
    assert!(ran(&seq).is_empty(), "no sub-verb runs when the werk is absent");
}
