// @test-type: unit
// @domain: security
// #4747 — the signer process (src/main.rs) refuses to start rather than run on
// a key anyone else could read or a configuration it does not have. What Jeff
// sees: a loose key file or a missing setting stops the daemon with the reason,
// and nothing ever connects or signs.
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

fn signer(key_file: &str, extra: &[(&str, &str)]) -> (i32, String) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_chorus-jeff-signer"));
    c.env_clear().env("CHORUS_SIGNER_KEY_FILE", key_file);
    // keep coverage's profile path, so the process's own lines count
    if let Ok(p) = std::env::var("LLVM_PROFILE_FILE") {
        c.env("LLVM_PROFILE_FILE", p);
    }
    for (k, v) in extra {
        c.env(k, v);
    }
    let out = c.output().expect("signer runs");
    (out.status.code().unwrap_or(-1), String::from_utf8_lossy(&out.stderr).to_string())
}

/// A key file in its own temp dir (no tempfile dep), removed when the guard drops.
struct KeyDir(std::path::PathBuf);
impl Drop for KeyDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn key_file(mode: u32) -> (KeyDir, String) {
    key_file_with(mode, "0000000000000000000000000000000000000000000000000000000000000001")
}

fn key_file_with(mode: u32, key: &str) -> (KeyDir, String) {
    let dir = std::env::temp_dir().join(format!("signer-4747-{}-{mode:o}-{}", std::process::id(), &key[60..]));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("key");
    std::fs::write(&p, format!("{key}\n")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(mode)).unwrap();
    let s = p.to_str().unwrap().to_string();
    (KeyDir(dir), s)
}

#[test]
fn a_key_file_others_can_read_stops_the_signer() {
    let (_d, p) = key_file(0o644);
    let (code, err) = signer(&p, &[]);
    assert_eq!(code, 2);
    assert!(err.contains("must be 0600"), "{err}");
}

#[test]
fn a_missing_key_file_stops_the_signer() {
    let (code, err) = signer("/nonexistent/chorus-signer-key", &[]);
    assert_eq!(code, 2);
    assert!(err.contains("key file /nonexistent/chorus-signer-key"), "{err}");
}

/// NEGATIVE PROOF: a 0600 key passes the wall, so the stop that follows is the
/// missing setting, not the key — the check separates its two states.
#[test]
fn a_private_key_passes_and_a_missing_setting_stops_it() {
    let (_d, p) = key_file(0o600);
    let (code, err) = signer(&p, &[]);
    assert_eq!(code, 2);
    assert!(!err.contains("must be 0600"), "{err}");
    assert!(err.contains("CHORUS_SIGNER_CLIENTS is not set"), "{err}");
}

/// With every setting present, a key that is not a valid secret (zero) stops
/// the signer before it dials anything.
#[test]
fn a_key_that_is_no_secret_stops_the_signer_before_it_connects() {
    let (_d, p) = key_file_with(0o600, "0000000000000000000000000000000000000000000000000000000000000000");
    let settings = [
        ("CHORUS_SIGNER_CLIENTS", "aa"),
        ("CHORUS_SIGNER_CHANNEL_H", "clearing"),
        ("CHORUS_SIGNER_RELAY", "ws://127.0.0.1:1"),
    ];
    let (code, err) = signer(&p, &settings);
    assert_eq!(code, 2, "{err}");
    assert!(!err.contains("connected to"), "{err}");
}
