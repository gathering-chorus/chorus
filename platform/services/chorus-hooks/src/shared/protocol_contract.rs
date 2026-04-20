//! #2311: boot-time protocol contract.
//!
//! Three-line stamp in each role's CLAUDE.md header declares what protocol
//! that file was built against:
//! ```text
//! <!-- chorus-prompt: X.Y -->
//! <!-- protocol-core: sha256=<64-hex> -->
//! <!-- role-fragments: sha256=<64-hex> -->
//! ```
//! At SessionStart the hook parses the stamps and compares to live state.
//! Three failure modes, three banners. See pair scratch /tmp/pair-2311.md
//! for the full spec.
//!
//! **Canonicalization MUST match `platform/scripts/claudemd-gen.py`**:
//! `sha256` over `sorted(rel_path\0sha256_hex(content)\0)` per file.
//! A shared test-vector file (`designing/claudemd/.protocol_test_vectors.json`)
//! pins the algorithm across Python and Rust.

use crate::shared::state_paths::chorus_root;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Stamps {
    pub chorus_prompt: String,        // e.g. "2.0"
    pub protocol_core_sha: String,    // 64-hex
    pub role_fragments_sha: String,   // 64-hex
}

#[derive(Debug)]
pub enum Violation {
    MissingStamp { missing: Vec<&'static str> },
    VersionMismatch {
        stamp_version: String,
        live_version: String,
        stamp_core: String,
        live_core: String,
    },
    Stale {
        stamp_fragments: String,
        live_fragments: String,
        staleness_s: u64,
    },
}

impl Violation {
    pub fn reason(&self) -> &'static str {
        match self {
            Violation::MissingStamp { .. } => "missing_stamp",
            Violation::VersionMismatch { .. } => "version_mismatch",
            Violation::Stale { .. } => "stale",
        }
    }
}

fn claudemd_dir() -> PathBuf {
    Path::new(chorus_root()).join("designing/claudemd")
}

fn role_claudemd_path(role: &str) -> PathBuf {
    Path::new(chorus_root()).join(format!("roles/{}/CLAUDE.md", role))
}

fn manifest_path() -> PathBuf {
    claudemd_dir().join("manifest.json")
}

fn protocol_version_path() -> PathBuf {
    claudemd_dir().join("PROTOCOL_VERSION")
}

/// Parse the three stamp lines from a CLAUDE.md file body.
/// Returns Err(list of missing stamp names) if any are absent.
pub fn parse_stamps(body: &str) -> Result<Stamps, Vec<&'static str>> {
    let mut chorus_prompt: Option<String> = None;
    let mut protocol_core: Option<String> = None;
    let mut role_fragments: Option<String> = None;

    for line in body.lines().take(20) {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("<!-- chorus-prompt:") {
            chorus_prompt = Some(
                rest.trim_end_matches("-->").trim().to_string()
            );
        } else if let Some(rest) = t.strip_prefix("<!-- protocol-core: sha256=") {
            protocol_core = Some(
                rest.trim_end_matches("-->").trim().to_string()
            );
        } else if let Some(rest) = t.strip_prefix("<!-- role-fragments: sha256=") {
            role_fragments = Some(
                rest.trim_end_matches("-->").trim().to_string()
            );
        }
    }

    let mut missing: Vec<&'static str> = Vec::new();
    if chorus_prompt.is_none() { missing.push("chorus-prompt"); }
    if protocol_core.is_none() { missing.push("protocol-core"); }
    if role_fragments.is_none() { missing.push("role-fragments"); }
    if !missing.is_empty() { return Err(missing); }

    Ok(Stamps {
        chorus_prompt: chorus_prompt.unwrap(),
        protocol_core_sha: protocol_core.unwrap(),
        role_fragments_sha: role_fragments.unwrap(),
    })
}

