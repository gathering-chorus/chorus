//! #4494 — a new werk's crate starts from prod's build cache (APFS clone of
//! canonical's target/), so a card compiles what it changed, not everything.
//! #4465 run 2 (2026-10-11): a card that touched no Rust built for 16 minutes.
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use werk_build::{seed_target, should_seed_target};

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

#[test]
fn a_new_werk_gets_canonicals_build_cache() {
    let (canon, werk) = (tmp("canon"), tmp("werk"));
    let rel = "platform/services/demo";
    fs::create_dir_all(canon.join(rel).join("target/release/deps")).unwrap();
    fs::write(canon.join(rel).join("target/release/deps/librocks.rlib"), "built").unwrap();
    fs::create_dir_all(werk.join(rel)).unwrap();
    let line = seed_target(werk.to_str().unwrap(), canon.to_str().unwrap(), rel);
    assert!(line.starts_with("seeded platform/services/demo in "), "{line}");
    assert_eq!(fs::read_to_string(werk.join(rel).join("target/release/deps/librocks.rlib")).unwrap(), "built");
    assert!(!werk.join(rel).join("target.seeding").exists(), "the side name is gone after the rename");
}

#[test]
fn negative_proof_a_werk_that_already_built_keeps_its_own_cache() {
    let (canon, werk) = (tmp("canon2"), tmp("werk2"));
    let rel = "platform/services/demo";
    fs::create_dir_all(canon.join(rel).join("target")).unwrap();
    fs::write(canon.join(rel).join("target/marker"), "prod").unwrap();
    fs::create_dir_all(werk.join(rel).join("target")).unwrap();
    fs::write(werk.join(rel).join("target/marker"), "this card").unwrap();
    let line = seed_target(werk.to_str().unwrap(), canon.to_str().unwrap(), rel);
    assert_eq!(line, "kept platform/services/demo");
    assert_eq!(fs::read_to_string(werk.join(rel).join("target/marker")).unwrap(), "this card");
}

#[test]
fn no_cache_in_prod_means_a_cold_build_named_as_one() {
    let (canon, werk) = (tmp("canon3"), tmp("werk3"));
    let line = seed_target(werk.to_str().unwrap(), canon.to_str().unwrap(), "platform/services/new");
    assert_eq!(line, "cold platform/services/new");
}
