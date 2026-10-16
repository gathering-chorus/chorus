// @test-type: integration — drives the built werk-test against temp git werks and a loopback stub door; no live service
// @domain: tests
//! #4536 — the runner's own test. Each row is a tiny throwaway werk (a temp git
//! repo, never committed) run through the real `werk-test <card> <role>`
//! against a stub door, and must end in the one result its fail route names.
//! It never runs the product's suites and finishes in seconds.
//!
//! Rows land with the card that builds their route (plan 10-15): this card owns
//! 1 pass, 2 fail, 6 ran nothing, 9 store refused, 10 no werk. #4537 adds 3, 4,
//! 5, 11; #4539 adds 7 (quarantined) and 8 (needs: machine); 12 lands with the
//! membrane row.
//!
//! Hermetic: registry, quarantine, stack probe and the result store are one
//! TcpListener stub; `werk-demo` on PATH is a stub, so no badge is written;
//! CHORUS_HOME is removed, so no spine and no token mint.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

fn read_request(stream: &mut impl Read) -> String {
    let mut data = Vec::new();
    let mut buf = [0u8; 8192];
    let mut wanted = None;
    loop {
        let n = stream.read(&mut buf).unwrap_or(0);
        if n == 0 { break; }
        data.extend_from_slice(&buf[..n]);
        if wanted.is_none() {
            if let Some(pos) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&data[..pos + 4]).to_ascii_lowercase();
                let len = headers.lines()
                    .find(|l| l.starts_with("content-length:"))
                    .and_then(|l| l.split_once(':'))
                    .and_then(|(_, v)| v.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                wanted = Some(pos + 4 + len);
            }
        }
        if wanted.is_some_and(|l| data.len() >= l) { break; }
    }
    String::from_utf8_lossy(&data).into_owned()
}

/// GET /tests → the registry; PATCH /tests/<name> → `patch_status`, kept;
/// anything else (stack probe, suite-run post) → 200.
fn stub(registry: String, patch_status: u16) -> (u16, Arc<Mutex<Vec<String>>>) {
    let l = TcpListener::bind("127.0.0.1:0").expect("stub bind");
    let port = l.local_addr().unwrap().port();
    let patches = Arc::new(Mutex::new(Vec::new()));
    let p2 = Arc::clone(&patches);
    std::thread::spawn(move || {
        for c in l.incoming() {
            let Ok(mut c) = c else { continue };
            let req = read_request(&mut c);
            let first = req.lines().next().unwrap_or("").to_string();
            let resp = if first.starts_with("GET ") && first.contains("/tests") {
                format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    registry.len(), registry)
            } else if first.starts_with("PATCH ") {
                p2.lock().unwrap().push(req.clone());
                format!("HTTP/1.1 {} X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", patch_status)
            } else {
                "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_string()
            };
            let _ = c.write_all(resp.as_bytes());
        }
    });
    (port, patches)
}

fn row(file: &str, test: &str, entity: &str) -> String {
    format!("{{\"filePath\":\"{file}\",\"covers\":\"werk\",\"testType\":\"unit\",\
             \"testName\":\"{test}\",\"name\":\"{entity}\",\"hermeticity\":\"hermetic\"}}")
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git").arg("-C").arg(dir).args(args)
        .env("GIT_AUTHOR_NAME", "f").env("GIT_AUTHOR_EMAIL", "f@f")
        .env("GIT_COMMITTER_NAME", "f").env("GIT_COMMITTER_EMAIL", "f@f")
        .output().map(|o| o.status.success()).unwrap_or(false);
    assert!(ok, "git {:?} in {}", args, dir.display());
}

struct Run { code: i32, out: String, patches: Vec<String> }

/// One fixture: a werk at <base>/fix-<card> whose last commit adds `files`.
fn run_fixture(tag: &str, card: &str, files: &[(&str, &str)], registry_rows: &[String], patch_status: u16) -> Run {
    let base = std::env::temp_dir().join(format!("werk-test-4536-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let werk = base.join(format!("fix-{}", card));
    std::fs::create_dir_all(&werk).unwrap();
    git(&werk, &["init", "-q"]);
    std::fs::write(werk.join("README"), "fixture\n").unwrap();
    // #4537 — the repo's own guard scripts, as every real werk has them; a
    // missing one is UNMEASURED now (row 1 must stay CLEAN)
    for g in ["platform/tests/doc-coherence-ratchet.test.sh", "platform/scripts/clippy-ratchet.sh"] {
        let p = werk.join(g);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "#!/bin/sh\nexit 0\n").unwrap();
    }
    git(&werk, &["add", "-A"]);
    git(&werk, &["commit", "-qm", "base"]);
    for (path, body) in files {
        let p = werk.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, body).unwrap();
    }
    git(&werk, &["add", "-A"]);
    git(&werk, &["commit", "-qm", "card"]);
    run_in(&base, card, registry_rows, patch_status)
}

fn stub_path(base: &Path) -> String {
    let bin: PathBuf = base.join("stub-bin");
    std::fs::create_dir_all(&bin).unwrap();
    let demo = bin.join("werk-demo");
    std::fs::write(&demo, "#!/bin/sh\nexit 0\n").unwrap();
    Command::new("chmod").arg("+x").arg(&demo).status().unwrap();
    format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default())
}

