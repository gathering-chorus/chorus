// @domain: builds — placed by chorus-crawl's domain rules (#4538)
//! #4710 AC4 — src/lib.rs `prod_source_gate` / `canonical_ff_sync`. What Jeff
//! sees: a prod deploy ships only what GitHub's main holds. Borg Queen's bypass
//! 2 (10-23): a prod deploy ran from local main, so a commit made on this Mac
//! and never pushed could ship. Real git fixtures: a bare "origin" and a
//! "canonical" clone, never the live repo.
use werk_deploy::{canonical_ff_sync, prod_source_gate, prod_source_verdict};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn tmp(tag: &str) -> PathBuf {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let d = std::env::temp_dir().join(format!("wd-4710-{}-{}-{}", tag, std::process::id(), n));
    fs::create_dir_all(&d).unwrap();
    d
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git").arg("-C").arg(dir).args(args)
        .env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t").env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t")
        .output().unwrap();
    assert!(out.status.success(), "git {:?}: {}", args, String::from_utf8_lossy(&out.stderr));
}

fn commit(dir: &Path, file: &str, subject: &str) {
    fs::write(dir.join(file), subject).unwrap();
    git(dir, &["add", file]);
    git(dir, &["commit", "-q", "-m", subject]);
}

/// origin (bare) with one landed commit for #4710, and canonical cloned from it.
fn fixture(tag: &str) -> (PathBuf, PathBuf) {
    let root = tmp(tag);
    let origin = root.join("origin.git");
    let seed = root.join("seed");
    let canonical = root.join("canonical");
    git(&root, &["init", "-q", "--bare", "-b", "main", origin.to_str().unwrap()]);
    git(&root, &["clone", "-q", origin.to_str().unwrap(), seed.to_str().unwrap()]);
    commit(&seed, "a", "#4700 (kade) (#1800)");
    commit(&seed, "b", "#4710 (silas) (#1832)");
    git(&seed, &["push", "-q", "origin", "HEAD:main"]);
    git(&root, &["clone", "-q", origin.to_str().unwrap(), canonical.to_str().unwrap()]);
    (canonical, seed)
}

#[test]
fn a_card_landed_on_origin_main_may_deploy() {
    let (canonical, _) = fixture("ok");
    prod_source_gate(canonical.to_str().unwrap(), 4710).unwrap();
    canonical_ff_sync(canonical.to_str().unwrap()).unwrap();
}

#[test]
fn negative_proof_a_local_only_commit_on_canonical_main_is_refused() {
    let (canonical, _) = fixture("local");
    commit(&canonical, "c", "#4710 (silas) local only, never pushed");
    let e = prod_source_gate(canonical.to_str().unwrap(), 4710).unwrap_err();
    assert!(e.contains("canonical holds 1 commit(s) that origin/main does not"), "{e}");
    // and the build-source sync no longer lets it through when canonical is not also behind
    let e = canonical_ff_sync(canonical.to_str().unwrap()).unwrap_err();
    assert!(e.contains("holds 1 commit(s) that origin/main does not"), "{e}");
}

#[test]
fn negative_proof_a_card_with_no_commit_on_origin_main_is_refused() {
    let (canonical, _) = fixture("absent");
    let e = prod_source_gate(canonical.to_str().unwrap(), 4711).unwrap_err();
    assert!(e.contains("origin/main holds no merge commit for #4711"), "{e}");
}

#[test]
fn a_commit_pushed_to_origin_after_the_clone_is_seen_by_the_fetch() {
    let (canonical, seed) = fixture("later");
    commit(&seed, "d", "#4712 (wren) (#1833)");
    git(&seed, &["push", "-q", "origin", "HEAD:main"]);
    prod_source_gate(canonical.to_str().unwrap(), 4712).unwrap();
}

#[test]
fn negative_proof_no_origin_to_fetch_is_refused_not_passed() {
    let root = tmp("noorigin");
    git(&root, &["init", "-q", "-b", "main"]);
    commit(&root, "a", "#4710 (silas) (#1832)");
    assert!(prod_source_gate(root.to_str().unwrap(), 4710).is_err(), "a repo with no origin cannot prove its source");
}

#[test]
fn the_verdict_matches_the_card_number_exactly() {
    assert!(prod_source_verdict(0, &["#47100 (x)"], 4710).is_err(), "#47100 is not #4710");
    assert!(prod_source_verdict(0, &["silas: #4710 (#9)"], 4710).is_ok());
    assert!(prod_source_verdict(0, &["#4710 (silas)"], 0).is_err(), "card 0 (no card) never deploys to prod");
}
