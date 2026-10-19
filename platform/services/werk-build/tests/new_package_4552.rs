// @domain: builds — placed by chorus-crawl's domain rules (#4645)
// #4552 — Clearing v2 landed (10-15 10:09) as a package canonical had never
// installed. werk-build skipped any package without a canonical node_modules,
// so every later card built it with no deps and died (#4538 runs 5, 6). It now
// installs such a package from its own lock. And a failed `npm run build`
// shows tsc's stdout, which used to be dropped ("npm run build:" and nothing).
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use werk_build::{ensure_node_modules, failure_text};

fn fixture(tag: &str) -> PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let p = std::env::temp_dir().join(format!("wb-4552-{}-{}-{}", tag, std::process::id(), nanos));
    fs::create_dir_all(&p).unwrap();
    p
}

fn write(root: &Path, rel: &str, body: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, body).unwrap();
}

#[test]
fn a_package_canonical_never_installed_is_installed_in_the_werk() {
    let root = fixture("new-pkg");
    let home = root.join("chorus");
    let werk = root.join("chorus-werk/silas-4538");
    // canonical has the package but never ran npm ci for it: no node_modules
    write(&home, "directing/clearing-v2/package.json", r#"{"name":"clearing-v2"}"#);
    write(&home, "directing/clearing-v2/package-lock.json", r#"{"express":"5"}"#);
    write(&werk, "directing/clearing-v2/package.json", r#"{"name":"clearing-v2"}"#);
    write(&werk, "directing/clearing-v2/package-lock.json", r#"{"express":"5"}"#);
    let g = |args: &[&str]| assert!(std::process::Command::new("git").arg("-C").arg(&werk).args(args).output().unwrap().status.success());
    g(&["init", "-q"]);
    g(&["add", "--", "directing/clearing-v2/package.json"]);
    let npm = root.join("fake-npm");
    fs::write(&npm, format!("#!/bin/sh\n[ \"$1\" = ci ] && {{ echo \"$PWD ci\" >> {}; mkdir -p node_modules && touch node_modules/express; }}\n", root.join("npm.log").display())).unwrap();
    fs::set_permissions(&npm, fs::Permissions::from_mode(0o755)).unwrap();
    std::env::set_var("CHORUS_NPM", &npm);

    // NEGATIVE PROOF (vs the old code, which skipped it and returned 0): it is installed
    assert_eq!(ensure_node_modules(werk.to_str().unwrap(), &home).unwrap(), 1);
    assert!(werk.join("directing/clearing-v2/node_modules/express").exists(), "npm ci ran in the werk");
    assert!(!home.join("directing/clearing-v2/node_modules").exists(), "canonical is left alone");
    // a second build is a no-op
    assert_eq!(ensure_node_modules(werk.to_str().unwrap(), &home).unwrap(), 0);
    assert_eq!(fs::read_to_string(root.join("npm.log")).unwrap().lines().count(), 1);
    std::env::remove_var("CHORUS_NPM");
}

#[test]
fn a_failed_build_shows_what_tsc_printed_on_stdout() {
    let tsc = b"tsconfig.json(12,5): error TS5107: Option 'moduleResolution=node10' is deprecated\n";
    let why = failure_text(b"", tsc);
    assert!(why.contains("TS5107"), "{why}");
    // NEGATIVE PROOF: the old message was stderr only, which is empty here
    assert!(!String::from_utf8_lossy(b"").contains("TS5107"));
    // stderr still comes first when there is some
    assert!(failure_text(b"npm ERR! build", tsc).starts_with("npm ERR! build"));
}
