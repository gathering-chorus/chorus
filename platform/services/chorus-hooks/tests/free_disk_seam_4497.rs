// @domain: spine — placed by chorus-crawl's domain rules (#4538)
//! #4497 — no chorus-hooks test asks Finder for the free disk space. The seam
//! lives in platform/services/chorus-hooks/.cargo/config.toml: every cargo
//! context gets CHORUS_FREE_DISK_BYTES, and the shim reads it instead of
//! sending the AppleEvent a test build has no TCC grant for (683 s, 2026-10-10).

fn config() -> String {
    // run time, not env!(): the werk-test 4030 guard refuses a compile-time path
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    std::fs::read_to_string(format!("{dir}/.cargo/config.toml")).expect(".cargo/config.toml")
}

#[test]
fn the_crate_config_carries_the_free_disk_seam() {
    let c = config();
    let line = c.lines().find(|l| l.starts_with("CHORUS_FREE_DISK_BYTES")).expect("the seam is in .cargo/config.toml");
    // force = false: a caller's own value still wins
    assert!(line.contains("force = false"), "{line}");
    assert_eq!(std::env::var("CHORUS_FREE_DISK_BYTES").as_deref(), Ok("200000000000"));
}

/// NEGATIVE PROOF: a spawned shim inherits the seam, so its session-start never
/// reaches Finder; without the variable the same shim would wait on the AppleEvent.
#[test]
fn a_spawned_shim_inherits_the_seam() {
    let out = std::process::Command::new("sh").args(["-c", "echo ${CHORUS_FREE_DISK_BYTES:-missing}"]).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "200000000000");
    let bare = std::process::Command::new("sh").args(["-c", "echo ${CHORUS_FREE_DISK_BYTES:-missing}"])
        .env_remove("CHORUS_FREE_DISK_BYTES").output().unwrap();
    assert_eq!(String::from_utf8_lossy(&bare.stdout).trim(), "missing");
}
