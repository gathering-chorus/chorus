// @domain: builds — placed by chorus-crawl's domain rules (#4538)
//! #4772 — each account builds in its own folder, <crate>/target/<account>.
//! #4805 run 2 (2026-10-31 05:30): bq's local cargo and the pipeline's
//! (jeffbridwell) wrote one target/; libsqlite3-sys build.rs:39 then panicked
//! "Could not copy bindings to output directory: Operation not permitted".
//!
//! The negative proof stands in for "a file another account owns" with a file
//! carrying the user-immutable flag: rewriting either one fails with the same
//! EPERM, and one account can make it inside a test.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use werk_build::{account_target_dir, current_account};

fn tmp(tag: &str) -> PathBuf {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let d = std::env::temp_dir().join(format!("wb-4772-{}-{}", tag, n));
    fs::create_dir_all(&d).unwrap();
    d
}

/// A crate whose build script copies a file into OUT_DIR, as libsqlite3-sys does.
fn fixture_crate() -> PathBuf {
    let c = tmp("crate");
    fs::write(c.join("Cargo.toml"), "[package]\nname = \"fx4772\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n").unwrap();
    fs::write(c.join("build.rs"), "fn main() {\n    println!(\"cargo:rerun-if-changed=bindings.rs\");\n    let out = std::path::Path::new(&std::env::var(\"OUT_DIR\").unwrap()).join(\"bindings.rs\");\n    std::fs::copy(\"bindings.rs\", out).expect(\"Could not copy bindings to output directory\");\n}\n").unwrap();
    fs::create_dir_all(c.join("src")).unwrap();
    fs::write(c.join("src/lib.rs"), "include!(concat!(env!(\"OUT_DIR\"), \"/bindings.rs\"));\n").unwrap();
    fs::write(c.join("bindings.rs"), "pub const V: u32 = 1;\n").unwrap();
    c
}

fn build(krate: &Path, target: &str) -> (bool, String) {
    let o = Command::new("cargo").args(["build", "--offline", "-q"])
        .env("CARGO_TARGET_DIR", target).current_dir(krate).output().unwrap();
    (o.status.success(), String::from_utf8_lossy(&o.stderr).to_string())
}

/// Lock every copied bindings.rs under `target`, as another account's file would be.
fn lock_copies(target: &str, on: bool) {
    for e in walk(Path::new(target)) {
        if e.file_name().map(|n| n == "bindings.rs").unwrap_or(false) {
            Command::new("chflags").args([if on { "uchg" } else { "nouchg" }, e.to_str().unwrap()]).status().unwrap();
        }
    }
}

fn walk(d: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(d) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() { out.extend(walk(&p)) } else { out.push(p) }
        }
    }
    out
}

#[test]
fn each_account_gets_its_own_folder_under_the_crates_target() {
    let a = account_target_dir("/w/platform/services/chorus-hooks", "chorus-borg-queen");
    let b = account_target_dir("/w/platform/services/chorus-hooks", "jeffbridwell");
    assert_eq!(a, "/w/platform/services/chorus-hooks/target/chorus-borg-queen");
    assert_ne!(a, b, "two accounts must never share a build folder");
    assert!(!current_account().is_empty() && current_account() != "unknown");
}

#[test]
fn a_shared_target_fails_with_eperm_and_the_accounts_own_folder_builds() {
    let krate = fixture_crate();
    let k = krate.to_str().unwrap();
    // the role builds first, in the crate's shared target/
    let shared = format!("{}/target", k);
    let (ok, err) = build(&krate, &shared);
    assert!(ok, "fixture must build once: {err}");
    lock_copies(&shared, true);
    fs::write(krate.join("bindings.rs"), "pub const V: u32 = 2;\n").unwrap();

    // negative proof: a second build into the same folder hits the locked copy
    let (ok, err) = build(&krate, &shared);
    assert!(!ok, "a build into a folder holding a file it may not rewrite must fail");
    assert!(err.contains("Operation not permitted"), "expected EPERM, got: {err}");

    // the fix: the second account builds in its own folder
    let (ok, err) = build(&krate, &account_target_dir(k, "jeffbridwell"));
    lock_copies(&shared, false);
    assert!(ok, "the account's own folder must build: {err}");
    let _ = fs::remove_dir_all(&krate);
}
