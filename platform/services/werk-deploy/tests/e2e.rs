//! Real end-to-end: actual `git` on temp repos + PATH-shimmed werk-build,
//! chorus-bin-install, launchctl, codesign, gh. Proves deploy() against ADR-032 +
//! #3062 AC: guaranteed rebuild; the TWO slot targets (test-in-demo = werk slot,
//! no kickstart; test-in-prod = canonical, kickstart + running==built verify);
//! AC2 stale-build cdhash-divergence refuse; cdhash-mismatch → all-or-nothing
//! rollback; no-werk/branch refusals. One env-mutating test fn so PATH can't race.
//!
//! codesign shim is marker-based so pre-install (old binary) and post-install (new)
//! return different cdhashes: chorus-bin-install touches $CS_MARKER; codesign returns
//! $CS_CDHASH if the marker exists (post-install/new), else $CS_PRE (pre-install/old).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use werk_deploy::deploy;

fn nanos() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
}
fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("wd-{}-{}-{}", tag, std::process::id(), nanos()));
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

#[test]
fn e2e_deploy_both_slots_and_guards() {
    let bin = tmp("bin");
    let logd = tmp("logs");
    let bs = logd.join("build"); let inst = logd.join("install");
    let lc = logd.join("launchctl"); let csl = logd.join("codesign");
    let marker = logd.join("installed.marker");
    std::env::set_var("CS_MARKER", marker.to_str().unwrap());

    write_exec(&bin.join("werk-build"), &format!("#!/bin/sh\necho \"$@\" >> {bs:?}\necho 'chorus-inject=DEADBEEF'\n"));
    // install: log argv + touch the marker (so codesign returns the NEW cdhash after).
    write_exec(&bin.join("chorus-bin-install"), &format!("#!/bin/sh\necho \"$@\" >> {inst:?}\ntouch \"$CS_MARKER\"\nexit 0\n"));
    write_exec(&bin.join("launchctl"), &format!("#!/bin/sh\necho \"$@\" >> {lc:?}\nexit 0\n"));
    // marker present => post-install/new ($CS_CDHASH); absent => pre-install/old ($CS_PRE).
    write_exec(&bin.join("codesign"), &format!("#!/bin/sh\necho \"$@\" >> {csl:?}\nif [ -f \"$CS_MARKER\" ]; then echo \"CDHash=${{CS_CDHASH:-DEADBEEF}}\"; else echo \"CDHash=${{CS_PRE:-OLD000}}\"; fi\n"));
    write_exec(&bin.join("gh"), "#!/bin/sh\nexit 0\n");
    std::env::set_var("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()));
    std::env::remove_var("CHORUS_TRACE_ID");

    let origin = tmp("origin");
    git(&origin, &["init", "-q", "-b", "main", "."]);
    fs::write(origin.join("README"), "x").unwrap();
    git(&origin, &["add", "."]); git(&origin, &["commit", "-q", "-m", "init"]);
    let home = tmp("home");
    assert!(Command::new("git").args(["clone", "-q", origin.to_str().unwrap(), home.to_str().unwrap()]).status().unwrap().success());
    let werk_base = tmp("werk");
    std::env::set_var("CHORUS_BIN", tmp("chorusbin").to_str().unwrap());
    std::env::set_var("WERK_SILAS_BIN", tmp("werkbin").to_str().unwrap());

    // --- no werk → refuse ---
    assert!(deploy(7001, "silas", "canonical", &home, &werk_base).is_err(), "no werk => refuse");

    let werk = werk_base.join("silas-7001");
    git(&home, &["worktree", "add", "-q", "-b", "silas/7001", werk.to_str().unwrap(), "origin/main"]);
    fs::create_dir_all(werk.join("platform/services/chorus-inject/src")).unwrap();
    fs::write(werk.join("platform/services/chorus-inject/src/lib.rs"), "// w\n").unwrap();
    git(&werk, &["add", "."]); git(&werk, &["commit", "-q", "-m", "chorus-inject"]);

    // === TEST-IN-DEMO: target=werk → role slot, NO kickstart, NO verify ===
    let _ = fs::remove_file(&marker);
    let r = deploy(7001, "silas", "werk", &home, &werk_base).expect("demo-slot deploy");
    assert!(r.contains("chorus-inject=DEADBEEF") && r.contains("target=werk"), "demo summary: {}", r);
    assert!(read(&bs).contains("7001"), "werk-build ran (guaranteed rebuild)");
    assert!(read(&inst).contains("--target werk"), "installed to werk slot: {}", read(&inst));
    assert!(read(&lc).is_empty(), "DEMO must NOT kickstart: {}", read(&lc));

    // === TEST-IN-PROD: target=canonical, running==built → install, kickstart, verify ===
    fs::remove_file(&inst).ok(); fs::remove_file(&lc).ok(); fs::remove_file(&csl).ok();
    fs::remove_file(&marker).ok();           // pre-install: no marker => codesign returns CS_PRE (old)
    std::env::set_var("CS_PRE", "OLD000");   // running(old) != built(DEADBEEF) => no divergence
    std::env::set_var("CS_CDHASH", "DEADBEEF"); // post-install: running == built => verify passes
    let r = deploy(7001, "silas", "canonical", &home, &werk_base).expect("prod deploy, cdhash match");
    assert!(r.contains("target=canonical"), "prod summary: {}", r);
    assert!(read(&inst).contains("--target canonical"), "installed to canonical");
    assert!(read(&lc).contains("kickstart"), "PROD kickstarts: {}", read(&lc));
    assert!(read(&csl).contains("--verbose"), "running==built verify ran: {}", read(&csl));

    // === AC2 STALE-BUILD: rebuild gives the running cdhash WHILE source changed → refuse ===
    fs::remove_file(&inst).ok();
    fs::remove_file(&marker).ok();              // pre-install read
    std::env::set_var("CS_PRE", "DEADBEEF");    // running(old) == built(DEADBEEF) AND chorus-inject source changed
    let e = deploy(7001, "silas", "canonical", &home, &werk_base).expect_err("stale build must refuse");
    assert!(e.contains("cdhash-divergence"), "stale-build refuse: {}", e);
    assert!(!read(&inst).contains("--target canonical"), "stale build refused BEFORE install (nothing mutated): {}", read(&inst));

    // === cdhash MISMATCH post-install → all-or-nothing rollback ===
    fs::remove_file(&inst).ok();
    fs::remove_file(&marker).ok();
    std::env::set_var("CS_PRE", "OLD000");      // pre-install old != built => passes divergence
    std::env::set_var("CS_CDHASH", "STALE99");  // post-install != built => mismatch
    let e = deploy(7001, "silas", "canonical", &home, &werk_base).expect_err("mismatch must fail loud");
    assert!(e.contains("running != built") || e.to_lowercase().contains("rolled back"), "mismatch err: {}", e);
    assert!(read(&inst).contains("--rollback"), "rollback restored prior binary: {}", read(&inst));
    std::env::remove_var("CS_CDHASH"); std::env::remove_var("CS_PRE");

    // === branch mismatch → refuse ===
    let werk2 = werk_base.join("silas-7002");
    git(&home, &["worktree", "add", "-q", "-b", "wrong/branch", werk2.to_str().unwrap(), "origin/main"]);
    assert!(deploy(7002, "silas", "canonical", &home, &werk_base).is_err(), "branch mismatch => refuse");
}