/// Canonical hash over a sorted set of (rel_path, content_bytes) pairs.
/// MUST match claudemd-gen.py `_hash_fragment_set`.
/// Algorithm: sha256 over sorted entries; per entry:
///   utf8(rel_path) || 0x00 || ascii(sha256_hex(content)) || 0x00
pub fn hash_entries(entries: &[(&str, &[u8])]) -> String {
    let mut sorted: Vec<&(&str, &[u8])> = entries.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    let mut h = Sha256::new();
    for (rel, content) in sorted {
        let inner_hex = format!("{:x}", Sha256::digest(content));
        h.update(rel.as_bytes());
        h.update(b"\0");
        h.update(inner_hex.as_bytes());
        h.update(b"\0");
    }
    format!("{:x}", h.finalize())
}

/// Read the files and hash them with the canonical algorithm.
pub fn hash_fragment_set(rel_paths: &[String]) -> std::io::Result<String> {
    let dir = claudemd_dir();
    let mut owned: Vec<(String, Vec<u8>)> = Vec::with_capacity(rel_paths.len());
    for rel in rel_paths {
        let full = dir.join(rel);
        let content = fs::read(&full)?;
        owned.push((rel.clone(), content));
    }
    let borrows: Vec<(&str, &[u8])> = owned.iter()
        .map(|(r, c)| (r.as_str(), c.as_slice()))
        .collect();
    Ok(hash_entries(&borrows))
}

fn load_core_paths() -> std::io::Result<Vec<String>> {
    let body = fs::read_to_string(manifest_path())?;
    let v: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let arr = v.get("protocol_core")
        .and_then(|x| x.as_array())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "manifest.protocol_core missing"))?;
    arr.iter().map(|x| x.as_str().map(String::from)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "protocol_core item not string"))).collect()
}

fn load_role_sections(role: &str) -> std::io::Result<Vec<String>> {
    let body = fs::read_to_string(manifest_path())?;
    let v: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let role_obj = v.get("roles")
        .and_then(|r| r.get(role))
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("roles.{} missing", role)))?;
    let sections = role_obj.get("sections")
        .and_then(|s| s.as_array())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "sections missing"))?;
    sections.iter()
        .map(|x| x.as_str().map(String::from)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "section not string")))
        .collect()
}

pub fn read_protocol_version() -> std::io::Result<String> {
    let p = protocol_version_path();
    if !p.exists() { return Ok("0.0".to_string()); }
    Ok(fs::read_to_string(p)?.trim().to_string())
}

/// The full contract check for a role. Ok(()) = pass; Err(Violation) = fail.
pub fn check(role: &str) -> Result<(), Violation> {
    let claudemd = match fs::read_to_string(role_claudemd_path(role)) {
        Ok(s) => s,
        Err(_) => return Err(Violation::MissingStamp {
            missing: vec!["chorus-prompt", "protocol-core", "role-fragments"]
        }),
    };

    let stamps = parse_stamps(&claudemd).map_err(|missing| Violation::MissingStamp { missing })?;

    let live_version = read_protocol_version().unwrap_or_else(|_| "0.0".to_string());
    let core_paths = load_core_paths().unwrap_or_default();
    let live_core = hash_fragment_set(&core_paths).unwrap_or_default();

    if stamps.chorus_prompt != live_version || stamps.protocol_core_sha != live_core {
        return Err(Violation::VersionMismatch {
            stamp_version: stamps.chorus_prompt,
            live_version,
            stamp_core: stamps.protocol_core_sha,
            live_core,
        });
    }

    let sections = load_role_sections(role).unwrap_or_default();
    let live_fragments = hash_fragment_set(&sections).unwrap_or_default();

    if stamps.role_fragments_sha != live_fragments {
        let staleness_s = fs::metadata(role_claudemd_path(role))
            .and_then(|m| m.modified())
            .and_then(|t| t.elapsed().map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "elapsed")))
            .map(|d| d.as_secs())
            .unwrap_or(0);
        return Err(Violation::Stale {
            stamp_fragments: stamps.role_fragments_sha,
            live_fragments,
            staleness_s,
        });
    }

    Ok(())
}

