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

/// #3517 inc2 — a FRESH canonical-deploy fixture for the running-proof tests: shims (incl the `ps`
/// shim whose lstart is driven by $CHORUS_TEST_LSTART), a git origin+home clone, and the werk
/// silas-7001 carrying the chorus-inject crate + a committed com.chorus.inject.plist (which forces
/// target_class_in → RustService → the daemon path, so resolve_restarted runs). Marker removed →
/// the deploy runs FULLY (install→verify), never the unchanged-skip. Each outcome test gets its
/// OWN fixture (hermetic-by-construction; no shared marker to leak across Ok/Stale/Unknown).
/// Returns (home, werk_base). Caller holds ENV_LOCK + sets $CHORUS_TEST_LSTART for the outcome.
fn canon_running_proof_fixture() -> (PathBuf, PathBuf) {
    let bin = tmp("bin");
    let logd = tmp("logs");
    let bs = logd.join("build");
    let inst = logd.join("install");
    let lc = logd.join("launchctl");
    let marker = logd.join("installed.marker");
    std::env::set_var("CS_MARKER", marker.to_str().unwrap());
    write_exec(&bin.join("werk-build"), &format!("#!/bin/sh\necho \"$@\" >> {bs:?}\necho 'chorus-inject=DEADBEEF'\n"));
    write_exec(&bin.join("chorus-bin-install"), &format!("#!/bin/sh\necho \"$@\" >> {inst:?}\ntouch \"$CS_MARKER\"\nexit 0\n"));
    write_exec(&bin.join("launchctl"), &format!("#!/bin/sh\necho \"$@\" >> {lc:?}\nif [ \"$1\" = print ]; then echo 'state = running'; echo 'pid = 123'; fi\nexit 0\n"));
    write_exec(&bin.join("codesign"), &format!("#!/bin/sh\ncase \"$*\" in\n  *target/release*) echo \"CDHash=${{CS_BUILT:-DEADBEEF}}\" ;;\n  *) if [ -f \"$CS_MARKER\" ]; then echo \"CDHash=${{CS_CDHASH:-DEADBEEF}}\"; else echo \"CDHash=${{CS_PRE:-OLD000}}\"; fi ;;\nesac\n"));
    // ps shim: real `date` parses its lstart (the LC_ALL=C parse stays exercised). OLD→-1y→Stale,
    // EMPTY→none→Unknown, default→+1y→restarted→Ok.
    write_exec(&bin.join("ps"), "#!/bin/sh\ncase \"$*\" in\n  *lstart*)\n    case \"${CHORUS_TEST_LSTART:-FUTURE}\" in\n      OLD) date -v-1y '+%a %b %d %H:%M:%S %Y' ;;\n      EMPTY) : ;;\n      *) date -v+1y '+%a %b %d %H:%M:%S %Y' ;;\n    esac ;;\n  *) exit 0 ;;\nesac\n");
    write_exec(&bin.join("gh"), "#!/bin/sh\nexit 0\n");
    std::env::set_var("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()));
    std::env::remove_var("CHORUS_TRACE_ID");

    let origin = tmp("origin");
    git(&origin, &["init", "-q", "-b", "main", "."]);
    fs::write(origin.join("README"), "x").unwrap();
    fs::create_dir_all(origin.join("platform/services/chorus-inject/src")).unwrap();
    fs::write(origin.join("platform/services/chorus-inject/Cargo.toml"), "[package]\nname=\"chorus-inject\"\n").unwrap();
    fs::write(origin.join("platform/services/chorus-inject/src/lib.rs"), "// v0\n").unwrap();
    fs::create_dir_all(origin.join("config/launchagents")).unwrap();
    fs::write(origin.join("config/launchagents/com.chorus.inject.plist"), "<plist><string>chorus-inject</string></plist>").unwrap();
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-q", "-m", "init"]);
    let home = tmp("home");
    assert!(Command::new("git").args(["clone", "-q", origin.to_str().unwrap(), home.to_str().unwrap()]).status().unwrap().success());
    let werk_base = tmp("werk");
    std::env::set_var("CHORUS_BIN", tmp("chorusbin").to_str().unwrap());
    std::env::set_var("WERK_SILAS_BIN", tmp("werkbin").to_str().unwrap());
    std::env::set_var("CHORUS_DEPLOY_LIVENESS_TIMEOUT_S", "3");
    std::env::set_var("CHORUS_HOME", home.to_str().unwrap());
    let werk = werk_base.join("silas-7001");
    git(&home, &["worktree", "add", "-q", "-b", "silas/7001", werk.to_str().unwrap(), "origin/main"]);
    fs::write(werk.join("platform/services/chorus-inject/src/lib.rs"), "// w\n").unwrap();
    git(&werk, &["add", "."]);
    git(&werk, &["commit", "-q", "-m", "chorus-inject"]);
    let _ = fs::remove_file(&marker); // installed reads OLD until install → FULL deploy, not skip
    (home, werk_base)
}

// #3517 inc2 — the 06-04 STALE-DAEMON catch, ISOLATED: installed==built but the daemon did NOT
// restart onto the new binary (codesign-on-path alone would false-pass it). ps-shim → OLD lstart
// (< install_epoch) → restarted=false → Stale → kickstart -k ONCE → re-resolve (still OLD) → Err.
// The must-test case: proves the running-proof + bounded-retry-then-Err end-to-end.
// #3528 — macOS-only: drives the lstart shim via BSD `date -v` and hits the real `date -j`
// parse in resolve_restarted; both are absent on the Linux CI runner (ubuntu). The pure
// branch logic is covered cross-platform by lib.rs::running_verdict_tests.
#[cfg(target_os = "macos")]
#[test]
fn e2e_running_proof_stale_daemon_reds() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (home, werk_base) = canon_running_proof_fixture();
    std::env::set_var("CHORUS_TEST_LSTART", "OLD");
    let e = deploy(7001, "silas", "canonical", &home, &werk_base)
        .expect_err("stale daemon (didn't restart onto the built binary) must RED, not pass");
    std::env::remove_var("CHORUS_TEST_LSTART");
    std::env::remove_var("CHORUS_HOME");
    assert!(e.contains("stale-running") || e.contains("did not reload"), "06-04 stale gate fires: {}", e);
}

