//! #3288 — version coherence gate: the committed Werk version must agree
//! with the committed constituent set, by construction.
//!
//! This replaces the retired runtime stamp-compare (protocol_contract) and
//! the frozen-hash layer: instead of detecting drift at session boot, the
//! repo itself cannot be incoherent without this test going red. The single
//! authority is designing/claudemd/version-ledger.json; PROTOCOL_VERSION is
//! a rendered cache of its head. `claudemd-gen check-version` owns the
//! comparison (one implementation of the hash, in Python); this test shells
//! it against THIS tree — no live-repo or daemon coupling.

use std::process::Command;

fn tree_root() -> String {
    // CARGO_MANIFEST_DIR = <tree>/platform/services/chorus-hooks — resolve
    // the tree under test, not canonical, so the gate proves the committed
    // state of the code it ships with.
    format!("{}/../../..", std::env::var("CARGO_MANIFEST_DIR").unwrap())
}

#[test]
fn committed_version_state_is_coherent() {
    let root = tree_root();
    let out = Command::new("python3")
        .args([
            &format!("{}/platform/scripts/claudemd-gen.py", root),
            &format!("{}/designing/claudemd/manifest.json", root),
            &format!("{}/designing/claudemd", root),
            "check-version",
            "",
            "",
        ])
        .output()
        .expect("python3 + claudemd-gen.py must be runnable");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "claudemd-gen check-version failed — a constituent changed without \
         `claudemd-gen bump`, or PROTOCOL_VERSION was hand-edited off the \
         ledger head.\nstdout: {}\nstderr: {}",
        stdout, stderr
    );
    assert!(
        stdout.contains("version coherent"),
        "check-version must report coherence; got: {}",
        stdout
    );
}

#[test]
fn ledger_head_version_is_strict_semver() {
    let root = tree_root();
    let ledger = std::fs::read_to_string(format!(
        "{}/designing/claudemd/version-ledger.json",
        root
    ))
    .expect("version-ledger.json must exist (seeded by #3288)");
    let v: serde_json::Value = serde_json::from_str(&ledger).expect("ledger is JSON");
    let entries = v["entries"].as_array().expect("ledger has entries");
    assert!(!entries.is_empty(), "ledger must have at least the seed entry");
    let head = entries.last().unwrap();
    let version = head["version"].as_str().expect("head has version");
    let re_ok = version.split('.').count() == 3
        && version.split('.').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
    assert!(re_ok, "ledger head version '{}' is not MAJOR.MINOR.PATCH", version);
    // PROTOCOL_VERSION is the rendered cache of the head — must agree.
    let rendered = std::fs::read_to_string(format!(
        "{}/designing/claudemd/PROTOCOL_VERSION",
        root
    ))
    .expect("PROTOCOL_VERSION exists");
    assert_eq!(rendered.trim(), version, "PROTOCOL_VERSION must equal ledger head");
}
