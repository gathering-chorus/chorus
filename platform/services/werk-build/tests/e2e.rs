//! Real end-to-end: actual `git` on temp repos + PATH-shimmed `build-signed.sh`
//! and `gh`. Proves build() against ADR-032: builds the WERK's crate (not
//! canonical), emits the cdhash, refuses with no werk / wrong branch / no crate,
//! and is invariant (same source -> same cdhash). One env-mutating test fn so
//! PATH can't race other tests. The REAL cargo-level invariance is the #[ignore]
//! wrap of test-build-invariance.sh at the bottom (slow, needs signing).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use werk_build::build;

fn nanos() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
}

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("wb-{}-{}-{}", tag, std::process::id(), nanos()));
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

/// Add + commit a file in the werk so it shows in `git diff origin/main`.
fn commit_file(werk: &Path, rel: &str, body: &str) {
    let p = werk.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, body).unwrap();
    git(werk, &["add", "."]);
    git(werk, &["commit", "-q", "-m", "change"]);
}

#[test]
fn e2e_build_in_werk() {
    // --- shims on PATH: build-signed.sh (deterministic cdhash) + gh ---
    let bin = tmp("bin");
    // assert build-signed.sh is called with the WERK crate dir (3-arg form), build-only.
    write_exec(
        &bin.join("build-signed.sh"),
        "#!/bin/sh\n\
         echo \"$1\" >> \"$BS_LOG\"\n\
         [ \"$BUILD_SKIP_INSTALL\" = \"1\" ] || { echo 'build-signed: NOT build-only!' >&2; exit 9; }\n\
         echo \"build-signed: cargo build --release in $1\"\n\
         echo \"build-signed: cdhash=${BS_CDHASH:-DEADBEEF}\"\n\
         exit 0\n",
    );
    write_exec(&bin.join("gh"), "#!/bin/sh\nexit 0\n");
    std::env::set_var("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()));
    let bs_log = tmp("bslog").join("calls");
    std::env::set_var("BS_LOG", bs_log.to_str().unwrap());
    std::env::remove_var("CHORUS_TRACE_ID");

    // --- real git: origin + home clone + werk base ---
    let origin = tmp("origin");
    git(&origin, &["init", "-q", "-b", "main", "."]);
    fs::write(origin.join("README"), "x").unwrap();
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-q", "-m", "init"]);
    let home = tmp("home");
    assert!(Command::new("git")
        .args(["clone", "-q", origin.to_str().unwrap(), home.to_str().unwrap()])
        .status()
        .unwrap()
        .success());
    let werk_base = tmp("werk");

    // --- no werk yet -> refuse (never builds canonical) ---
    assert!(build(8001, "silas", &home, &werk_base).is_err(), "no werk => refuse");

    // --- create the card's werk on silas/8001 with a changed Rust crate ---
    // #3132: a crate is structurally a platform/services/<name>/ dir WITH a Cargo.toml
    // (a real crate always has one). Build is now GLOBAL — it enumerates the werk's
    // crates rather than diffing — so the fixture writes a proper crate manifest.
    let werk = werk_base.join("silas-8001");
    git(&home, &["worktree", "add", "-q", "-b", "silas/8001", werk.to_str().unwrap(), "origin/main"]);
    fs::create_dir_all(werk.join("platform/services/widget/src")).unwrap();
    fs::write(werk.join("platform/services/widget/Cargo.toml"), "[package]\nname=\"widget\"\n").unwrap();
    commit_file(&werk, "platform/services/widget/src/lib.rs", "// widget\n");

    // --- happy path: builds the werk crate, emits cdhash ---
    let summary = build(8001, "silas", &home, &werk_base).expect("happy build");
    assert_eq!(summary, "widget=DEADBEEF", "summary carries crate=cdhash");
    let bs_calls = fs::read_to_string(&bs_log).unwrap_or_default();
    assert!(
        bs_calls.contains(werk.join("platform/services/widget").to_str().unwrap()),
        "build-signed.sh got the WERK crate dir (not canonical): {}",
        bs_calls
    );

    // --- invariance: same source -> same cdhash twice ---
    let again = build(8001, "silas", &home, &werk_base).expect("rebuild");
    assert_eq!(summary, again, "same source commit => same cdhash (invariance)");

    // --- #3107: no Rust crate / TS service changed -> no-op success, not refuse.
    // Docs-only / config-only / graph-only cards have no build cycle; build returns
    // Ok("") and werk-deploy handles whatever artifacts there are. The verb's job
    // is "compile what needs compiling, then get out of the way."
    let werk2 = werk_base.join("silas-8002");
    git(&home, &["worktree", "add", "-q", "-b", "silas/8002", werk2.to_str().unwrap(), "origin/main"]);
    commit_file(&werk2, "roles/silas/notes.md", "just docs\n");
    let no_cycle = build(8002, "silas", &home, &werk_base).expect("docs-only => no-op success");
    assert_eq!(no_cycle, "", "no-build-cycle returns empty summary");

    // --- wrong branch -> refuse ---
    let werk3 = werk_base.join("silas-8003");
    git(&home, &["worktree", "add", "-q", "-b", "wrong/branch", werk3.to_str().unwrap(), "origin/main"]);
    assert!(build(8003, "silas", &home, &werk_base).is_err(), "branch mismatch => refuse");
}

/// AC3 real cargo-level build-invariance — wraps test-build-invariance.sh
/// (cargo clean + build twice, assert same artifact hash). Slow + needs the
/// signing identity, so #[ignore] by default; run on demand / nightly:
///   cargo test --test e2e -- --ignored real_build_invariance
#[test]
#[ignore]
fn real_build_invariance() {
    let home = std::env::var("CHORUS_HOME")
        .unwrap_or_else(|_| "/Users/jeffbridwell/CascadeProjects/chorus".to_string());
    let script = format!("{}/platform/scripts/test-build-invariance.sh", home);
    let status = Command::new("bash").arg(&script).status().expect("run test-build-invariance.sh");
    assert!(status.success(), "build-invariance violated (test-build-invariance.sh failed)");
}
