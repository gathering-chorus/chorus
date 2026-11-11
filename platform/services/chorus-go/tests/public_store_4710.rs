//! #4710 slice 2 (Silas 18:27–18:28) — Jeff's passkey public key lives in the lander's public dir:
//! a 0644 file in a 0755 dir, both lander-owned and writable by no one else. The land runs as
//! jeffbridwell and reads it; no other account can write or swap it; a missing key refuses.
use chorus_go::*;
use std::os::unix::fs::{MetadataExt, PermissionsExt};

fn tmp(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("pub4710-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}
fn pk() -> Passkey {
    use p256::pkcs8::EncodePublicKey;
    let k = p256::ecdsa::SigningKey::random(&mut p256::elliptic_curve::rand_core::OsRng);
    Passkey { cred_id: "c".into(), spki: b64url(k.verifying_key().to_public_key_der().unwrap().as_bytes()),
              rp_id: "go.example.test".into(), origin: "https://go.example.test".into(), sign_count: 0 }
}
fn me() -> u32 { extern "C" { fn geteuid() -> u32; } unsafe { geteuid() } }

#[test]
fn the_key_is_written_public_and_read_back() {
    let d = tmp("ok");
    let s = Store::open(&d).unwrap();
    s.save_passkey(&pk()).unwrap();
    assert_eq!(std::fs::metadata(&s.public).unwrap().mode() & 0o777, 0o755);
    assert_eq!(std::fs::metadata(s.public.join("passkey.json")).unwrap().mode() & 0o777, 0o644);
    assert_eq!(std::fs::metadata(&d).unwrap().mode() & 0o777, 0o700, "the challenges dir stays private");
    assert!(s.passkey().unwrap().is_some());
}

// NEGATIVE PROOFS: a group-writable dir (a rename swaps the file), a group-writable file, and a
// key owned by another account are each refused.
#[test]
fn a_writable_dir_or_file_or_a_foreign_owner_is_refused() {
    let d = tmp("neg");
    let s = Store::open(&d).unwrap();
    s.save_passkey(&pk()).unwrap();
    std::fs::set_permissions(&s.public, std::fs::Permissions::from_mode(0o775)).unwrap();
    assert!(s.passkey().unwrap_err().contains("writable by others"));
    std::fs::set_permissions(&s.public, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::set_permissions(s.public.join("passkey.json"), std::fs::Permissions::from_mode(0o666)).unwrap();
    assert!(s.passkey().unwrap_err().contains("writable by others"));
    std::fs::set_permissions(s.public.join("passkey.json"), std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(public_problem(&s.public, me() + 1).unwrap().contains("not the lander"));
    assert!(public_problem(&s.public, me()).is_none());
}

// The land finds the lander by account, not by a path in the environment.
#[test]
fn the_lander_is_found_by_account() {
    assert!(account("no-such-account-4710").is_none());
    match account(LANDER_ACCOUNT) {
        Some((_, home)) => assert_eq!(Store::lander_public(), Some(home.join("public"))),
        None => eprintln!("unmeasured: no chorus-lander account on this box"),
    }
}

// NEGATIVE PROOF (fail closed): `chorus-go check` with no key on the lander refuses (exit 1);
// before slice 2 it printed "no passkey registered" and exited 0, letting the land go on.
#[test]
fn check_with_no_key_on_the_lander_refuses() {
    if Store::lander_public().map(|p| p.join("passkey.json").exists()).unwrap_or(false) {
        eprintln!("unmeasured: the lander already holds a key here"); return;
    }
    let h = tmp("check");
    std::fs::create_dir_all(&h).unwrap();
    let o = std::process::Command::new(env!("CARGO_BIN_EXE_chorus-go")).args(["check", "4710", "f39d803dd09794e645778328a44acf2b9ca7b3a3"])
        .env("HOME", &h).output().unwrap();
    assert_eq!(o.status.code(), Some(1), "{}", String::from_utf8_lossy(&o.stdout));
    assert!(String::from_utf8_lossy(&o.stderr).contains("no passkey registered on the lander"));
}