fn run_in(base: &Path, card: &str, registry_rows: &[String], patch_status: u16) -> Run {
    run_in_path(base, card, registry_rows, patch_status, None, &[], None)
}

/// #4537 — a PATH holding only the system tools, git, and the named tools
/// (`bats`, `cargo`): a row leaves out the one it is about.
fn tools_path(base: &Path, with: &[&str]) -> String {
    let tools = base.join("tools");
    std::fs::create_dir_all(&tools).unwrap();
    let real = |name: &str| -> PathBuf {
        let out = Command::new("sh").arg("-c").arg(format!("command -v {}", name)).output().unwrap();
        PathBuf::from(String::from_utf8_lossy(&out.stdout).trim())
    };
    for name in ["git"].iter().chain(with.iter()) {
        let src = real(name);
        assert!(src.is_file(), "the fixture needs {} on this box", name);
        let _ = std::os::unix::fs::symlink(&src, tools.join(name));
    }
    let stub = stub_path(base);
    let stub_dir = stub.split(':').next().unwrap().to_string();
    format!("{}:{}:/usr/bin:/bin:/usr/sbin:/sbin", stub_dir, tools.display())
}

/// #4537 — one fixture with its own PATH.
fn run_fixture_path(tag: &str, card: &str, files: &[(&str, &str)], registry_rows: &[String], with: &[&str]) -> Run {
    run_fixture_full(tag, card, files, registry_rows, with, &[], None, |_| {})
}

/// #4537 — the same, with extra env, another werk-test binary, and a hook run
/// on the werk just before werk-test starts.
#[allow(clippy::too_many_arguments)]
fn run_fixture_full(tag: &str, card: &str, files: &[(&str, &str)], registry_rows: &[String], with: &[&str],
                    extra: &[(&str, String)], bin: Option<PathBuf>, before: impl Fn(&Path)) -> Run {
    let base = std::env::temp_dir().join(format!("werk-test-4537-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let path = tools_path(&base, with);
    let werk = base.join(format!("fix-{}", card));
    std::fs::create_dir_all(&werk).unwrap();
    git(&werk, &["init", "-q"]);
    std::fs::write(werk.join("README"), "fixture\n").unwrap();
    for g in ["platform/tests/doc-coherence-ratchet.test.sh", "platform/scripts/clippy-ratchet.sh"] {
        let p = werk.join(g);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "#!/bin/sh\nexit 0\n").unwrap();
    }
    // the repo's one nextest pin, as every werk carries it
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let pin = std::fs::read_to_string(Path::new(&dir).join("../../../.config/nextest.toml")).unwrap();
    std::fs::create_dir_all(werk.join(".config")).unwrap();
    std::fs::write(werk.join(".config/nextest.toml"), pin).unwrap();
    git(&werk, &["add", "-A"]);
    git(&werk, &["commit", "-qm", "base"]);
    for (path, body) in files {
        let p = werk.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, body).unwrap();
    }
    git(&werk, &["add", "-A"]);
    git(&werk, &["commit", "-qm", "card"]);
    before(&werk);
    run_in_path(&base, card, registry_rows, 200, Some(path), extra, bin)
}

fn run_in_path(base: &Path, card: &str, registry_rows: &[String], patch_status: u16, path: Option<String>,
               extra: &[(&str, String)], bin: Option<PathBuf>) -> Run {
    std::fs::create_dir_all(base).unwrap();
    let registry = format!("{{\"data\":[{}]}}", registry_rows.join(","));
    let (port, patches) = stub(registry, patch_status);
    let door = format!("http://127.0.0.1:{}", port);
    let mut cmd = Command::new(bin.unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_werk-test"))));
    for (k, v) in extra {
        cmd.env(k, v);
    }
    let out = cmd
        .args([card, "fix"])
        .env("PATH", path.unwrap_or_else(|| stub_path(base)))
        .env("CHORUS_WERK_BASE", base)
        .env_remove("WERK_TEST_TREE")
        .env_remove("CHORUS_HOME")
        .env("CHORUS_WRITE_TOKEN", "fixture")
        .env("OWL_API_TESTS", format!("{door}/tests?limit=10000"))
        .env("OWL_API_TESTSUITERUNS", format!("{door}/testsuiteruns"))
        .env("WERK_STACK_PROBES", format!("door={door}/"))
        .output()
        .expect("spawn werk-test");
    let _ = std::fs::remove_dir_all(base);
    let written = patches.lock().unwrap().clone();
    Run {
        code: out.status.code().unwrap_or(-1),
        out: format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)),
        patches: written,
    }
}

const PASS: &str = "#!/usr/bin/env bats\n@test \"p\" {\n  true\n}\n";
const FAIL: &str = "#!/usr/bin/env bats\n@test \"f\" {\n  false\n}\n";
const EMPTY: &str = "#!/usr/bin/env bats\n# no cases\n";

#[test]
fn row_01_one_passing_case_is_clean_exit_0() {
    let r = run_fixture("r1", "91001", &[("platform/tests/pass.bats", PASS)],
        &[row("platform/tests/pass.bats", "p", "test-pass")], 200);
    assert_eq!(r.code, 0, "{}", r.out);
    assert!(r.out.contains("werk-test: CLEAN (exit 0)"), "{}", r.out);
}

