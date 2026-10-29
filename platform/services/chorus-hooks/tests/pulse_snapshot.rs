// @domain: spine — placed by chorus-crawl's domain rules (#4538)
//! #2120 — pulse fires on every post-tool-use, so /tmp/pulse-latest.json is always hot.
//! This test invokes the shim's `pulse` subcommand and verifies the snapshot file
//! is rewritten with the expected top-level keys. It's the contract downstream
//! readers (tiles.ts, Clearing) rely on.

use std::fs;
use std::process::Command;
use std::time::SystemTime;

const SHIM: &str = env!("CARGO_BIN_EXE_chorus-hook-shim");

#[test]
fn pulse_subcommand_rewrites_snapshot_with_required_keys() {
    let p = std::env::temp_dir().join(format!("pulse-snapshot-test-{}.json", std::process::id()));
    fs::write(&p, "{}").expect("initial write");
    let before = fs::metadata(&p).and_then(|m| m.modified()).ok();

    // Small delay so mtime resolution (~1s on some FS) can distinguish before/after
    std::thread::sleep(std::time::Duration::from_millis(1100));

    let output = Command::new(SHIM)
        .arg("pulse")
        .env("CHORUS_PULSE_PATH", &p)
        .output()
        .expect("shim pulse should execute");
    assert!(output.status.success(), "pulse should exit 0");

    let content = fs::read_to_string(&p).expect("pulse snapshot file should exist");
    let parsed: serde_json::Value = serde_json::from_str(&content).expect("valid json");

    for key in ["timestamp", "roles", "events", "alerts", "health", "board"] {
        assert!(
            parsed.get(key).is_some(),
            "pulse snapshot missing required key `{}`",
            key
        );
    }

    // mtime must advance — proves the subcommand rewrote the file, not just touched
    let after = fs::metadata(&p).unwrap().modified().unwrap();
    if let Some(b) = before {
        assert!(
            after > b,
            "pulse file not rewritten (before={:?} after={:?})",
            b,
            b.duration_since(SystemTime::UNIX_EPOCH).ok()
        );
    }

    // All three roles must appear in roles section
    let roles = parsed.get("roles").and_then(|v| v.as_object()).expect("roles object");
    for role in ["wren", "silas", "kade"] {
        assert!(
            roles.contains_key(role),
            "pulse.roles missing {}",
            role
        );
    }
    let _ = fs::remove_file(&p);
}
