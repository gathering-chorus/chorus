// @test-type: unit — renders and runs the wrapper with /bin/sh in a temp dir, no live services.
// @domain: builds
// #4414 (AC2): a card's slot build never answers Jeff's shell — only the role's own account, or a runner that opts in
//! #4444 reopen 3 — a hyphenated role finds its werk bin slot. #4530 run 6 (14 Oct
//! 19:24): werk-deploy read WERK_ABBY-NORMAL_BIN, found nothing, and rolled back a
//! good install as "installed=None".
use werk_deploy::{canonical_wrapper, refresh_stale_wrappers, werk_slot_var};
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
    run_as(w, role, None, envs)
}

/// #4414 — runs the wrapper as if `account` called it: a stub `id -un` answers the
/// account name; every other `id` call goes to the real one (uid checks stay real).
fn run_as(w: &str, role: &str, account: Option<&str>, envs: &[(&str, String)]) -> (i32, String, String) {
    let mut path = "/usr/bin:/bin".to_string();
    if let Some(a) = account {
        // one stub dir per account, reused across calls in this test process
        let d = std::env::temp_dir().join(format!("id-4414-{}-{}", a, std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let id = d.join("id");
        std::fs::write(&id, format!("#!/bin/sh\nif [ \"$1\" = -un ]; then echo {a}; else exec /usr/bin/id \"$@\"; fi\n")).unwrap();
        std::fs::set_permissions(&id, std::fs::Permissions::from_mode(0o755)).unwrap();
        path = format!("{}:{path}", d.display());
    }
    let mut c = Command::new("/bin/sh");
    c.arg(w).env_clear().env("PATH", path).env("CHORUS_ROLE", role);
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
    let (code, out, err) = run_full(&w, "wren", &[("WERK_WREN_BIN", slot.to_string_lossy().to_string()), ("CHORUS_USE_SLOT", "1".into())]);
    assert_eq!((code, out.as_str()), (126, ""), "{err}");
    assert!(err.contains("writable by group or other"), "{err}");

    let (dir, w) = fixture("4853file");
    std::fs::set_permissions(dir.join("slot/verb"), std::fs::Permissions::from_mode(0o775)).unwrap();
    let (code, _, err) = run_full(&w, "wren", &[("WERK_WREN_BIN", dir.join("slot").to_string_lossy().to_string()), ("CHORUS_USE_SLOT", "1".into())]);
    assert_eq!(code, 126, "{err}");

    let (dir, w) = fixture("4853link");
    std::fs::remove_file(dir.join("slot/verb")).unwrap();
    std::os::unix::fs::symlink(dir.join("verb-bin"), dir.join("slot/verb")).unwrap();
    let (code, out, err) = run_full(&w, "wren", &[("WERK_WREN_BIN", dir.join("slot").to_string_lossy().to_string()), ("CHORUS_USE_SLOT", "1".into())]);
    assert_eq!((code, out.as_str()), (126, ""), "{err}");
    assert!(err.contains("symlink"), "{err}");
}

#[test]
fn the_wrapper_runs_abby_normals_slot() {
    let (dir, w) = fixture("abby");
    let slot = dir.join("slot").to_string_lossy().to_string();
    assert_eq!(run(&w, "abby-normal", &[("WERK_ABBY_NORMAL_BIN", slot), ("CHORUS_USE_SLOT", "1".into())]), "slot");
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

/// #4414 AC2 — Jeff 09-30 15:48: his shell (CHORUS_ROLE=silas, WERK_SILAS_BIN set by
/// chorus-env-setup) ran silas's unlanded slot build, which refused him. NEGATIVE PROOF:
/// a slot is set and trusted, the caller is not the role's account and did not opt in:
/// the installed binary answers, never the slot.
#[test]
fn negative_proof_jeffs_shell_never_runs_a_roles_slot() {
    let (dir, w) = fixture("4414jeff");
    let slot = dir.join("slot").to_string_lossy().to_string();
    let (code, out, err) = run_as(&w, "silas", Some("jeffbridwell"), &[("WERK_SILAS_BIN", slot)]);
    assert_eq!((code, out.as_str()), (0, "canonical"), "{err}");
}

/// #4414 AC2 — the role's own account runs its own slot with no opt-in; another role's
/// account does not; a runner that opts in (CHORUS_USE_SLOT=1) does.
#[test]
fn a_roles_own_account_or_an_opted_in_runner_runs_the_slot() {
    let (dir, w) = fixture("4414own");
    let slot = dir.join("slot").to_string_lossy().to_string();
    assert_eq!(run_as(&w, "silas", Some("chorus-silas"), &[("WERK_SILAS_BIN", slot.clone())]).1, "slot");
    assert_eq!(run_as(&w, "silas", Some("chorus-kade"), &[("WERK_SILAS_BIN", slot.clone())]).1, "canonical");
    assert_eq!(run_as(&w, "silas", Some("jeffbridwell"), &[("WERK_SILAS_BIN", slot), ("CHORUS_USE_SLOT", "1".into())]).1, "slot");
}

/// #4414 AC2 — Jeff's shell sources chorus-env-setup.sh. NEGATIVE PROOF: if that file
/// ever exported the opt-in, every shell would reach every slot again.
#[test]
fn negative_proof_the_shared_env_file_never_opts_in() {
    let f = concat!(env!("CARGO_MANIFEST_DIR"), "/../../scripts/chorus-env-setup.sh");
    let text = std::fs::read_to_string(f).expect("chorus-env-setup.sh is where the slot vars are exported");
    assert!(text.contains("WERK_SILAS_BIN"), "guard target moved: {f}");
    assert!(!text.contains("CHORUS_USE_SLOT"), "chorus-env-setup.sh must never set the slot opt-in");
}

/// #4414 (Silas 14:07) — the new rule reaches every installed wrapper, not only the verb
/// that deployed. NEGATIVE PROOF: a wrapper with the old text is rewritten; a file that
/// werk-deploy did not write (no header, or no -bin beside it) is never touched.
#[test]
fn a_stale_wrapper_is_rewritten_and_nothing_else_is_touched() {
    let d = std::env::temp_dir().join(format!("refresh-4414-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d); // code review: start clean
    std::fs::create_dir_all(&d).unwrap();
    let b = d.to_string_lossy().to_string();
    std::fs::write(d.join("werk-test-bin"), "bin").unwrap();
    std::fs::write(d.join("werk-test"), "#!/bin/sh\n# Auto-generated by werk-deploy (#3101). Routes to role-slot first.\nold\n").unwrap();
    std::fs::set_permissions(d.join("werk-test"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let ino_before = { use std::os::unix::fs::MetadataExt; std::fs::metadata(d.join("werk-test")).unwrap().ino() };
    std::fs::write(d.join("hand-script"), "#!/bin/sh\necho mine\n").unwrap();
    std::fs::write(d.join("orphan"), "#!/bin/sh\n# Auto-generated by werk-deploy (#3101).\nold\n").unwrap();
    // review 2 — an older header variant (same prefix) is still refreshed; a symlinked wrapper is not
    std::fs::write(d.join("cards-bin"), "bin").unwrap();
    std::fs::write(d.join("cards"), "#!/bin/sh\n# Auto-generated by werk-deploy (#3101): an older header\nold\n").unwrap();
    std::fs::write(d.join("real-target"), "#!/bin/sh\n# Auto-generated by werk-deploy (#3101). Routes to role-slot first.\nold\n").unwrap();
    std::fs::write(d.join("linked-bin"), "bin").unwrap();
    std::os::unix::fs::symlink(d.join("real-target"), d.join("linked")).unwrap();
    assert_eq!(refresh_stale_wrappers(&b), vec!["cards".to_string(), "werk-test".to_string()]);
    assert!(std::fs::symlink_metadata(d.join("linked")).unwrap().file_type().is_symlink(), "a symlinked wrapper is never replaced");
    let w = std::fs::read_to_string(d.join("werk-test")).unwrap();
    assert!(w.contains("CHORUS_USE_SLOT"), "{w}");
    // Silas 14:31 — replaced by rename, not rewritten in place (a new inode), mode kept, no temp left
    { use std::os::unix::fs::MetadataExt;
      let m = std::fs::metadata(d.join("werk-test")).unwrap();
      assert_ne!(m.ino(), ino_before, "rewritten in place: a hook mid-exec could read half a script");
      assert_eq!(m.mode() & 0o777, 0o755); }
    assert!(std::fs::read_dir(&d).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().contains(".replace-")));
    assert_eq!(std::fs::read_to_string(d.join("hand-script")).unwrap(), "#!/bin/sh\necho mine\n");
    assert!(std::fs::read_to_string(d.join("orphan")).unwrap().ends_with("old\n"));
    // already current: nothing to do
    assert!(refresh_stale_wrappers(&b).is_empty());
    let _ = std::fs::remove_dir_all(&d);
}