#[test]
fn row_02_a_failing_assert_is_red_exit_1() {
    let r = run_fixture("r2", "91002", &[("platform/tests/fail.bats", FAIL)],
        &[row("platform/tests/fail.bats", "f", "test-fail")], 200);
    assert_eq!(r.code, 1, "{}", r.out);
    assert!(r.out.contains("werk-test: RED (exit 1)"), "{}", r.out);
}

#[test]
fn row_06_a_suite_with_zero_cases_is_unmeasured_ran_nothing() {
    let r = run_fixture("r6", "91006", &[("platform/tests/empty.bats", EMPTY)],
        &[row("platform/tests/empty.bats", "none", "test-empty")], 200);
    assert_eq!(r.code, 2, "{}", r.out);
    assert!(r.out.contains("ran nothing"), "{}", r.out);
}

#[test]
fn row_10_no_werk_is_refused_exit_3() {
    let base = std::env::temp_dir().join(format!("werk-test-4536-r10-{}", std::process::id()));
    let r = run_in(&base, "91010", &[], 200);
    assert_eq!(r.code, 3, "{}", r.out);
    assert!(r.out.contains("REFUSED — werk not found"), "{}", r.out);
}

#[test]
fn a_card_run_writes_no_test_row() {
    // #4536 (Silas 07:55): a card run tests unmerged code, so its result is
    // not the Test's latest. Loki only; only the canonical nightly writes.
    let r = run_fixture("rc", "91012", &[("platform/tests/fail.bats", FAIL)],
        &[row("platform/tests/fail.bats", "f", "test-fail")], 200);
    assert_eq!(r.code, 1, "{}", r.out);
    assert!(r.patches.is_empty(), "a card run wrote a Test row: {:?}", r.patches);
}

/// The nightly against a temp root holding `files`, with the door stubbed.
fn run_nightly(root_name: &str, files: &[(&str, &str)], registry_rows: &[String], patch_status: u16) -> Run {
    // #4539 — rows run in parallel and several use the root "chorus": one base each
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let base = std::env::temp_dir().join(format!("werk-test-4536-n-{}-{}-{}", root_name.replace('/', "_"), std::process::id(), n));
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join(root_name);
    std::fs::create_dir_all(root.join("platform/scripts")).unwrap();
    for (path, body) in files {
        let p = root.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, body).unwrap();
    }
    let registry = format!("{{\"data\":[{}]}}", registry_rows.join(","));
    let (port, patches) = stub(registry, patch_status);
    let door = format!("http://127.0.0.1:{}", port);
    let out = Command::new(env!("CARGO_BIN_EXE_werk-test"))
        .arg("--nightly")
        .env("PATH", stub_path(&base))
        .env("CHORUS_ROOT", &root)
        .env_remove("CHORUS_HOME")
        .env("CHORUS_WRITE_TOKEN", "fixture")
        .env("OWL_API_TESTS", format!("{door}/tests?limit=10000"))
        .env("OWL_API_TESTSUITERUNS", format!("{door}/testsuiteruns"))
        .env("WERK_STACK_PROBES", format!("door={door}/"))
        .env("NIGHTLY_SUITE_WORKERS", "1")
        .env("NIGHTLY_LOAD_CAP", "100000")
        .output()
        .expect("spawn werk-test --nightly");
    let _ = std::fs::remove_dir_all(&base);
    let written = patches.lock().unwrap().clone();
    Run {
        code: out.status.code().unwrap_or(-1),
        out: format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)),
        patches: written,
    }
}

#[test]
fn row_09_a_refused_store_never_changes_the_nightly_verdict() {
    let r = run_nightly("chorus", &[("platform/tests/pass.bats", PASS)],
        &[row("platform/tests/pass.bats", "p", "test-pass")], 403);
    assert_eq!(r.patches.len(), 1, "the flipped case was written once: {:?}\n{}", r.patches, r.out);
    assert!(r.patches[0].starts_with("PATCH /tests/test-pass "), "{:?}", r.patches);
    assert!(r.out.contains("results not stored: 1 of 1"), "{}", r.out);
    assert!(!r.out.contains("werk-test: RED"), "storage is not the product: {}", r.out);
}

#[test]
fn the_nightly_writes_only_the_cases_that_flipped() {
    let prior = "{\"filePath\":\"platform/tests/pass.bats\",\"covers\":\"werk\",\"testType\":\"unit\",\
                 \"testName\":\"p\",\"name\":\"test-pass\",\"hermeticity\":\"hermetic\",\"result\":\"pass\"}".to_string();
    let r = run_nightly("chorus", &[("platform/tests/pass.bats", PASS)], &[prior], 200);
    assert!(r.patches.is_empty(), "nothing flipped, nothing written: {:?}\n{}", r.patches, r.out);
}

#[test]
fn a_werk_root_nightly_writes_nothing_even_pointed_at_a_door() {
    // NEGATIVE PROOF: OWL_API_TESTS points at a live door and the case flipped;
    // the werk root alone withholds it.
    let r = run_nightly("chorus-werk/kade-0000", &[("platform/tests/pass.bats", PASS)],
        &[row("platform/tests/pass.bats", "p", "test-pass")], 200);
    assert!(r.patches.is_empty(), "a werk nightly wrote a Test row: {:?}\n{}", r.patches, r.out);
    assert!(r.out.contains("only the canonical nightly writes Test rows"), "{}", r.out);
}

