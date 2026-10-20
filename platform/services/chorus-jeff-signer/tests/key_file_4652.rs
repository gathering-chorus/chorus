// @test-type: unit — hermetic: runs the built binary against temp key files; it exits before any network.
// @domain: security
// @card: #4652
// @owner: silas
//! #4652 — the signer refuses to start on a key file anyone else could read.
//! A looser file means the wall is already down; starting anyway would hide that.
use std::os::unix::fs::PermissionsExt;
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn bin() -> std::path::PathBuf {
    // Run time, never compile time (the 4030 rule): this test runs from
    // target/<profile>/deps/, and the binary it starts sits one level up.
    let exe = std::env::current_exe().unwrap();
    let bin = exe.parent().unwrap().parent().unwrap().join("chorus-jeff-signer");
    assert!(bin.is_file(), "the signer binary is not built at {}", bin.display());
    bin
}

fn run_with_key(mode: u32, contents: &str) -> (i32, String) {
    let dir = std::env::temp_dir().join(format!("jeff-signer-keyfile-{}-{mode:o}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let key = dir.join("key");
    std::fs::write(&key, contents).unwrap();
    std::fs::set_permissions(&key, std::fs::Permissions::from_mode(mode)).unwrap();
    let mut child = Command::new(bin())
        .env("CHORUS_SIGNER_KEY_FILE", &key)
        .env("CHORUS_SIGNER_RELAY", "ws://127.0.0.1:9")
        .env("CHORUS_SIGNER_CLIENTS", "")
        .env("CHORUS_SIGNER_TEAM_H", "t")
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // A signer that did not refuse goes on to retry the relay forever; after
    // 5 s that is a failed refusal, reported as code -1, never a hang.
    let start = Instant::now();
    let code = loop {
        if let Some(st) = child.try_wait().unwrap() {
            break st.code().unwrap_or(-1);
        }
        if start.elapsed() > Duration::from_secs(5) {
            let _ = child.kill();
            let _ = child.wait();
            break -1;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let mut err = String::new();
    child.stderr.take().unwrap().read_to_string(&mut err).unwrap();
    (code, err)
}

// NEGATIVE PROOF: a group- or world-readable key file stops the signer cold.
#[test]
fn a_key_file_others_can_read_is_refused() {
    for mode in [0o644, 0o640, 0o604] {
        let (code, err) = run_with_key(mode, &"01".repeat(32));
        assert_eq!(code, 2, "mode {mode:o} must refuse: {err}");
        assert!(err.contains("must be 0600"), "mode {mode:o}: {err}");
    }
}

#[test]
fn a_key_file_that_is_not_a_key_is_refused() {
    let (code, err) = run_with_key(0o600, "not a key");
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("key file"), "{err}");
}
