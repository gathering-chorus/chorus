// @domain: builds — placed by chorus-crawl's domain rules (#4538)
//! #4494 — a new werk's crate starts from prod's build cache (APFS clone of
//! canonical's target/), so a card compiles what it changed, not everything.
//! #4465 run 2 (2026-10-11): a card that touched no Rust built for 16 minutes.
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use werk_build::{account_target_dir, seed_account_target, should_seed_target};

fn tmp(tag: &str) -> PathBuf {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let d = std::env::temp_dir().join(format!("wb-4494-{}-{}", tag, n));
    fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn only_an_empty_werk_is_seeded_and_only_from_a_cache_that_exists() {
    assert!(should_seed_target(false, true));
    assert!(!should_seed_target(true, true), "a werk's own builds are never replaced");
    assert!(!should_seed_target(false, false));
}

// #4555 — one workspace: the cache is the checkout's folder for the account.
fn seed(werk: &PathBuf, canon: &PathBuf) -> String {
    let dst = account_target_dir(werk.to_str().unwrap(), "acct");
    seed_account_target(&dst, canon.to_str().unwrap(), "acct", "workspace")
}

#[test]
fn a_new_werk_gets_canonicals_build_cache() {
    let (canon, werk) = (tmp("canon"), tmp("werk"));
    fs::create_dir_all(canon.join("target/acct/release/deps")).unwrap();
    fs::write(canon.join("target/acct/release/deps/librocks.rlib"), "built").unwrap();
    let line = seed(&werk, &canon);
    assert!(line.starts_with("seeded workspace in "), "{line}");
    assert_eq!(fs::read_to_string(werk.join("target/acct/release/deps/librocks.rlib")).unwrap(), "built");
    assert!(!werk.join("target/acct.seeding").exists(), "the side name is gone after the rename");
}

#[test]
fn negative_proof_a_werk_that_already_built_keeps_its_own_cache() {
    let (canon, werk) = (tmp("canon2"), tmp("werk2"));
    fs::create_dir_all(canon.join("target/acct")).unwrap();
    fs::write(canon.join("target/acct/marker"), "prod").unwrap();
    fs::create_dir_all(werk.join("target/acct")).unwrap();
    fs::write(werk.join("target/acct/marker"), "this card").unwrap();
    assert_eq!(seed(&werk, &canon), "kept workspace");
    assert_eq!(fs::read_to_string(werk.join("target/acct/marker")).unwrap(), "this card");
}

#[test]
fn no_cache_in_prod_means_a_cold_build_named_as_one() {
    let (canon, werk) = (tmp("canon3"), tmp("werk3"));
    assert_eq!(seed(&werk, &canon), "cold workspace");
}

#[test]
fn negative_proof_another_accounts_folder_is_never_cloned_into_this_one() {
    // #4555: canonical's bare target/ holds every account's folder; seeding from it
    // would nest them all inside this account's folder
    let (canon, werk) = (tmp("canon4"), tmp("werk4"));
    fs::create_dir_all(canon.join("target/other-acct/release")).unwrap();
    assert_eq!(seed(&werk, &canon), "cold workspace");
    assert!(!werk.join("target/acct/other-acct").exists());
}