// ── #4537 — the rows this card adds (plan rows 3, 4, 5, 11, 12) ──────────────

const FX_TOML: &str = "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n";
const FX_LIB: &str = "pub fn one() -> u32 { 1 }\n";
const FX_TEST: &str = "#[test]\nfn one_is_one() { assert_eq!(fx::one(), 1); }\n";

fn crate_files(lib: &'static str) -> Vec<(&'static str, &'static str)> {
    vec![("platform/services/fx/Cargo.toml", FX_TOML), ("platform/services/fx/src/lib.rs", lib),
         ("platform/services/fx/tests/t.rs", FX_TEST)]
}

fn crate_row() -> String {
    row("platform/services/fx/tests/t.rs", "one_is_one", "test-fx")
}

#[test]
fn row_03_a_missing_tool_is_unmeasured_never_red() {
    // bats is not on PATH
    let r = run_fixture_path("r3", "91003", &[("platform/tests/pass.bats", PASS)],
        &[row("platform/tests/pass.bats", "p", "test-pass")], &[]);
    assert_eq!(r.code, 2, "{}", r.out);
    assert!(r.out.contains("missing bats"), "{}", r.out);
    assert!(!r.out.contains("werk-test: CLEAN") && !r.out.contains("werk-test: RED"), "{}", r.out);
}

#[test]
fn row_04_a_build_that_cannot_start_is_unmeasured() {
    // no cargo on PATH
    let r = run_fixture_path("r4", "91004", &crate_files(FX_LIB), &[crate_row()], &[]);
    assert_eq!(r.code, 2, "{}", r.out);
    assert!(r.out.contains("build did not start"), "{}", r.out);
    assert!(!r.out.contains("werk-test: CLEAN") && !r.out.contains("werk-test: RED"), "{}", r.out);
}

#[test]
fn row_05_a_compile_error_in_the_cards_code_is_red() {
    // cargo is there; the card's own code does not compile
    let r = run_fixture_path("r5", "91005", &crate_files("pub fn one( {\n"), &[crate_row()], &["cargo"]);
    assert_eq!(r.code, 1, "{}", r.out);
    assert!(r.out.contains("werk-test: RED (exit 1)"), "{}", r.out);
    assert!(r.out.contains("platform/services/fx"), "{}", r.out);
}

#[test]
fn row_11_a_fail_and_a_missing_tool_together_is_red_and_still_lists_the_unmeasured() {
    // bats runs and fails; the crate's build cannot start (no cargo)
    let mut files = crate_files(FX_LIB);
    files.push(("platform/tests/fail.bats", FAIL));
    let r = run_fixture_path("r11", "91011", &files,
        &[crate_row(), row("platform/tests/fail.bats", "f", "test-fail")], &["bats"]);
    assert_eq!(r.code, 1, "{}", r.out);
    assert!(r.out.contains("werk-test: RED (exit 1)"), "{}", r.out);
    assert!(r.out.contains("build did not start"), "the unmeasured list must still print: {}", r.out);
}

#[test]
fn row_12_a_case_that_tries_to_write_prod_is_unmeasured_and_named() {
    // a real membrane-aware writer (chorus-log through the installed shim),
    // asked to write the spine from inside a bats case with no test world
    let home = std::env::var("HOME").unwrap();
    let shim = format!("{}/.chorus/bin/chorus-hook-shim", home);
    assert!(Path::new(&shim).is_file(), "row 12 needs the installed chorus-hook-shim at {}", shim);
    let case = format!("#!/usr/bin/env bats\n@test \"writes the live spine\" {{\n  env -u CHORUS_LOG_FILE HOME=\"$BATS_TEST_TMPDIR/h\" {} chorus-log test.fixture kade x=1\n}}\n", shim);
    let case: &'static str = Box::leak(case.into_boxed_str());
    let r = run_fixture_path("r12", "91013", &[("platform/tests/prodwrite.bats", case)],
        &[row("platform/tests/prodwrite.bats", "writes the live spine", "test-prodwrite")], &["bats"]);
    assert_eq!(r.code, 2, "{}", r.out);
    assert!(r.out.contains("test tried to write prod: writes the live spine"), "{}", r.out);
}

#[test]
fn row_lock_a_build_lock_held_past_the_cap_is_unmeasured_and_names_the_holder() {
    // another process holds the crate's cargo build lock; the cap is 2s
    let holder = std::sync::Mutex::new(None::<std::process::Child>);
    let r = run_fixture_full("rl", "91014", &crate_files(FX_LIB), &[crate_row()], &["cargo"],
        &[("WERK_TEST_LOCK_CAP", "2".to_string())], None, |werk| {
            let d = werk.join("platform/services/fx/target/debug");
            std::fs::create_dir_all(&d).unwrap();
            let child = Command::new("sh").arg("-c").arg("exec 9>>.cargo-lock; sleep 60").current_dir(&d).spawn().unwrap();
            std::thread::sleep(std::time::Duration::from_millis(300));
            *holder.lock().unwrap() = Some(child);
        });
    if let Some(mut c) = holder.lock().unwrap().take() { let _ = c.kill(); let _ = c.wait(); }
    assert_eq!(r.code, 2, "{}", r.out);
    assert!(r.out.contains("build did not start: lock held by pid"), "{}", r.out);
}

