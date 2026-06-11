//! Real end-to-end (#3193 v2): temp repos + shimmed cards/chorus-log at the
//! absolute home path. Proves the floor records its objective findings to the
//! witness, the verdict recorder enforces the anti-ceremony rules against the
//! REAL witness state, and `check` reads the verdict back with the exit contract.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use werk_review::{check, floor, verdict};

// cards shim reads env knobs; serialize env-mutating tests.
static ENV_LOCK: Mutex<()> = Mutex::new(());
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);
fn tmp(tag: &str) -> PathBuf {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("wrev-{}-{}-{}-{}", tag, std::process::id(), n, seq));
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

/// home (shims) + origin/clone + card werk on kade/<card> with a committed change.
/// CARDS_AC controls the shimmed card view body. Returns (home, werk_base, spine).
fn scenario(card: u64, src_change: bool, with_test: bool) -> (PathBuf, PathBuf, PathBuf) {
    let home = tmp("home");
    let scripts = home.join("platform/scripts");
    fs::create_dir_all(&scripts).unwrap();
    let spine = home.join("spine.txt");
    write_exec(&scripts.join("cards"),
        "#!/bin/sh\nif [ \"$1\" = view ]; then printf '%s\\n' \"${CARDS_AC:-- [x] default checked}\"; fi\nexit 0\n");
    write_exec(&scripts.join("chorus-log"), &format!("#!/bin/sh\necho \"$@\" >> \"{}\"\n", spine.display()));

    let origin = tmp("origin");
    git(&origin, &["init", "-q", "-b", "main", "."]);
    fs::create_dir_all(origin.join("platform/services/x/src")).unwrap();
    fs::write(origin.join("platform/services/x/src/lib.rs"), "pub fn keep() {}\npub fn doomed() {}\n").unwrap();
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-q", "-m", "init"]);
    let clone = tmp("clone");
    assert!(Command::new("git")
        .args(["clone", "-q", origin.to_str().unwrap(), clone.to_str().unwrap()])
        .status().unwrap().success());
    let werk_base = tmp("werkbase");
    let werk = werk_base.join(format!("kade-{}", card));
    git(&clone, &["worktree", "add", "-q", "-b", &format!("kade/{}", card), werk.to_str().unwrap(), "origin/main"]);
    if src_change {
        // remove a pub symbol + change src (drives both objective checks).
        fs::write(werk.join("platform/services/x/src/lib.rs"), "pub fn keep() {}\n").unwrap();
    }
    if with_test {
        fs::create_dir_all(werk.join("platform/services/x/tests")).unwrap();
        fs::write(werk.join("platform/services/x/tests/e2e.rs"), "#[test] fn t() {}\n").unwrap();
    }
    git(&werk, &["add", "."]);
    git(&werk, &["commit", "-q", "-m", &format!("kade: #{} — work", card)]);
    (home, werk_base, spine)
}

#[test]
fn floor_records_objective_findings_to_the_witness() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    std::env::set_var("CARDS_AC", "- [x] built\n- [ ] still open AC item");
    let (home, werk_base, spine) = scenario(9101, true, false);
    let out = floor(9101, "kade", &home, &werk_base).expect("floor runs");
    std::env::remove_var("CARDS_AC");
    // the three objective findings: open AC box, src-without-test, removed pub symbol.
    assert!(out.contains("unchecked AC"), "open checkbox surfaced: {out}");
    assert!(out.contains("src changed without any test change"), "missing-test heuristic: {out}");
    assert!(out.contains("doomed"), "removed pub symbol named: {out}");
    let w = read(&home.join("ops/logs/werk-review.jsonl"));
    assert!(w.contains("\"event\":\"review.floor\"") && w.contains("\"card_id\":9101,"), "{w}");
    assert!(read(&spine).contains("review.floor"), "floor witnessed on the spine");
}

#[test]
fn floor_on_a_clean_diff_reports_clean() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    std::env::set_var("CARDS_AC", "- [x] everything done");
    let (home, werk_base, _spine) = scenario(9102, false, true);
    let out = floor(9102, "kade", &home, &werk_base).expect("floor runs");
    std::env::remove_var("CARDS_AC");
    assert!(out.contains("floor clean"), "no objective findings on a clean diff: {out}");
}

#[test]
fn verdict_enforces_anti_ceremony_against_the_real_witness() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    std::env::set_var("CARDS_AC", "- [x] done");
    let (home, werk_base, spine) = scenario(9103, false, true);
    // BEFORE the floor ran: any verdict is ceremony → rejected.
    let err = verdict(9103, true, "looks fine", &home).expect_err("verdict without floor run rejected");
    assert!(err.contains("floor"), "{err}");
    // run the floor, then: fail-with-empty-findings rejected; substantive verdicts record.
    floor(9103, "kade", &home, &werk_base).expect("floor ok");
    std::env::remove_var("CARDS_AC");
    assert!(verdict(9103, false, "", &home).is_err(), "empty-findings fail rejected");
    let msg = verdict(9103, false, "AC item 2 not covered; src/lib.rs:14 silently swallows Err", &home)
        .expect("substantive fail records");
    assert!(msg.contains("fail"), "{msg}");
    let w = read(&home.join("ops/logs/werk-review.jsonl"));
    assert!(w.contains("\"event\":\"review.verdict\"") && w.contains("\"verdict\":\"fail\""), "{w}");
    assert!(read(&spine).contains("review.verdict"), "verdict on the spine");
}

#[test]
fn check_reads_the_latest_verdict_with_the_exit_contract() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    std::env::set_var("CARDS_AC", "- [x] done");
    let (home, werk_base, _spine) = scenario(9104, false, true);
    // no verdict on record → check fails (missing = not reviewed).
    assert!(check(9104, &home).is_err(), "missing verdict = fail (hard-gate semantics)");
    floor(9104, "kade", &home, &werk_base).unwrap();
    std::env::remove_var("CARDS_AC");
    verdict(9104, false, "AC item 1 not covered", &home).unwrap();
    assert!(check(9104, &home).is_err(), "fail verdict = check fails");
    // a later pass supersedes (latest wins — the fix-then-re-review loop).
    verdict(9104, true, "re-reviewed after fix: AC 1 now covered by tests/e2e.rs", &home).unwrap();
    let ok = check(9104, &home).expect("pass verdict = check passes");
    assert!(ok.contains("pass"), "{ok}");
    // anti-collision: card 910's verdict must not satisfy 9104 (comma-terminated key).
    assert!(check(910, &home).is_err());
}

#[test]
fn floor_refuses_typed_on_missing_werk() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (home, werk_base, _spine) = scenario(9105, false, true);
    fs::remove_dir_all(werk_base.join("kade-9105")).unwrap();
    let err = floor(9105, "kade", &home, &werk_base).expect_err("no werk refuses");
    assert!(err.contains("no-werk"), "typed no-werk: {err}");
}
