// @domain: builds — placed by chorus-crawl's domain rules (#4538)
// #4511 — the canonical deploy installs what the lock pins before it builds.
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use werk_deploy::ensure_installed;

fn fixture(tag: &str) -> PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let p = std::env::temp_dir().join(format!("wd-4511-{}-{}-{}", tag, std::process::id(), nanos));
    fs::create_dir_all(&p).unwrap();
    p
}

/// A stand-in npm. `ls` passes only when node_modules/ok exists (what is
/// installed satisfies package.json); `ci` makes it, unless told to install
/// the wrong thing. Every call is logged.
fn fake_npm(dir: &PathBuf, ci_fixes_it: bool) -> String {
    let p = dir.join("fake-npm");
    let log = dir.join("calls.log");
    let fix = if ci_fixes_it { "mkdir -p node_modules && touch node_modules/ok" } else { "mkdir -p node_modules" };
    // `ci` also records whether the live tree was still whole while it ran
    fs::write(&p, format!(
        "#!/bin/sh\necho \"$1\" >> {log}\ncase \"$1\" in\n  ls) [ -f node_modules/ok ] || {{ echo 'missing: @modelcontextprotocol/sdk@1.31.0' >&2; exit 1; }} ;;\n  ci) [ -f ../node_modules/old-1.29.0 ] && echo live-intact >> {log}; {fix} ;;\nesac\n",
        log = log.display()
    )).unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    p.to_string_lossy().into_owned()
}

fn calls(dir: &PathBuf) -> Vec<String> {
    fs::read_to_string(dir.join("calls.log")).unwrap_or_default().lines().map(String::from).collect()
}

#[test]
fn installed_matching_the_lock_is_left_alone() {
    let d = fixture("match");
    fs::create_dir_all(d.join("node_modules")).unwrap();
    fs::write(d.join("node_modules/ok"), "").unwrap();
    let npm = fake_npm(&d, true);
    assert_eq!(ensure_installed(d.to_str().unwrap(), &npm), Ok(false));
    assert_eq!(calls(&d), vec!["ls"], "no npm ci when nothing drifted");
}

#[test]
fn drift_from_the_lock_reinstalls_then_checks_again() {
    // the 1.29.0-installed / 1.31.0-pinned state measured on canonical 2026-10-13
    let d = fixture("drift");
    fs::create_dir_all(d.join("node_modules")).unwrap();
    fs::write(d.join("node_modules/old-1.29.0"), "").unwrap();
    fs::write(d.join("package-lock.json"), "{}").unwrap();
    let npm = fake_npm(&d, true);
    assert_eq!(ensure_installed(d.to_str().unwrap(), &npm), Ok(true));
    assert_eq!(calls(&d), vec!["ls", "ci", "live-intact", "ls"], "the live tree stayed whole while npm ci ran");
    assert!(d.join("node_modules/ok").exists(), "the new tree is swapped in");
    assert!(!d.join("node_modules/old-1.29.0").exists());
    assert!(!d.join(".npm-stage").exists() && !d.join(".node_modules.prev").exists(), "no leftovers");
}

#[test]
fn negative_proof_an_install_that_still_does_not_match_stops_the_deploy() {
    let d = fixture("still-wrong");
    fs::create_dir_all(d.join("node_modules")).unwrap();
    fs::write(d.join("node_modules/old-1.29.0"), "").unwrap();
    let npm = fake_npm(&d, false);
    let e = ensure_installed(d.to_str().unwrap(), &npm).unwrap_err();
    assert!(d.join("node_modules/old-1.29.0").exists(), "the old tree is put back, so the running server keeps what it had");
    assert!(e.contains("still does not match"), "{e}");
    assert!(e.contains("@modelcontextprotocol/sdk@1.31.0"), "the refusal names what is missing: {e}");
}