#[test]
fn negative_proof_a_free_build_lock_builds_and_runs_clean() {
    let r = run_fixture_full("rlf", "91015", &crate_files(FX_LIB), &[crate_row()], &["cargo"],
        &[("WERK_TEST_LOCK_CAP", "2".to_string())], None, |_| {});
    assert_eq!(r.code, 0, "{}", r.out);
    assert!(!r.out.contains("lock held"), "{}", r.out);
}

/// Row 13: a werk-test slot copy older than the installed one, on a card that
/// does not build werk-test, hands the run to the installed one and says so.
fn slot_world(tag: &str, slot_older: bool) -> (PathBuf, PathBuf) {
    let w = std::env::temp_dir().join(format!("werk-test-4537-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&w);
    let slot = w.join("kade-bin");
    let home_bin = w.join("home/.chorus/bin");
    std::fs::create_dir_all(&slot).unwrap();
    std::fs::create_dir_all(&home_bin).unwrap();
    let copy = slot.join("werk-test");
    std::fs::copy(env!("CARGO_BIN_EXE_werk-test"), &copy).unwrap();
    let installed = home_bin.join("werk-test-bin");
    std::fs::write(&installed, "#!/bin/sh\necho INSTALLED-WERK-TEST-RAN\nexit 0\n").unwrap();
    Command::new("chmod").arg("+x").arg(&installed).status().unwrap();
    let (old, new) = if slot_older { (&copy, &installed) } else { (&installed, &copy) };
    Command::new("touch").args(["-t", "202601010000"]).arg(old).status().unwrap();
    Command::new("touch").arg(new).status().unwrap();
    (copy, w.join("home"))
}

#[test]
fn row_13_a_stale_slot_copy_hands_the_run_to_the_installed_werk_test() {
    let (copy, home) = slot_world("r13", true);
    let r = run_fixture_full("r13w", "91016", &[("platform/tests/pass.bats", PASS)],
        &[row("platform/tests/pass.bats", "p", "test-pass")], &["bats"],
        &[("HOME", home.display().to_string())], Some(copy), |_| {});
    assert!(r.out.contains("is older than the installed one"), "{}", r.out);
    assert!(r.out.contains("INSTALLED-WERK-TEST-RAN"), "{}", r.out);
    assert_eq!(r.code, 0, "{}", r.out);
}

#[test]
fn negative_proof_a_slot_copy_newer_than_the_installed_one_runs_itself() {
    let (copy, home) = slot_world("r13n", false);
    let r = run_fixture_full("r13nw", "91017", &[("platform/tests/pass.bats", PASS)],
        &[row("platform/tests/pass.bats", "p", "test-pass")], &["bats"],
        &[("HOME", home.display().to_string())], Some(copy), |_| {});
    assert!(!r.out.contains("INSTALLED-WERK-TEST-RAN"), "{}", r.out);
    assert!(r.out.contains("werk-test: CLEAN (exit 0)"), "{}", r.out);
}

// ── #4539 — rows 7 and 8 ──

/// A registry row whose case is held until `until`.
fn held_row(file: &str, test: &str, entity: &str, until: &str) -> String {
    format!("{{\"filePath\":\"{file}\",\"covers\":\"werk\",\"testType\":\"unit\",\
             \"testName\":\"{test}\",\"name\":\"{entity}\",\"hermeticity\":\"hermetic\",\
             \"quarantined\":\"true\",\"quarantineReason\":\"flaky\",\"quarantineUntil\":\"{until}\"}}")
}

const HELD: &str = "#!/usr/bin/env bats\n@test \"p\" {\n  true\n}\n@test \"f\" {\n  false\n}\n";

#[test]
fn row_07_a_quarantined_case_does_not_run_and_is_unmeasured_until_its_date() {
    let r = run_fixture("r7", "91007", &[("platform/tests/held.bats", HELD)],
        &[row("platform/tests/held.bats", "p", "test-held-p"),
          held_row("platform/tests/held.bats", "f", "test-held-f", "2099-12-31")], 200);
    assert_eq!(r.code, 2, "{}", r.out);
    assert!(r.out.contains("quarantined until 2099-12-31: f (flaky)"), "{}", r.out);
    assert!(!r.out.contains("werk-test: RED"), "the held failing case never ran: {}", r.out);
}

/// NEGATIVE PROOF: the same hold, expired, runs the case — and it fails.
#[test]
fn negative_proof_an_expired_quarantine_runs_the_test() {
    let r = run_fixture("r7x", "91017", &[("platform/tests/held.bats", HELD)],
        &[row("platform/tests/held.bats", "p", "test-held-p"),
          held_row("platform/tests/held.bats", "f", "test-held-f", "2000-01-01")], 200);
    assert_eq!(r.code, 1, "{}", r.out);
    assert!(r.out.contains("quarantine expired, runs (1)"), "{}", r.out);
}

const BOX: &str = "#!/usr/bin/env bats\n# @needs: machine\n@test \"b\" {\n  false\n}\n";

#[test]
fn row_08_a_needs_machine_suite_is_not_selected_and_is_listed_nightly_only() {
    let r = run_fixture("r8", "91008", &[("platform/tests/pass.bats", PASS), ("platform/tests/box.bats", BOX)],
        &[row("platform/tests/pass.bats", "p", "test-pass"), row("platform/tests/box.bats", "b", "test-box")], 200);
    assert_eq!(r.code, 0, "{}", r.out);
    assert!(r.out.contains("1 tests run nightly only: platform/tests/box.bats (needs: machine)"), "{}", r.out);
    assert!(r.out.contains("werk-test: CLEAN (exit 0)"), "{}", r.out);
}

/// NEGATIVE PROOF: remove the header and the card run selects the suite —
/// its failing case runs and the card is red.
#[test]
fn negative_proof_without_the_needs_header_the_card_run_selects_the_suite() {
    let bare = BOX.replace("# @needs: machine\n", "");
    let r = run_fixture("r8x", "91018", &[("platform/tests/pass.bats", PASS), ("platform/tests/box.bats", &bare)],
        &[row("platform/tests/pass.bats", "p", "test-pass"), row("platform/tests/box.bats", "b", "test-box")], 200);
    assert_eq!(r.code, 1, "{}", r.out);
    assert!(r.out.contains("0 tests run nightly only"), "{}", r.out);
}

// ── #4539 — the nightly takes the same rules (stub door, no CHORUS_HOME) ──

#[test]
fn the_nightly_does_not_run_a_held_case_and_names_it_unmeasured() {
    let r = run_nightly("chorus", &[("platform/tests/held.bats", HELD)],
        &[row("platform/tests/held.bats", "p", "test-held-p"),
          held_row("platform/tests/held.bats", "f", "test-held-f", "2099-12-31")], 200);
    assert!(r.out.contains("quarantined until 2099-12-31: f (flaky)"), "{}", r.out);
    assert!(!r.out.contains("werk-test: RED"), "the held failing case never ran: {}", r.out);
}

/// NEGATIVE PROOF: expired, the nightly runs the case and it is red.
#[test]
fn negative_proof_the_nightly_runs_an_expired_hold() {
    let r = run_nightly("chorus", &[("platform/tests/held.bats", HELD)],
        &[row("platform/tests/held.bats", "p", "test-held-p"),
          held_row("platform/tests/held.bats", "f", "test-held-f", "2000-01-01")], 200);
    assert!(r.out.contains("quarantine expired, runs (1)"), "{}", r.out);
    assert!(r.out.contains("werk-test: RED"), "{}", r.out);
}

/// The nightly selects what card runs leave to it: a needs: machine suite runs.
#[test]
fn the_nightly_runs_a_needs_machine_suite() {
    let r = run_nightly("chorus", &[("platform/tests/box.bats", BOX)],
        &[row("platform/tests/box.bats", "b", "test-box")], 200);
    assert!(r.out.contains("werk-test: RED"), "the nightly ran the box suite's failing case: {}", r.out);
}

// ── #4539 — a card that edits only a nightly-only cargo test binary (Silas 04:30) ──

const FX_BOX: &str = "// @needs: machine\n#[test]\nfn reads_the_box() { assert_eq!(fx::one(), 2); }\n";

/// The crate is on the base; the card's last commit edits only tests/box.rs,
/// so the cargo lane takes its only-bins path.
fn box_only_edit(tag: &str, card: &str, body: &'static str) -> Run {
    let mut files = crate_files(FX_LIB);
    files.push(("platform/services/fx/tests/box.rs", body));
    run_fixture_full(tag, card, &files,
        &[crate_row(), row("platform/services/fx/tests/box.rs", "reads_the_box", "test-fx-box")],
        &["cargo"], &[], None, |w| {
            let p = w.join("platform/services/fx/tests/box.rs");
            std::fs::write(&p, format!("{}// edited\n", std::fs::read_to_string(&p).unwrap())).unwrap();
            git(w, &["commit", "-qam", "edit box"]);
        })
}

#[test]
fn editing_only_a_needs_machine_test_binary_does_not_run_it() {
    let r = box_only_edit("r8c", "91028", FX_BOX);
    assert!(r.out.contains("all nightly only or stack-down: not run"), "{}", r.out);
    assert!(!r.out.contains("werk-test: RED"), "the box binary ran: {}", r.out);
    // the lane is not "ran nothing"; the card as a whole proved nothing, and
    // card-coverage says so — a diff only the nightly can prove is not clean
    assert!(!r.out.contains("cargo-test:fx — ran nothing"), "never selected is not a lane that ran nothing: {}", r.out);
    assert!(r.out.contains("card-coverage: zero-coverage"), "{}", r.out);
}

/// NEGATIVE PROOF: the same edit without the header runs the binary — red.
#[test]
fn negative_proof_without_the_header_the_edited_binary_runs() {
    let r = box_only_edit("r8d", "91038", "#[test]\nfn reads_the_box() { assert_eq!(fx::one(), 2); }\n");
    assert!(r.out.contains("werk-test: RED"), "{}", r.out);
}

// ── #4540 — the five steps, each its own process, through one run folder ──

/// A werk like `run_fixture`'s, then `steps` run one by one (each a separate
/// werk-test process) on one trace and one runs folder. Returns every step's run.
fn run_steps(tag: &str, card: &str, files: &[(&str, &str)], registry_rows: &[String], steps: &[&[&str]]) -> Vec<Run> {
    let base = std::env::temp_dir().join(format!("werk-test-4540-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let werk = base.join(format!("fix-{}", card));
    std::fs::create_dir_all(&werk).unwrap();
    git(&werk, &["init", "-q"]);
    std::fs::write(werk.join("README"), "fixture\n").unwrap();
    for g in ["platform/tests/doc-coherence-ratchet.test.sh", "platform/scripts/clippy-ratchet.sh"] {
        let p = werk.join(g);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "#!/bin/sh\nexit 0\n").unwrap();
    }
    git(&werk, &["add", "-A"]);
    git(&werk, &["commit", "-qm", "base"]);
    for (path, body) in files {
        let p = werk.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, body).unwrap();
    }
    git(&werk, &["add", "-A"]);
    git(&werk, &["commit", "-qm", "card"]);
    let registry = format!("{{\"data\":[{}]}}", registry_rows.join(","));
    let (port, patches) = stub(registry, 200);
    let door = format!("http://127.0.0.1:{}", port);
    let path = stub_path(&base);
    let mut runs = Vec::new();
    for step in steps {
        let mut argv: Vec<&str> = step.to_vec();
        argv.splice(1..1, [card, "fix"]);
        let out = Command::new(env!("CARGO_BIN_EXE_werk-test"))
            .args(&argv)
            .env("PATH", &path)
            .env("CHORUS_WERK_BASE", &base)
            .env("WERK_TEST_RUNS", base.join("runs"))
            .env("CHORUS_TRACE_ID", format!("fixture-4540-{}", tag))
            .env_remove("WERK_TEST_TREE")
            .env_remove("CHORUS_HOME")
            .env("CHORUS_WRITE_TOKEN", "fixture")
            .env("OWL_API_TESTS", format!("{door}/tests?limit=10000"))
            .env("OWL_API_TESTSUITERUNS", format!("{door}/testsuiteruns"))
            .env("WERK_STACK_PROBES", format!("door={door}/"))
            .output()
            .expect("spawn werk-test step");
        runs.push(Run {
            code: out.status.code().unwrap_or(-1),
            out: format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)),
            patches: patches.lock().unwrap().clone(),
        });
    }
    let _ = std::fs::remove_dir_all(&base);
    runs
}