/// Format a PROTOCOL VIOLATION / STALE banner suitable for prepending to
/// `/tmp/session-start-<role>.md`. The banner is the first thing the model
/// sees when it reads its boot context — so the failure is visible at the
/// exact surface Jeff touches.
pub fn banner(role: &str, v: &Violation) -> String {
    match v {
        Violation::MissingStamp { missing } => format!(
"## 🛑 PROTOCOL VIOLATION — session boot blocked

reason: missing_stamp
role: {role}
your CLAUDE.md is missing: {missing}
fix: platform/scripts/claudemd-gen && git add roles/{role}/CLAUDE.md

Normal boot steps below will not run until this is resolved.
---
",
            role = role,
            missing = missing.join(", "),
        ),
        Violation::VersionMismatch { stamp_version, live_version, stamp_core, live_core } => format!(
"## 🛑 PROTOCOL VIOLATION — session boot blocked

reason: version_mismatch
role: {role}
your stamp: chorus-prompt/{sv} core={sc_short}
live:       chorus-prompt/{lv} core={lc_short}

You are on a different protocol from the other roles. Regenerate before continuing.
fix: cd chorus && platform/scripts/claudemd-gen && git add roles/*/CLAUDE.md designing/claudemd/PROTOCOL_VERSION

Normal boot steps below will not run until this is resolved.
---
",
            role = role,
            sv = stamp_version, lv = live_version,
            sc_short = &stamp_core.chars().take(12).collect::<String>(),
            lc_short = &live_core.chars().take(12).collect::<String>(),
        ),
        Violation::Stale { stamp_fragments, live_fragments, staleness_s } => format!(
"## ⚠ STALE CLAUDE.md — session boot blocked

reason: role-fragments hash mismatch (role-specific fragments drifted)
role: {role}
stamp fragments: {sf_short}
live fragments:  {lf_short}
staleness: {staleness_s}s

Your protocol version matches the team but your CLAUDE.md is out of date.
fix: cd chorus && platform/scripts/claudemd-gen --role {role} && git add roles/{role}/CLAUDE.md

Normal boot steps below will not run until this is resolved.
---
",
            role = role,
            sf_short = &stamp_fragments.chars().take(12).collect::<String>(),
            lf_short = &live_fragments.chars().take(12).collect::<String>(),
            staleness_s = staleness_s,
        ),
    }
}

/// Spine event fields for tracing.
pub fn event_fields(role: &str, v: &Violation) -> Vec<(&'static str, String)> {
    let mut f: Vec<(&'static str, String)> = vec![
        ("role", role.to_string()),
        ("reason", v.reason().to_string()),
    ];
    match v {
        Violation::MissingStamp { missing } => {
            f.push(("missing", missing.join(",")));
        }
        Violation::VersionMismatch { stamp_version, live_version, stamp_core, live_core } => {
            f.push(("stamp_version", stamp_version.clone()));
            f.push(("live_version", live_version.clone()));
            f.push(("stamp_core", stamp_core.clone()));
            f.push(("live_core", live_core.clone()));
        }
        Violation::Stale { stamp_fragments, live_fragments, staleness_s } => {
            f.push(("stamp_fragments", stamp_fragments.clone()));
            f.push(("live_fragments", live_fragments.clone()));
            f.push(("staleness_s", staleness_s.to_string()));
        }
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_stamps_all_three_present() {
        let body = "<!-- GENERATED by claudemd-gen.sh | v215 | 2026-04-20 16:30 | DO NOT EDIT DIRECTLY -->\n<!-- chorus-prompt: 2.0 -->\n<!-- protocol-core: sha256=33d22991bde8fc9c1ecd9bb3c182566c1a3a7cefc749f89a10ef0eeaf68f3734 -->\n<!-- role-fragments: sha256=22f77ceba8d1183c4a4f596861278ee1fc20296f7229833be3fa555ce3400f8e -->\n\n# Role\n";
        let s = parse_stamps(body).expect("parse");
        assert_eq!(s.chorus_prompt, "2.0");
        assert_eq!(s.protocol_core_sha, "33d22991bde8fc9c1ecd9bb3c182566c1a3a7cefc749f89a10ef0eeaf68f3734");
        assert_eq!(s.role_fragments_sha, "22f77ceba8d1183c4a4f596861278ee1fc20296f7229833be3fa555ce3400f8e");
    }

    #[test]
    fn parse_stamps_missing_all() {
        let body = "<!-- GENERATED by claudemd-gen.sh | v215 | 2026-04-20 16:30 | DO NOT EDIT DIRECTLY -->\n\n# Role\n";
        let missing = parse_stamps(body).unwrap_err();
        assert_eq!(missing.len(), 3);
    }

    #[test]
    fn parse_stamps_missing_one() {
        let body = "<!-- chorus-prompt: 2.0 -->\n<!-- protocol-core: sha256=abc -->\n\n# Role\n";
        let missing = parse_stamps(body).unwrap_err();
        assert_eq!(missing, vec!["role-fragments"]);
    }

    #[test]
    fn hash_fragment_set_empty_matches_python() {
        // Empty set → sha256 of zero bytes fed to hasher → the sha256 of empty input.
        let h = hash_fragment_set(&[]).unwrap();
        // sha256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        assert_eq!(h, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    /// Load the shared test vectors file and assert this Rust implementation
    /// matches the Python generator on every fixture. Cross-language parity.
    #[test]
    fn matches_python_test_vectors() {
        let path = claudemd_dir().join(".protocol_test_vectors.json");
        let body = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("test vectors missing at {:?}", path));
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["version"].as_i64(), Some(1), "vector schema version drifted");
        let fixtures = v["fixtures"].as_array().unwrap();
        for fix in fixtures {
            let name = fix["name"].as_str().unwrap();
            let expected = fix["expected_core_hash"].as_str().unwrap();
            let actual = if fix.get("core_paths_from_manifest").and_then(|x| x.as_bool()).unwrap_or(false) {
                // live_core fixture — read the 13 real paths
                let paths: Vec<String> = v["core_paths"].as_array().unwrap().iter()
                    .map(|s| s.as_str().unwrap().to_string()).collect();
                hash_fragment_set(&paths).unwrap()
            } else {
                // in-memory fixture
                let files = fix["files"].as_object().unwrap();
                let entries: Vec<(String, Vec<u8>)> = files.iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap().as_bytes().to_vec()))
                    .collect();
                let borrows: Vec<(&str, &[u8])> = entries.iter()
                    .map(|(r, c)| (r.as_str(), c.as_slice())).collect();
                hash_entries(&borrows)
            };
            assert_eq!(actual, expected,
                "fixture '{}' hash mismatch — Rust/Python canonicalization drift", name);
        }
    }

    #[test]
    fn live_roles_pass_contract() {
        // All three real role files should currently pass the contract —
        // they were just regenerated with the new stamps.
        for role in &["silas", "wren", "kade"] {
            match check(role) {
                Ok(()) => {}
                Err(v) => panic!("live role {} failed: {:?}", role, v),
            }
        }
    }

    #[test]
    fn all_three_roles_share_protocol_core() {
        // The key invariant Jeff asked for: three roles on the same protocol.
        // protocol-core hash MUST be identical across all three.
        let silas = fs::read_to_string(role_claudemd_path("silas")).unwrap();
        let wren = fs::read_to_string(role_claudemd_path("wren")).unwrap();
        let kade = fs::read_to_string(role_claudemd_path("kade")).unwrap();
        let s = parse_stamps(&silas).unwrap();
        let w = parse_stamps(&wren).unwrap();
        let k = parse_stamps(&kade).unwrap();
        assert_eq!(s.chorus_prompt, w.chorus_prompt);
        assert_eq!(w.chorus_prompt, k.chorus_prompt);
        assert_eq!(s.protocol_core_sha, w.protocol_core_sha);
        assert_eq!(w.protocol_core_sha, k.protocol_core_sha);
    }
}
