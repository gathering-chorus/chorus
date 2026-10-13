// #4511 — a werk whose package-lock.json differs from canonical's gets its own
// npm ci instead of borrowing canonical's node_modules, and never writes
// through the symlink into canonical's live tree.
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use werk_build::ensure_node_modules;

fn fixture(tag: &str) -> PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let p = std::env::temp_dir().join(format!("wb-4511-{}-{}-{}", tag, std::process::id(), nanos));
    fs::create_dir_all(&p).unwrap();
    p
}

fn write(root: &Path, rel: &str, body: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, body).unwrap();
}

fn git_track(dir: &Path, files: &[&str]) {
    let g = |args: &[&str]| assert!(std::process::Command::new("git").arg("-C").arg(dir).args(args).output().unwrap().status.success());
    g(&["init", "-q"]);
    let mut add = vec!["add", "--"];
    add.extend_from_slice(files);
    g(&add);
}

/// A stand-in npm. `ls` passes only where node_modules/satisfied exists;
/// `ci` makes node_modules with a marker and logs the call.
fn fake_npm(root: &Path, ci_ok: bool) -> PathBuf {
    let p = root.join("fake-npm");
    let ci = if ci_ok {
        format!("echo \"$PWD ci\" >> {log}; mkdir -p node_modules && touch node_modules/installed-from-own-lock", log = root.join("npm.log").display())
    } else {
        "echo 'ERESOLVE could not resolve' >&2; exit 1".to_string()
    };
    let body = format!("#!/bin/sh\ncase \"$1\" in\n  ls) [ -f node_modules/satisfied ] || exit 1 ;;\n  ci) {ci} ;;\nesac\n");
    fs::write(&p, body).unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    p
}

// one test: CHORUS_NPM is process-wide, so the cases run in sequence
#[test]
fn a_werk_with_its_own_lock_installs_from_it_and_leaves_canonical_alone() {
    let root = fixture("own-lock");
    let home = root.join("chorus");
    let werk = root.join("chorus-werk/silas-4511");
    write(&home, "platform/mcp-server/package.json", r#"{"name":"chorus-mcp"}"#);
    write(&home, "platform/mcp-server/package-lock.json", r#"{"sdk":"1.29.0"}"#);
    fs::create_dir_all(home.join("platform/mcp-server/node_modules")).unwrap();
    write(&werk, "platform/mcp-server/package.json", r#"{"name":"chorus-mcp"}"#);
    write(&werk, "platform/mcp-server/package-lock.json", r#"{"sdk":"1.31.0"}"#);
    git_track(&werk, &["platform/mcp-server/package.json"]);
    // the werk starts on the old symlink, the way every werk does today
    let werk_nm = werk.join("platform/mcp-server/node_modules");
    std::os::unix::fs::symlink(home.join("platform/mcp-server/node_modules"), &werk_nm).unwrap();

    std::env::set_var("CHORUS_NPM", fake_npm(&root, true));
    assert_eq!(ensure_node_modules(werk.to_str().unwrap(), &home).unwrap(), 1);
    assert!(!werk_nm.is_symlink(), "the werk has its own node_modules now");
    assert!(werk_nm.join("installed-from-own-lock").exists(), "npm ci ran in the werk");
    assert!(!home.join("platform/mcp-server/node_modules/installed-from-own-lock").exists(),
        "NEGATIVE PROOF: nothing was written through the old symlink into canonical");
    let calls = fs::read_to_string(root.join("npm.log")).unwrap();
    assert_eq!(calls.lines().count(), 1);
    assert!(calls.contains("silas-4511/platform/mcp-server ci"), "{calls}");

    // a second build is a no-op: the stamp says it was installed from this lock
    assert_eq!(ensure_node_modules(werk.to_str().unwrap(), &home).unwrap(), 0);
    assert_eq!(fs::read_to_string(root.join("npm.log")).unwrap().lines().count(), 1, "no second npm ci");

    // the 2026-10-13 state: the locks match, but canonical never installed it
    write(&werk, "platform/mcp-server/package-lock.json", r#"{"sdk":"1.29.0"}"#);
    fs::remove_dir_all(&werk_nm).unwrap();
    assert_eq!(ensure_node_modules(werk.to_str().unwrap(), &home).unwrap(), 1);
    assert!(!werk_nm.is_symlink(), "same lock, but canonical's tree does not satisfy it: own install");

    // NEGATIVE PROOF: same lock and canonical satisfies it, so it goes back to the symlink
    fs::write(home.join("platform/mcp-server/node_modules/satisfied"), "").unwrap();
    fs::remove_dir_all(&werk_nm).unwrap();
    assert_eq!(ensure_node_modules(werk.to_str().unwrap(), &home).unwrap(), 1);
    assert!(werk_nm.is_symlink(), "same lock and canonical is installed: borrow canonical's tree");

    // a failed install is a refusal that names the package, never a silent symlink
    write(&werk, "platform/mcp-server/package-lock.json", r#"{"sdk":"9.9.9"}"#);
    std::env::set_var("CHORUS_NPM", fake_npm(&root, false));
    let e = ensure_node_modules(werk.to_str().unwrap(), &home).unwrap_err();
    assert!(e.contains("platform/mcp-server") && e.contains("npm ci failed"), "{e}");
    std::env::remove_var("CHORUS_NPM");
}