/// #4540 — Jeff 10-16 B: the queue, then perf alone, then report and verdict.
const ALL_STEPS: &[&[&str]] = &[
    &["scope"], &["prepare"], &["run"], &["run", "--test-type=perf"], &["report"], &["verdict"],
];

#[test]
fn the_steps_run_apart_reach_the_same_clean_verdict() {
    let runs = run_steps("s1", "92001", &[("platform/tests/pass.bats", PASS)],
        &[row("platform/tests/pass.bats", "p", "test-pass")], ALL_STEPS);
    for (r, s) in runs.iter().zip(ALL_STEPS) {
        if s[0] != "verdict" {
            assert_eq!(r.code, 0, "{:?}: {}", s, r.out);
        }
    }
    let v = runs.last().unwrap();
    assert_eq!(v.code, 0, "{}", v.out);
    assert!(v.out.contains("werk-test: CLEAN (exit 0)"), "{}", v.out);
}

#[test]
fn a_red_case_goes_green_in_its_type_step_and_red_at_the_verdict() {
    let runs = run_steps("s2", "92002", &[("platform/tests/fail.bats", FAIL)],
        &[row("platform/tests/fail.bats", "f", "test-fail")], ALL_STEPS);
    let queue = &runs[2];
    assert_eq!(queue.code, 0, "the run step writes its result and lets the next run: {}", queue.out);
    let v = runs.last().unwrap();
    assert_eq!(v.code, 1, "{}", v.out);
    assert!(v.out.contains("RED in: bats:platform/tests/fail.bats"), "{}", v.out);
    assert!(v.out.contains("verdict: RED — 1 type failed: unit"), "the line /cws reads: {}", v.out);
}

