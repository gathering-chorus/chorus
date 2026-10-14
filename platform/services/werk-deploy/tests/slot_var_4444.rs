// @test-type: unit — renders and runs the wrapper with /bin/sh in a temp dir, no live services.
// @domain: werk
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
    mk(&dir.join("verb-bin"), "canonical");
    let w = dir.join("verb");
    std::fs::write(&w, canonical_wrapper("verb", dir.join("verb-bin").to_str().unwrap())).unwrap();
    std::fs::set_permissions(&w, std::fs::Permissions::from_mode(0o755)).unwrap();
    (dir, w.to_string_lossy().to_string())
}

fn run(w: &str, role: &str, envs: &[(&str, String)]) -> String {
    let mut c = Command::new("/bin/sh");
    c.arg(w).env_clear().env("PATH", "/usr/bin:/bin").env("CHORUS_ROLE", role);
    for (k, v) in envs { c.env(k, v); }
    String::from_utf8_lossy(&c.output().unwrap().stdout).trim().to_string()
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
