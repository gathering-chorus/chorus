// @test-type: unit
// @domain: security
//! #4853 — a slot binary runs as the runner's account, so the runner refuses a
//! slot (file or dir) that another account owns or that group/other can write.
//! What Jeff sees: no role account can get its own code run as his account.
use chorus_runner::server::{binary_path, slot_refusal};
use std::os::unix::fs::PermissionsExt;

fn slot(tag: &str, dir_mode: u32, file_mode: u32) -> std::path::PathBuf {
    let base = std::env::temp_dir().join(format!("werk-base-4853-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let bin = base.join("wren-bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("werk-build"), "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(bin.join("werk-build"), std::fs::Permissions::from_mode(file_mode)).unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(dir_mode)).unwrap();
    base
}

#[test]
fn a_runner_owned_0755_slot_runs_and_group_writable_slots_are_refused_naming_the_path() {
    // one test: CHORUS_WERK_BASE is process-global
    let good = slot("good", 0o755, 0o755);
    std::env::set_var("CHORUS_WERK_BASE", &good);
    assert_eq!(binary_path("werk-build", "wren").unwrap(), good.join("wren-bin/werk-build").display().to_string());

    // negative proof: the same binary in a group-writable dir is refused, and the error names it
    let dir_gw = slot("dirgw", 0o775, 0o755);
    std::env::set_var("CHORUS_WERK_BASE", &dir_gw);
    let e = binary_path("werk-build", "wren").unwrap_err();
    assert!(e.contains("wren-bin") && e.contains("group or other"), "{e}");

    // negative proof: a group-writable binary in a good dir is refused
    let file_gw = slot("filegw", 0o755, 0o775);
    std::env::set_var("CHORUS_WERK_BASE", &file_gw);
    let e = binary_path("werk-build", "wren").unwrap_err();
    assert!(e.contains("werk-build") && e.contains("mode 775"), "{e}");

    // negative proof: a symlinked slot is refused even when its target is trusted
    let link = slot("link", 0o755, 0o755);
    std::fs::rename(link.join("wren-bin/werk-build"), link.join("real")).unwrap();
    std::os::unix::fs::symlink(link.join("real"), link.join("wren-bin/werk-build")).unwrap();
    std::env::set_var("CHORUS_WERK_BASE", &link);
    let e = binary_path("werk-build", "wren").unwrap_err();
    assert!(e.contains("symlink"), "{e}");
    std::env::remove_var("CHORUS_WERK_BASE");
}

#[test]
fn another_accounts_file_is_refused_even_at_0755() {
    // the live finding: silas-bin/security owned by chorus-borg-queen
    assert!(slot_refusal(504, 0o100755, 502).unwrap().contains("uid 504"));
    assert!(slot_refusal(502, 0o100755, 502).is_none());
    assert!(slot_refusal(502, 0o100757, 502).is_some(), "other-writable");
}

/// #4853 demo find (12:18): a role running `chorus-runner call` itself refused a
/// slot the installer (jeffbridwell) owns. The installer's slots run; a slot
/// owned by any other account is still refused (negative proof, the live
/// silas-bin/security case, is `another_accounts_file_is_refused_even_at_0755`).
#[test]
fn a_slot_owned_by_the_installer_is_trusted_by_a_role_running_call() {
    use std::os::unix::fs::MetadataExt;
    let state = std::env::temp_dir().join(format!("state-4853-{}", std::process::id()));
    std::fs::create_dir_all(state.join("bin")).unwrap();
    std::env::set_var("CHORUS_STATE_DIR", &state);
    let base = slot("installer", 0o755, 0o755);
    std::env::set_var("CHORUS_WERK_BASE", &base);
    // the test account is both installer and caller here; the owner check passes, the mode check still applies
    assert_eq!(std::fs::metadata(state.join("bin")).unwrap().uid(), std::fs::metadata(&base).unwrap().uid());
    assert!(binary_path("werk-build", "wren").is_ok());
    std::env::remove_var("CHORUS_WERK_BASE");
    std::env::remove_var("CHORUS_STATE_DIR");
}
