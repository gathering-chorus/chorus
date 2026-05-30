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
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use werk_deploy::deploy;

// Both e2e tests mutate process env (PATH etc.); Rust runs tests in parallel by
// default, so serialize the env-mutating sections under one lock (std-only; avoids
// a serial_test dep). Held for the whole test body — env stays consistent per run.
static ENV_LOCK: Mutex<()> = Mutex::new(());

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
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
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
    // #3132: a Rust SERVICE is structurally a crate (Cargo.toml) WITH a committed
    // com.chorus.<svc> plist (the "is it kickstarted?" signal read from the repo, not
    // a hardcoded name). This synthetic crate stands in for the RustService path.
    fs::create_dir_all(werk.join("platform/services/chorus-inject/src")).unwrap();
    fs::write(werk.join("platform/services/chorus-inject/Cargo.toml"), "[package]\nname=\"chorus-inject\"\n").unwrap();
    fs::create_dir_all(werk.join("config/launchagents")).unwrap();
    fs::write(werk.join("config/launchagents/com.chorus.inject.plist"),
        "<plist><string>chorus-inject</string></plist>").unwrap();
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

/// #3126 — SHARED-LIBRARY deploy: cascade to graph-discovered consumers + the AC4
/// anti-stale verify. Proves the silent-stale-prod gap #3092 left is closed:
///  - werk-build emits `chorus-sdk=<H>`; the lib dist installs to canonical and its
///    sha verifies == H.
///  - the consumer set is DISCOVERED from the dependency graph (the consumer's
///    `file:` dep resolving to the lib dir), never hardcoded.
///  - each consumer's dist is cascaded to canonical, then anti-stale verify confirms
///    the chorus-sdk it RESOLVES at runtime (node_modules/chorus-sdk/dist) hashes to
///    the merged H.
///  - a consumer that resolves a STALE lib (vendored old copy) FAILS the deploy and
///    rolls back everything — a shared-lib change can't merge green while prod stale.
///
/// Real git/fs/symlink/sha256; werk-build is shimmed (it writes deterministic werk
/// dists + echoes their hash) so the test pins werk-deploy's orchestration, mirroring
/// how the existing e2e shims build-signed.sh.
#[test]
fn e2e_shared_lib_cascade_and_anti_stale() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let bin = tmp("slbin");
    // werk-build shim: write the werk's lib dist + each consumer's dist with
    // deterministic content, then echo `chorus-sdk=<sha-of-lib-dist>`. $PWD is the
    // werk (deploy runs werk-build with current_dir=werk).
    write_exec(
        &bin.join("werk-build"),
        "#!/bin/sh\n\
         W=$(pwd)\n\
         mkdir -p \"$W/platform/chorus-sdk/dist\"\n\
         printf '%s' \"${SL_LIB_CONTENT:-v1}\" > \"$W/platform/chorus-sdk/dist/index.js\"\n\
         mkdir -p \"$W/products/cards/dist\"; printf 'cli' > \"$W/products/cards/dist/cli.js\"\n\
         mkdir -p \"$W/products/vendored/dist\"; printf 'v' > \"$W/products/vendored/dist/main.js\"\n\
         SHA=$(cd \"$W/platform/chorus-sdk/dist\" && find . -type f | LC_ALL=C sort | xargs shasum -a 256 | shasum -a 256 | cut -d' ' -f1)\n\
         echo \"chorus-sdk=$SHA\"\n",
    );
    write_exec(&bin.join("gh"), "#!/bin/sh\nexit 0\n");
    std::env::set_var("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()));
    std::env::remove_var("CHORUS_TRACE_ID");

    // origin with the tracked package.json graph: a shared lib + two consumers, both
    // declaring it via `file:` deps (so discover_consumers finds them from the graph).
    let origin = tmp("slorigin");
    git(&origin, &["init", "-q", "-b", "main", "."]);
    fs::create_dir_all(origin.join("platform/chorus-sdk")).unwrap();
    fs::write(origin.join("platform/chorus-sdk/package.json"), r#"{ "name": "chorus-sdk", "main": "dist/index.js", "scripts": { "build": "tsc" } }"#).unwrap();
    fs::create_dir_all(origin.join("products/cards")).unwrap();
    fs::write(origin.join("products/cards/package.json"), r#"{ "name": "cards", "dependencies": { "chorus-sdk": "file:../../platform/chorus-sdk" } }"#).unwrap();
    fs::create_dir_all(origin.join("products/vendored")).unwrap();
    fs::write(origin.join("products/vendored/package.json"), r#"{ "name": "vendored", "dependencies": { "chorus-sdk": "file:../../platform/chorus-sdk" } }"#).unwrap();
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-q", "-m", "init graph"]);

    let home = tmp("slhome");
    assert!(Command::new("git").args(["clone", "-q", origin.to_str().unwrap(), home.to_str().unwrap()]).status().unwrap().success());
    // canonical_root_path() reads CHORUS_HOME (the verb is invoked with it set) —
    // point it at this test's canonical clone, not the session's real chorus root.
    std::env::set_var("CHORUS_HOME", home.to_str().unwrap());
    // canonical consumer wiring: `cards` resolves the lib via a symlink (the real
    // cards pattern) → it will see the freshly-deployed lib. `vendored` has a REAL
    // node_modules/chorus-sdk dir with STALE content → it must fail anti-stale.
    fs::create_dir_all(home.join("products/cards/node_modules")).unwrap();
    symlink("../../../platform/chorus-sdk", home.join("products/cards/node_modules/chorus-sdk")).unwrap();
    fs::create_dir_all(home.join("products/vendored/node_modules/chorus-sdk/dist")).unwrap();
    fs::write(home.join("products/vendored/node_modules/chorus-sdk/dist/index.js"), "STALE-OLD").unwrap();

    let werk_base = tmp("slwerk");

    // --- HAPPY: only `cards` in the graph (drop vendored from this werk) ---
    // Build a werk whose consumer graph is just cards (vendored removed) so the
    // happy path proves cascade + resolve-verify pass end-to-end.
    let werk = werk_base.join("silas-9101");
    git(&home, &["worktree", "add", "-q", "-b", "silas/9101", werk.to_str().unwrap(), "origin/main"]);
    // remove vendored from this werk's graph so only the symlink consumer is discovered.
    fs::remove_dir_all(werk.join("products/vendored")).unwrap();
    git(&werk, &["add", "-A"]);
    git(&werk, &["commit", "-q", "-m", "drop vendored; touch lib"]);
    // a lib change so discover_build_units sees the SharedLib unit.
    fs::write(werk.join("platform/chorus-sdk/src.ts"), "// changed\n").unwrap();
    git(&werk, &["add", "-A"]);
    git(&werk, &["commit", "-q", "-m", "lib change"]);

    std::env::set_var("SL_LIB_CONTENT", "v1");
    let r = deploy(9101, "silas", "canonical", &home, &werk_base).expect("shared-lib happy deploy");
    assert!(r.contains("chorus-sdk="), "summary carries the lib identity: {}", r);
    // lib dist landed in canonical.
    assert!(home.join("platform/chorus-sdk/dist/index.js").is_file(), "lib dist deployed to canonical");
    // cascade: cards dist landed in canonical.
    assert!(home.join("products/cards/dist/cli.js").is_file(), "consumer (cards) dist cascaded to canonical");
    // anti-stale verify passed: cards resolves the freshly-deployed lib (symlink → canonical lib dist).
    let resolved = read(&home.join("products/cards/node_modules/chorus-sdk/dist/index.js"));
    assert_eq!(resolved, "v1", "cards RESOLVES the new lib content (not stale) — the gap closed");

    // --- ANTI-STALE FAIL: a consumer that resolves a STALE vendored lib → refuse+rollback ---
    let werk2 = werk_base.join("silas-9102");
    git(&home, &["worktree", "add", "-q", "-b", "silas/9102", werk2.to_str().unwrap(), "origin/main"]);
    fs::write(werk2.join("platform/chorus-sdk/src.ts"), "// changed2\n").unwrap();
    git(&werk2, &["add", "-A"]);
    git(&werk2, &["commit", "-q", "-m", "lib change 2"]);
    // snapshot canonical lib dist content before the failing deploy (it had "v1").
    let lib_before = read(&home.join("platform/chorus-sdk/dist/index.js"));
    // the failing deploy builds DIFFERENT content ("v2") so a successful rollback to
    // "v1" is observable (not a coincidence of identical bytes).
    std::env::set_var("SL_LIB_CONTENT", "v2");
    let e = deploy(9102, "silas", "canonical", &home, &werk_base)
        .expect_err("a consumer resolving a stale lib must REFUSE (can't merge green while prod stale)");
    assert!(
        e.to_lowercase().contains("anti-stale") && e.contains("vendored"),
        "error must name the anti-stale guard + the offending consumer: {}",
        e
    );
    // rollback: canonical lib dist restored to its pre-deploy content (all-or-nothing).
    assert_eq!(read(&home.join("platform/chorus-sdk/dist/index.js")), lib_before, "failed deploy rolled the lib dist back");
    // the stale vendored copy was NOT silently accepted.
    assert_eq!(read(&home.join("products/vendored/node_modules/chorus-sdk/dist/index.js")), "STALE-OLD", "stale vendored lib untouched (deploy refused, not papered over)");
    std::env::remove_var("CHORUS_HOME");
    std::env::remove_var("SL_LIB_CONTENT");
}
