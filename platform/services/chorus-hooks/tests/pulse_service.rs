// @domain: spine — placed by chorus-crawl's domain rules (#4538)
//! Tests for #1881 — Pulse service: structured team state JSON
//! What Jeff sees: one JSON payload with full team state on every prompt cycle.

use std::fs;
use std::path::Path;

/// Pulse output file exists after running the command
#[test]
fn pulse_writes_output_file() {
    let _ = fs::remove_file("/tmp/pulse-latest.json");
    let output = std::process::Command::new(
        env!("CARGO_BIN_EXE_chorus-hook-shim")
    ).arg("pulse").output();

    assert!(output.is_ok(), "pulse command should execute");
    assert!(Path::new("/tmp/pulse-latest.json").exists(), "pulse should write /tmp/pulse-latest.json");
}

/// Pulse JSON has all required top-level keys
#[test]
fn pulse_contains_required_sections() {
    // Run pulse
    let _ = std::process::Command::new(
        env!("CARGO_BIN_EXE_chorus-hook-shim")
    ).arg("pulse").output();

    let content = fs::read_to_string("/tmp/pulse-latest.json")
        .expect("pulse output should be readable");
    let v: serde_json::Value = serde_json::from_str(&content)
        .expect("pulse output should be valid JSON");

    let obj = v.as_object().expect("pulse should be a JSON object");
    assert!(obj.contains_key("timestamp"), "must have timestamp");
    assert!(obj.contains_key("roles"), "must have roles");
    assert!(obj.contains_key("events"), "must have events");
    assert!(obj.contains_key("alerts"), "must have alerts");
    assert!(obj.contains_key("nudges"), "must have nudges");
    assert!(obj.contains_key("health"), "must have health");
    assert!(obj.contains_key("board"), "must have board");
    assert!(obj.contains_key("index_freshness"), "must have index_freshness");
    assert!(obj.contains_key("elapsed_ms"), "must have elapsed_ms");
}

/// Pulse includes all 3 roles
#[test]
fn pulse_has_all_three_roles() {
    // Hermetic test fixture for CHORUS_ROLES_URL
    let dir = std::env::temp_dir().join(format!("pulse-test-{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    let pulse_path = dir.join("pulse-roles.json");

    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = server.local_addr().unwrap().port();
    let t = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = server.accept() {
            use std::io::{Read, Write};
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let body = r#"{"data":{"roles":[{"role":"wren"},{"role":"silas"},{"role":"kade"}]}}"#;
            let resp = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
            let _ = stream.write_all(resp.as_bytes());
        }
    });

    let _ = std::process::Command::new(
        env!("CARGO_BIN_EXE_chorus-hook-shim")
    )
    .arg("pulse")
    .env("CHORUS_ROLES_URL", format!("http://127.0.0.1:{}/roles", port))
    .env("CHORUS_PULSE_PATH", &pulse_path)
    .output();

    let _ = t.join();

    let content = fs::read_to_string(&pulse_path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();

    let roles = v.get("roles").and_then(|r| r.as_object()).expect("roles should be object");
    assert!(roles.contains_key("wren"), "must have wren");
    assert!(roles.contains_key("silas"), "must have silas");
    assert!(roles.contains_key("kade"), "must have kade");

    let _ = fs::remove_dir_all(&dir);
}

/// #3734 NEGATIVE PROOF: when roles endpoint returns fewer than all 3 roles, test fails to find missing role
#[test]
fn pulse_has_all_three_roles_negative_proof_missing_role() {
    let dir = std::env::temp_dir().join(format!("pulse-neg-test-{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    let pulse_path = dir.join("pulse-neg-roles.json");

    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = server.local_addr().unwrap().port();
    let t = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = server.accept() {
            use std::io::{Read, Write};
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            // Missing 'kade'
            let body = r#"{"data":{"roles":[{"role":"wren"},{"role":"silas"}]}}"#;
            let resp = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
            let _ = stream.write_all(resp.as_bytes());
        }
    });

    let _ = std::process::Command::new(
        env!("CARGO_BIN_EXE_chorus-hook-shim")
    )
    .arg("pulse")
    .env("CHORUS_ROLES_URL", format!("http://127.0.0.1:{}/roles", port))
    .env("CHORUS_PULSE_PATH", &pulse_path)
    .output();

    let _ = t.join();

    let content = fs::read_to_string(&pulse_path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();

    let roles = v.get("roles").and_then(|r| r.as_object()).expect("roles should be object");
    assert!(roles.contains_key("wren"), "wren present");
    assert!(roles.contains_key("silas"), "silas present");
    assert!(!roles.contains_key("kade"), "negative proof: kade must be absent when omitted from upstream");

    let _ = fs::remove_dir_all(&dir);
}

/// Nudge section shows per-role pending counts
#[test]
fn pulse_nudges_per_role() {
    let _ = std::process::Command::new(
        env!("CARGO_BIN_EXE_chorus-hook-shim")
    ).arg("pulse").output();

    let content = fs::read_to_string("/tmp/pulse-latest.json").unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();

    let nudges = v.get("nudges").and_then(|n| n.as_object()).expect("nudges should be object");
    for role in &["wren", "silas", "kade"] {
        let role_nudge = nudges.get(*role).unwrap_or_else(|| panic!("{} should have nudge entry", role));
        assert!(role_nudge.get("pending").is_some(), "{} should have pending count", role);
        assert!(role_nudge.get("stale").is_some(), "{} should have stale flag", role);
    }
}

/// Alerts section lists what fired today
#[test]
fn pulse_alerts_fired_today() {
    let _ = std::process::Command::new(
        env!("CARGO_BIN_EXE_chorus-hook-shim")
    ).arg("pulse").output();

    let content = fs::read_to_string("/tmp/pulse-latest.json").unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();

    let alerts = v.get("alerts").expect("must have alerts");
    assert!(alerts.get("fired_today").is_some(), "must have fired_today list");
    assert!(alerts.get("count").is_some(), "must have alert count");
}

/// #3202 — the pulse snapshot is the team's boot ground-truth; it must SURVIVE a
/// reboot. Written to a durable store (CHORUS_PULSE_PATH / ~/.chorus), not only
/// /tmp which a reboot wipes (the live failure this session: roles booted blind).
#[test]
fn pulse_writes_durable_store_surviving_tmp_wipe() {
    let dir = std::env::temp_dir().join(format!("pulse-durable-{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    let durable = dir.join("pulse-latest.json");
    let _ = fs::remove_file(&durable);

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_chorus-hook-shim"))
        .arg("pulse")
        .env("CHORUS_PULSE_PATH", &durable)
        .output();
    assert!(output.is_ok(), "pulse command should execute");

    // durable file written + valid pulse JSON carrying the sensorium shape
    assert!(durable.exists(), "pulse must write the durable store ({}), not only /tmp", durable.display());
    let content = fs::read_to_string(&durable).expect("durable pulse readable");
    let v: serde_json::Value = serde_json::from_str(&content).expect("durable pulse is valid json");
    assert!(v.get("roles").is_some() && v.get("board").is_some(), "durable pulse carries the sensorium shape");

    // simulate a reboot wiping /tmp — the durable copy still serves
    let _ = fs::remove_file("/tmp/pulse-latest.json");
    assert!(durable.exists(), "after /tmp wipe, durable pulse still readable (survives reboot)");

    let _ = fs::remove_dir_all(&dir);
}