// #3517 inc2 — UNKNOWN, ISOLATED: PID/start-time unresolvable → RED (never silent-pass).
#[test]
fn e2e_running_proof_unknown_pid_reds() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (home, werk_base) = canon_running_proof_fixture();
    std::env::set_var("CHORUS_TEST_LSTART", "EMPTY");
    let e = deploy(7001, "silas", "canonical", &home, &werk_base)
        .expect_err("unresolvable running process must RED (unknown=RED)");
    std::env::remove_var("CHORUS_TEST_LSTART");
    std::env::remove_var("CHORUS_HOME");
    assert!(e.contains("unknown=RED") || e.contains("runtime unverified"), "unknown=RED gate fires: {}", e);
}

// #3528 — macOS-only: the prod-deploy path verifies running==built via launchd + BSD
// `date` (the FUTURE-lstart shim → restarted), absent on the Linux CI runner. Pure verdict
// logic is covered by lib.rs::running_verdict_tests; the platform-independent deploy guards
// run in the other e2e tests.
#[cfg(target_os = "macos")]
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
    // launchctl shim: log argv; answer `print` with a live job (state=running + pid) so
    // the #3317 post-kickstart liveness gate (bash #3232 port) sees the daemon up.
    write_exec(&bin.join("launchctl"), &format!("#!/bin/sh\necho \"$@\" >> {lc:?}\nif [ \"$1\" = print ]; then echo 'state = running'; echo 'pid = 123'; fi\nexit 0\n"));
    // #3517 inc2 — ps shim: resolve_restarted runs REAL `ps -p <pid> -o lstart=` against the
    // launchctl-shim's pid; answer with a controlled lstart so the e2e exercises the REAL LC_ALL=C
    // lstart→date-j parse (the fragile bit). Default +1y (>= install_epoch) → restarted → Ok.
    // $CHORUS_TEST_LSTART overrides: OLD → -1y → Stale; EMPTY → no output → None → Unknown.
    write_exec(&bin.join("ps"),
        "#!/bin/sh\ncase \"$*\" in\n  *lstart*)\n    case \"${CHORUS_TEST_LSTART:-FUTURE}\" in\n      OLD) date -v-1y '+%a %b %d %H:%M:%S %Y' ;;\n      EMPTY) : ;;\n      *) date -v+1y '+%a %b %d %H:%M:%S %Y' ;;\n    esac ;;\n  *) exit 0 ;;\nesac\n");
    // #3179 — path-aware: codesign of the WERK's BUILT file (…/target/release/…) is
    // always the freshly-built hash ($CS_BUILT, default DEADBEEF, matching werk-build's
    // echo). codesign of the INSTALLED file is marker-based: post-install/new
    // ($CS_CDHASH) vs pre-install/old ($CS_PRE). werk-deploy now verifies built==installed
    // per-file (not against the build summary's single per-crate hash), so the shim must
    // distinguish the two paths.
    write_exec(&bin.join("codesign"), &format!("#!/bin/sh\necho \"$@\" >> {csl:?}\ncase \"$*\" in\n  *target/release*) echo \"CDHash=${{CS_BUILT:-DEADBEEF}}\" ;;\n  *) if [ -f \"$CS_MARKER\" ]; then echo \"CDHash=${{CS_CDHASH:-DEADBEEF}}\"; else echo \"CDHash=${{CS_PRE:-OLD000}}\"; fi ;;\nesac\n"));
    write_exec(&bin.join("gh"), "#!/bin/sh\nexit 0\n");
    std::env::set_var("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()));
    std::env::remove_var("CHORUS_TRACE_ID");

    let origin = tmp("origin");
    git(&origin, &["init", "-q", "-b", "main", "."]);
    fs::write(origin.join("README"), "x").unwrap();
    // #3132/#3317: a Rust SERVICE is structurally a crate (Cargo.toml) WITH a committed
    // com.chorus.<svc> plist (the "is it kickstarted?" signal read from the repo, not
    // a hardcoded name). The crate lives on MAIN (the native canonical path classifies
    // + installs from canonical, the post-merge reality); the card then modifies it.
    fs::create_dir_all(origin.join("platform/services/chorus-inject/src")).unwrap();
    fs::write(origin.join("platform/services/chorus-inject/Cargo.toml"), "[package]\nname=\"chorus-inject\"\n").unwrap();
    fs::write(origin.join("platform/services/chorus-inject/src/lib.rs"), "// v0\n").unwrap();
    fs::create_dir_all(origin.join("config/launchagents")).unwrap();
    fs::write(origin.join("config/launchagents/com.chorus.inject.plist"),
        "<plist><string>chorus-inject</string></plist>").unwrap();
    git(&origin, &["add", "."]); git(&origin, &["commit", "-q", "-m", "init"]);
    let home = tmp("home");
    assert!(Command::new("git").args(["clone", "-q", origin.to_str().unwrap(), home.to_str().unwrap()]).status().unwrap().success());
    let werk_base = tmp("werk");
    std::env::set_var("CHORUS_BIN", tmp("chorusbin").to_str().unwrap());
    std::env::set_var("WERK_SILAS_BIN", tmp("werkbin").to_str().unwrap());
    std::env::set_var("CHORUS_DEPLOY_LIVENESS_TIMEOUT_S", "3");

    // --- no werk → refuse ---
    assert!(deploy(7001, "silas", "canonical", &home, &werk_base).is_err(), "no werk => refuse");

    let werk = werk_base.join("silas-7001");
    git(&home, &["worktree", "add", "-q", "-b", "silas/7001", werk.to_str().unwrap(), "origin/main"]);
    fs::write(werk.join("platform/services/chorus-inject/src/lib.rs"), "// w\n").unwrap();
    git(&werk, &["add", "."]); git(&werk, &["commit", "-q", "-m", "chorus-inject"]);

    // === TEST-IN-DEMO: target=werk → role slot, NO kickstart, NO verify ===
    let _ = fs::remove_file(&marker);
    let r = deploy(7001, "silas", "werk", &home, &werk_base).expect("demo-slot deploy");
    assert!(r.contains("chorus-inject=DEADBEEF") && r.contains("target=werk"), "demo summary: {}", r);
    assert!(read(&bs).contains("7001"), "werk-build ran (guaranteed rebuild)");
    assert!(read(&inst).contains("--target werk"), "installed to werk slot: {}", read(&inst));
    assert!(read(&lc).is_empty(), "DEMO must NOT kickstart: {}", read(&lc));

    // === TEST-IN-PROD: target=canonical → NATIVE build-from-main install (#3317) ===
    // #3222: canonical no longer builds from the werk. It derives the card's crate(s)
    // from the werk diff, builds them from CANONICAL via `werk-build --target canonical
    // --only <crates>` (the one structural build tool, crate-scoped), then installs/
    // verifies/kickstarts NATIVELY — the bash chorus-deploy shell-out seam is gone
    // (#3317 absorbed it). The install drives chorus-bin-install; the kickstart is
    // liveness-gated (bash #3232 port); cdhash verify is built==installed per binary.
    std::env::set_var("CHORUS_HOME", home.to_str().unwrap()); // canonical_root_path anchor

    fs::write(&bs, "").unwrap(); // reset werk-build call log
    let _ = fs::remove_file(&marker); // installed slot reads OLD until install runs
    fs::write(&inst, "").unwrap(); let _ = fs::write(&lc, "");
    let r = deploy(7001, "silas", "canonical", &home, &werk_base).expect("native prod deploy");
    assert!(r.contains("target=canonical") && r.contains("chorus-inject"), "prod summary: {}", r);
    // built from canonical, crate-scoped, via the one structural build tool
    assert!(read(&bs).contains("--target canonical"), "werk-build built from canonical: {}", read(&bs));
    assert!(read(&bs).contains("--only chorus-inject"), "crate-scoped to the card's crate: {}", read(&bs));
    // installed natively via chorus-bin-install to canonical (no chorus-deploy delegation)
    assert!(read(&inst).contains("--target canonical") && read(&inst).contains("chorus-inject"),
        "native install via chorus-bin-install: {}", read(&inst));
    // kickstarted the service and liveness-checked it (kickstart + print in the launchctl log)
    assert!(read(&lc).contains("kickstart") && read(&lc).contains("com.chorus.inject"),
        "kickstarted the launchd service: {}", read(&lc));
    assert!(read(&lc).contains("print"), "liveness-verified after kickstart (#3232 port): {}", read(&lc));

    // === AC2: a werk BEHIND origin/main still deploys — the werk-stale guard is GONE ===
    // A peer moves origin/main ahead so the werk is behind. The old path refused
    // "werk-stale"; the from-main path builds from main regardless (nothing to be stale
    // against), so canonical must deploy, not refuse.
    git(&origin, &["commit", "-q", "--allow-empty", "-m", "peer moves main ahead"]);
    let r2 = deploy(7001, "silas", "canonical", &home, &werk_base)
        .expect("behind-werk canonical must NOT refuse werk-stale (#3222 removed the guard)");
    assert!(r2.contains("target=canonical"), "behind werk still deploys from main: {}", r2);
    std::env::remove_var("CHORUS_HOME");

    // === branch mismatch → refuse (still guarded before the canonical dispatch) ===
    let werk2 = werk_base.join("silas-7002");
    git(&home, &["worktree", "add", "-q", "-b", "wrong/branch", werk2.to_str().unwrap(), "origin/main"]);
    assert!(deploy(7002, "silas", "canonical", &home, &werk_base).is_err(), "branch mismatch => refuse");
}