/// NEGATIVE PROOF: a planned type whose step never ran is not clean — the
/// verdict names it UNMEASURED, never reads the missing file as nothing.
#[test]
fn negative_proof_a_type_step_that_never_ran_is_unmeasured_at_the_verdict() {
    // the queue step never ran: every type it carries, unit among them, is missing
    let skip_unit: Vec<&[&str]> = ALL_STEPS.iter().copied().filter(|s| *s != ["run"]).collect();
    let runs = run_steps("s3", "92003", &[("platform/tests/pass.bats", PASS)],
        &[row("platform/tests/pass.bats", "p", "test-pass")], &skip_unit);
    let v = runs.last().unwrap();
    assert_eq!(v.code, 2, "{}", v.out);
    assert!(v.out.contains("unit — did not report"), "{}", v.out);
}

/// NEGATIVE PROOF: a step whose input was never written refuses, naming the
/// step that writes it (dagu stops there, by name).
#[test]
fn negative_proof_a_step_before_its_inputs_refuses_and_names_them() {
    let runs = run_steps("s4", "92004", &[("platform/tests/pass.bats", PASS)],
        &[row("platform/tests/pass.bats", "p", "test-pass")], &[&["prepare"]]);
    assert_eq!(runs[0].code, 3, "{}", runs[0].out);
    assert!(runs[0].out.contains("run `werk-test scope` first"), "{}", runs[0].out);
}

