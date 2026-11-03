// @test-type: unit — renders and runs the wrapper with /bin/sh in a temp dir, no live services.
// @domain: builds
//! #4444 reopen 3 — a hyphenated role finds its werk bin slot. #4530 run 6 (14 Oct
//! 19:24): werk-deploy read WERK_ABBY-NORMAL_BIN, found nothing, and rolled back a
//! good install as "installed=None".
use werk_deploy::{canonical_wrapper, werk_slot_var};
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

#[test]
fn a_hyphenated_role_maps_to_a_legal_env_name() {
    assert_eq!(werk_slot_var("abby-normal"), "WERK_ABBY_NORMAL_BIN");
    assert_eq!(werk_slot_var("silas"), "WERK_SILAS_BIN");
}

fn fixture(tag: &str) -> (std::path::PathBuf, String) {
    let dir = std::env::temp_dir().join(format!("slot-4444-{}-{}", tag, std::process::id()));
    std::fs::create_dir_all(dir.join("slot")).unwrap();
    let mk = |p: &std::path::Path, out: &str| {
        std::fs::write(p, format!("#!/bin/sh\necho {}\n", out)).unwrap();
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
    };
    mk(&dir.join("slot/verb"), "slot");
    // #4853 — a trusted slot dir is writable only by its owner (role umask is 002)
    std::fs::set_permissions(dir.join("slot"), std::fs::Permissions::from_mode(0o755)).unwrap();
    mk(&dir.join("verb-bin"), "canonical");
    let w = dir.join("verb");
    std::fs::write(&w, canonical_wrapper("verb", dir.join("verb-bin").to_str().unwrap())).unwrap();
    std::fs::set_permissions(&w, std::fs::Permissions::from_mode(0o755)).unwrap();
    (dir, w.to_string_lossy().to_string())
}

fn run(w: &str, role: &str, envs: &[(&str, String)]) -> String {
    run_full(w, role, envs).1
}

fn run_full(w: &str, role: &str, envs: &[(&str, String)]) -> (i32, String, String) {
    let mut c = Command::new("/bin/sh");
    c.arg(w).env_clear().env("PATH", "/usr/bin:/bin").env("CHORUS_ROLE", role);
    for (k, v) in envs { c.env(k, v); }
    let o = c.output().unwrap();
    (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).trim().to_string(), String::from_utf8_lossy(&o.stderr).trim().to_string())
}

/// #4853 — the wrapper is the second door to a slot (a verb calling a verb never
/// passes the runner's binary_path). NEGATIVE PROOF: a group-writable slot dir, a
/// group-writable slot file and a symlinked slot are refused loudly; nothing runs.
#[test]
fn negative_proof_an_untrusted_slot_is_refused_and_nothing_runs() {
    let (dir, w) = fixture("4853dir");
    let slot = dir.join("slot");
    std::fs::set_permissions(&slot, std::fs::Permissions::from_mode(0o775)).unwrap();
    let (code, out, err) = run_full(&w, "wren", &[("WERK_WREN_BIN", slot.to_string_lossy().to_string())]);
    assert_eq!((code, out.as_str()), (126, ""), "{err}");
    assert!(err.contains("writable by group or other"), "{err}");

    let (dir, w) = fixture("4853file");
    std::fs::set_permissions(dir.join("slot/verb"), std::fs::Permissions::from_mode(0o775)).unwrap();
    let (code, _, err) = run_full(&w, "wren", &[("WERK_WREN_BIN", dir.join("slot").to_string_lossy().to_string())]);
    assert_eq!(code, 126, "{err}");

    let (dir, w) = fixture("4853link");
    std::fs::remove_file(dir.join("slot/verb")).unwrap();
    std::os::unix::fs::symlink(dir.join("verb-bin"), dir.join("slot/verb")).unwrap();
    let (code, out, err) = run_full(&w, "wren", &[("WERK_WREN_BIN", dir.join("slot").to_string_lossy().to_string())]);
    assert_eq!((code, out.as_str()), (126, ""), "{err}");
    assert!(err.contains("symlink"), "{err}");
}

#[test]
fn the_wrapper_runs_abby_normals_slot() {
    let (dir, w) = fixture("abby");
    let slot = dir.join("slot").to_string_lossy().to_string();
    assert_eq!(run(&w, "abby-normal", &[("WERK_ABBY_NORMAL_BIN", slot)]), "slot");
}

/// NEGATIVE PROOF (#3734): with no slot exported, and for a role name that is not
/// a plain name, the wrapper falls back to the installed binary — the derived
/// name never opens a slot that was not set, and a crafted role cannot inject.
#[test]
fn negative_proof_no_slot_or_a_crafted_role_runs_the_installed_binary() {
    let (dir, w) = fixture("neg");
    let slot = dir.join("slot").to_string_lossy().to_string();
    assert_eq!(run(&w, "abby-normal", &[]), "canonical");
    assert_eq!(run(&w, "x;echo pwned", &[("WERK_ABBY_NORMAL_BIN", slot)]), "canonical");
}