/// #3167 — a TRUE terminal Err (empty build summary: werk-build produced no
/// crate=cdhash pairs → "nothing to deploy") must witness `deploy.failed{reason}` to
/// ops/logs/werk-deploy.jsonl — distinct from rolledback (caught+reverted) and refused
/// (guard). RED before the died() instrumentation: this exit was silent, so a deploy
/// that died before it could roll back vanished with only a dangling deploy.started.
#[test]
fn e2e_deploy_failed_witnessed_on_died_not_rolledback() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let bin = tmp("dfbin");
    // werk-build exits 0 but emits NO crate=cdhash pairs → the empty-summary terminal Err.
    write_exec(&bin.join("werk-build"), "#!/bin/sh\nexit 0\n");
    std::env::set_var("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()));
    std::env::remove_var("CHORUS_TRACE_ID");

    let origin = tmp("dforigin");
    git(&origin, &["init", "-q", "-b", "main", "."]);
    fs::write(origin.join("README"), "x").unwrap();
    git(&origin, &["add", "."]); git(&origin, &["commit", "-q", "-m", "init"]);
    let home = tmp("dfhome");
    assert!(Command::new("git").args(["clone", "-q", origin.to_str().unwrap(), home.to_str().unwrap()]).status().unwrap().success());
    let werk_base = tmp("dfwerk");
    std::env::set_var("WERK_SILAS_BIN", tmp("dfwerkbin").to_str().unwrap());

    let werk = werk_base.join("silas-7301");
    git(&home, &["worktree", "add", "-q", "-b", "silas/7301", werk.to_str().unwrap(), "origin/main"]);

    // target=werk (demo) skips the canonical stale-guard, so we reach the build step,
    // which returns empty → the empty-summary terminal Err (a death, not a rollback).
    let e = deploy(7301, "silas", "werk", &home, &werk_base).expect_err("empty summary must Err");
    assert!(e.contains("no crate=cdhash") || e.contains("nothing to deploy"), "empty-summary err: {}", e);

    let witness = read(&home.join("ops/logs/werk-deploy.jsonl"));
    assert!(witness.contains("\"event\":\"deploy.failed\""),
        "deploy.failed witnessed on a died deploy (was silent): {}", witness);
    assert!(witness.contains("\"reason\":\"empty-summary\""),
        "deploy.failed carries the reason: {}", witness);
    assert!(witness.contains("\"card_id\":7301"), "deploy.failed is card-bound: {}", witness);
    assert!(!witness.contains("deploy.rolled_back"),
        "empty-summary is a death, not a rollback: {}", witness);
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
///
/// #3222 DEFERRAL: the converged target=canonical path (werk-build --target canonical +
/// chorus-deploy) covers RUST services + verbs — the merged≠live victims (chorus-hooks/mcp
/// daemons, werk-* verbs) and the bootstrap. TS SHARED-LIB canonical deploy (chorus-sdk
/// cascade + the anti-stale consumer-resolve verify this test pins) is NOT yet covered:
/// chorus-deploy has no shared-lib class, and changed_service_crates scopes to
/// platform/services/ (chorus-sdk lives at platform/chorus-sdk). Ignored, not silently
/// passed — the cascade/anti-stale logic must be ported into the converged path as a
/// follow-on before canonical shared-lib deploys go through #3222. Tracked on the card.
#[ignore = "#3222: canonical TS shared-lib cascade/anti-stale not yet in the converged path — follow-on"]
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

// #3186 AC4's werk-stale refuse guard was RETIRED by #3222: target=canonical no longer
// builds from the werk, so a behind-werk can't ship a stale tree — there is nothing to be
// stale against. The inverse (behind werk still deploys from main, no refuse) is now
// asserted in e2e_deploy_both_slots_and_guards' AC2 section. The old refuse test is gone
// because it asserted behavior we deliberately removed.

// #3320 — the self-deploy detach, end-to-end against the REAL binary. A chorus-mcp
// crate-mode deploy invoked with CHORUS_INVOKER=chorus-mcp must NOT run inline (the
// kickstart would kill the invoking daemon and drop the caller's response): it acks
// immediately (exit 0, transport survives) and hands off to a detached continuation
// carrying CHORUS_DETACHED=1 + the same trace. The continuation itself must run the
// NORMAL inline path (no respawn loop). Non-self deploys are unchanged.
#[test]
fn e2e_mcp_self_deploy_detaches_and_continuation_runs_inline() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let bin = env!("CARGO_BIN_EXE_werk-deploy");
    let home = tmp("selfdep");
    // stub continuation: logs argv + the detach-relevant env, so we can assert the
    // child got the crate-mode redeploy with CHORUS_DETACHED=1 and the same trace.
    let stub_log = home.join("stub.log");
    let stub = home.join("stub.sh");
    write_exec(&stub, &format!(
        "#!/bin/sh\necho \"argv=$* detached=$CHORUS_DETACHED trace=$CHORUS_TRACE_ID ghtok=$GH_TOKEN\" >> {:?}\n", stub_log
    ));
    // #3323 — gh shim: the PARENT (which still has keychain) captures `gh auth token`
    // and injects GH_TOKEN into the detached child's env, so the child's gh calls
    // survive the lost user security session (the post-#3320 401 class).
    let shim_bin = tmp("ghshim");
    write_exec(&shim_bin.join("gh"),
        "#!/bin/sh\nif [ \"$1 $2\" = \"auth token\" ]; then echo FAKE-TOKEN-3323; exit 0; fi\nexit 0\n");
    let shim_path = format!("{}:{}", shim_bin.display(), std::env::var("PATH").unwrap_or_default());

    // (1) self-deploy: invoker chorus-mcp → detach ack, exit 0, child = crate-mode redeploy.
    let out = Command::new(bin)
        .args(["crate", "chorus-mcp"])
        .env("CHORUS_HOME", &home)
        .env("CHORUS_INVOKER", "chorus-mcp")
        .env("WERK_DEPLOY_SELF_BIN", &stub)
        .env("CHORUS_TRACE_ID", "tr-3320")
        .env("PATH", &shim_path)
        .env_remove("CHORUS_DETACHED")
        .env_remove("GH_TOKEN")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "self-deploy must ack with exit 0 (transport survives): {}",
        String::from_utf8_lossy(&out.stderr));
    assert!(stdout.contains("deploy detached pid="), "ack names the detach: {stdout}");
    assert!(stdout.contains("trace=tr-3320"), "ack carries the poll trace: {stdout}");
    // the detached child got the crate-mode redeploy, marked detached, same trace.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while read(&stub_log).is_empty() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let child = read(&stub_log);
    assert!(child.contains("argv=crate chorus-mcp"), "continuation is a crate-mode redeploy: {child}");
    assert!(child.contains("detached=1"), "continuation carries CHORUS_DETACHED=1: {child}");
    assert!(child.contains("trace=tr-3320"), "continuation reuses the SAME trace: {child}");
    // #3323 — the parent captured the token and injected it into the child's env.
    assert!(child.contains("ghtok=FAKE-TOKEN-3323"),
        "detached child carries GH_TOKEN from the parent's keychain capture: {child}");
    // the parent did NOT emit deploy.completed — that's the child's to write (no false-green).
    let jsonl = read(&home.join("ops/logs/werk-deploy.jsonl"));
    assert!(!jsonl.contains("deploy.completed"), "ack must not false-emit deploy.completed: {jsonl}");
    assert!(jsonl.contains("deploy.detached"), "the detach itself is witnessed: {jsonl}");

    // (2) the continuation does NOT re-detach: with CHORUS_DETACHED=1 it runs the normal
    // inline path — in this bare home that's the dir-not-found refusal, NOT another spawn.
    let out2 = Command::new(bin)
        .args(["crate", "chorus-mcp"])
        .env("CHORUS_HOME", &home)
        .env("CHORUS_INVOKER", "chorus-mcp")
        .env("CHORUS_DETACHED", "1")
        .env("WERK_DEPLOY_SELF_BIN", &stub)
        .output()
        .unwrap();
    assert!(!out2.status.success(), "continuation runs inline (here: dir-not-found refusal)");
    let combined2 = format!("{}{}", String::from_utf8_lossy(&out2.stdout), String::from_utf8_lossy(&out2.stderr));
    assert!(combined2.contains("dir not found"), "inline path reached, no respawn: {combined2}");

    // (3) non-self deploy unchanged: no invoker env → inline path (same refusal), no detach ack.
    let out3 = Command::new(bin)
        .args(["crate", "chorus-mcp"])
        .env("CHORUS_HOME", &home)
        .env_remove("CHORUS_INVOKER")
        .env_remove("CHORUS_DETACHED")
        .output()
        .unwrap();
    let combined3 = format!("{}{}", String::from_utf8_lossy(&out3.stdout), String::from_utf8_lossy(&out3.stderr));
    assert!(!out3.status.success() && combined3.contains("dir not found"),
        "no invoker → inline path, never detached: {combined3}");
    assert!(!combined3.contains("deploy detached"), "no detach ack without the invoker: {combined3}");

    // (4) #3323 — token-capture failure is NON-FATAL but witnessed: with no working gh
    // on PATH the detach still proceeds (gh-less child better than no deploy), the ack
    // still returns exit 0, and the jsonl witnesses the missing token.
    let stub_log2 = home.join("stub2.log");
    let stub2 = home.join("stub2.sh");
    write_exec(&stub2, &format!(
        "#!/bin/sh\necho \"ghtok=${{GH_TOKEN:-ABSENT}}\" >> {:?}\n", stub_log2
    ));
    let no_gh = tmp("noghbin"); // empty dir — PATH has no gh at all
    let out4 = Command::new(bin)
        .args(["crate", "chorus-mcp"])
        .env("CHORUS_HOME", &home)
        .env("CHORUS_INVOKER", "chorus-mcp")
        .env("WERK_DEPLOY_SELF_BIN", &stub2)
        .env("CHORUS_TRACE_ID", "tr-3323-nogh")
        .env("PATH", no_gh.to_str().unwrap())
        .env_remove("CHORUS_DETACHED")
        .env_remove("GH_TOKEN")
        .output()
        .unwrap();
    assert!(out4.status.success(), "capture failure must not block the detach: {}",
        String::from_utf8_lossy(&out4.stderr));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while read(&stub_log2).is_empty() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(read(&stub_log2).contains("ghtok=ABSENT"), "no fake/empty token injected on capture failure");
    let jsonl = read(&home.join("ops/logs/werk-deploy.jsonl"));
    assert!(jsonl.contains("gh-token-capture"), "capture failure witnessed in the jsonl: {jsonl}");
}