/// A retried type step overwrites its own result: the case is counted once.
#[test]
fn a_retried_type_step_does_not_count_twice() {
    let mut steps: Vec<&[&str]> = ALL_STEPS.to_vec();
    steps.insert(3, &["run"]);
    let runs = run_steps("s5", "92005", &[("platform/tests/pass.bats", PASS)],
        &[row("platform/tests/pass.bats", "p", "test-pass")], &steps);
    let v = runs.last().unwrap();
    assert_eq!(v.code, 0, "{}", v.out);
    let report = &runs[runs.len() - 2];
    assert!(report.out.contains("card-coverage: covered (1 registered case(s)"), "{}", report.out);
}

// ── #4540 — the nightly as steps (scope = all), each its own process ──

fn run_nightly_steps(tag: &str, files: &[(&str, &str)], registry_rows: &[String], steps: &[&[&str]]) -> Vec<Run> {
    let base = std::env::temp_dir().join(format!("werk-test-4540-n-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join("chorus");
    std::fs::create_dir_all(root.join("platform/scripts")).unwrap();
    for (path, body) in files {
        let p = root.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, body).unwrap();
    }
    let registry = format!("{{\"data\":[{}]}}", registry_rows.join(","));
    let (port, patches) = stub(registry, 200);
    let door = format!("http://127.0.0.1:{}", port);
    let mut runs = Vec::new();
    for step in steps {
        let out = Command::new(env!("CARGO_BIN_EXE_werk-test"))
            .args(*step)
            .env("PATH", stub_path(&base))
            .env("CHORUS_ROOT", &root)
            .env_remove("CHORUS_HOME")
            .env("CHORUS_TRACE_ID", format!("fixture-4540-n-{}", tag))
            .env("WERK_TEST_RUNS", base.join("runs"))
            .env("WERK_TEST_LEGS_NOOP", "1")
            .env("NIGHTLY_LEGS_NOOP", "1")
            .env("NIGHTLY_LOG_PATH", base.join("nightly.log"))
            .env("NIGHTLY_FAIL_DIR", base.join("fail"))
            .env("NIGHTLY_NO_NUDGE", "1")
            .env("NIGHTLY_LOAD_STUB", "0.1")
            .env("CHORUS_WRITE_TOKEN", "fixture")
            .env("OWLAPI", &door)
            .env("OWL_API_TESTS", format!("{door}/tests?limit=10000"))
            .env("OWL_API_TESTSUITERUNS", format!("{door}/testsuiteruns"))
            .env("WERK_STACK_PROBES", format!("door={door}/"))
            .env("NIGHTLY_SUITE_WORKERS", "1")
            .env("NIGHTLY_LOAD_CAP", "100000")
            .output()
            .expect("spawn werk-test nightly step");
        runs.push(Run {
            code: out.status.code().unwrap_or(-1),
            out: format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)),
            patches: patches.lock().unwrap().clone(),
        });
    }
    // the run log, read from the FILE (Silas 06:08: under dagu stdout is the step log)
    let log = std::fs::read_to_string(base.join("nightly.log")).unwrap_or_default();
    runs.push(Run { code: 0, out: log, patches: Vec::new() });
    let _ = std::fs::remove_dir_all(&base);
    runs
}

const NIGHTLY_STEPS: &[&[&str]] = &[
    &["scope", "--scope=all"], &["prepare", "--scope=all"], &["run", "--scope=all"],
    &["run", "--scope=all", "--test-type=perf"], &["report", "--scope=all"], &["verdict", "--scope=all"],
];

#[test]
fn the_nightly_steps_run_the_registry_and_write_one_log() {
    let runs = run_nightly_steps("n1", &[("platform/tests/pass.bats", PASS)],
        &[row("platform/tests/pass.bats", "p", "test-pass")], NIGHTLY_STEPS);
    let all = runs.iter().map(|r| r.out.as_str()).collect::<Vec<_>>().join("\n----\n");
    let verdict = &runs[5];
    assert_eq!(verdict.code, 0, "{all}");
    assert!(verdict.out.contains("verdict: CLEAN"), "{all}");
    let log = &runs[6].out;
    assert!(log.contains("RUN|start|") && log.contains("RUN|complete|"), "{log}");
    assert!(log.contains("SUITE|") && log.contains("platform/tests/pass.bats"), "{log}");
    assert!(log.contains("nightly-plan|"), "the lane's pass-through lines reach the log file: {log}");
}

#[test]
fn a_red_suite_makes_the_nightly_verdict_red() {
    let runs = run_nightly_steps("n2", &[("platform/tests/fail.bats", FAIL)],
        &[row("platform/tests/fail.bats", "f", "test-fail")], NIGHTLY_STEPS);
    let all = runs.iter().map(|r| r.out.as_str()).collect::<Vec<_>>().join("\n----\n");
    assert_eq!(runs[2].code, 0, "the run step writes its rows and lets report run: {all}");
    assert_eq!(runs[5].code, 1, "{all}");
    assert!(runs[5].out.contains("1 red"), "{all}");
}

/// NEGATIVE PROOF: a stopped run is recorded stopped, and never completes.
#[test]
fn negative_proof_a_stopped_nightly_says_stopped_never_complete() {
    let runs = run_nightly_steps("n3", &[("platform/tests/pass.bats", PASS)],
        &[row("platform/tests/pass.bats", "p", "test-pass")],
        &[&["scope", "--scope=all"], &["report", "--scope=all", "--stopped"]]);
    let log = &runs[2].out;
    assert!(log.contains("RUN|stopped|"), "{log}");
    assert!(!log.contains("RUN|complete|"), "{log}");
}
