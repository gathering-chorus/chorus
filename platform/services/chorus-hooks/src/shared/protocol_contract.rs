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
    // #2731: Stale variant retired. Under the derived-artifact model,
    // SessionStart regenerates roles/<role>/CLAUDE.md from fragments before
    // running this check (commands/session.rs AC4), so a fragment-hash
    // mismatch here cannot mean "the role's CLAUDE.md drifted." If stamp
    // != live after regen, that is a claudemd-gen output bug (Python/Rust
    // hash divergence), pinned by designing/claudemd/.protocol_test_vectors.json
    // and caught in CI — not a runtime failure mode.
}

impl Violation {
    pub fn reason(&self) -> &'static str {
        match self {
            Violation::MissingStamp { .. } => "missing_stamp",
            Violation::VersionMismatch { .. } => "version_mismatch",
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


pub fn read_protocol_version() -> std::io::Result<String> {
    let p = protocol_version_path();
    if !p.exists() { return Ok("0.0".to_string()); }
    Ok(fs::read_to_string(p)?.trim().to_string())
}

/// #3254 — the pure pass/fail decision, split out of `check()` so the contract LOGIC is
/// hermetically testable without reading live role files. Given a role's parsed stamps and
/// the freshly-computed live version + core hash, decide. No IO, no live-doc coupling — so a
/// unit test of this never goes stale on a CLAUDE.md regen.
pub(crate) fn evaluate_stamps(stamps: &Stamps, live_version: &str, live_core: &str) -> Result<(), Violation> {
    if stamps.chorus_prompt != live_version || stamps.protocol_core_sha != live_core {
        return Err(Violation::VersionMismatch {
            stamp_version: stamps.chorus_prompt.clone(),
            live_version: live_version.to_string(),
            stamp_core: stamps.protocol_core_sha.clone(),
            live_core: live_core.to_string(),
        });
    }
    Ok(())
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

    // #2731: role-fragments staleness check retired. SessionStart's defensive regen
    // (commands/session.rs AC4) guarantees the file the protocol check reads was just
    // rewritten from the live fragment set, so a stamp-vs-live mismatch here would be a
    // claudemd-gen output bug, not drift. Cross-language hash agreement is pinned by
    // designing/claudemd/.protocol_test_vectors.json and verified in CI.
    evaluate_stamps(&stamps, &live_version, &live_core)
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
        // #2731: Stale variant retired — banner removed.
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
        // #2731: Stale variant retired — no event fields needed.
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
        // #3254: SYNTHETIC fixed-byte fixtures only. The old `live_core` fixture pinned the
        // hash of the 13 LIVE protocol-core fragments — a content snapshot that went stale on
        // every CLAUDE.md regen (e23de77→c6958c6e in one day), crying wolf in the nightly. The
        // invariant worth testing is "Rust canonicalization == the Python generator's" — proven
        // by FIXED inputs that never change. Whether the live doc matches its fragments is the
        // runtime guard's job (`check()`, self-relative stamp-vs-fresh-hash), not this unit test.
        for fix in fixtures {
            let name = fix["name"].as_str().unwrap();
            let expected = fix["expected_core_hash"].as_str().unwrap();
            let files = fix["files"].as_object().unwrap_or_else(|| panic!(
                "fixture '{}' has no synthetic 'files' — live-content fixtures were removed (#3254); \
                 add fixed bytes, never pin live-fragment hashes", name));
            let entries: Vec<(String, Vec<u8>)> = files.iter()
                .map(|(k, val)| (k.clone(), val.as_str().unwrap().as_bytes().to_vec()))
                .collect();
            let borrows: Vec<(&str, &[u8])> = entries.iter()
                .map(|(r, c)| (r.as_str(), c.as_slice())).collect();
            let actual = hash_entries(&borrows);
            assert_eq!(actual, expected,
                "fixture '{}' hash mismatch — Rust/Python canonicalization drift", name);
        }
    }

    // #3254 — these replace `live_roles_pass_contract` + `all_three_roles_share_protocol_core`,
    // which READ the live role CLAUDE.md files and went red whenever the tree was mid-drift
    // (e.g. a role stamped a different protocol version) — a unit test coupled to live doc
    // content, the exact cry-wolf smell. The CONTRACT LOGIC is what a unit test should cover;
    // whether the deployed docs are fresh/consistent is the runtime guard's job at session
    // start (#2731 defensive regen), not a unit assertion.
    #[test]
    fn contract_logic_passes_when_stamps_match_live() {
        let s = Stamps { chorus_prompt: "1.4".into(), protocol_core_sha: "abc123".into(), role_fragments_sha: "def456".into() };
        assert!(evaluate_stamps(&s, "1.4", "abc123").is_ok());
    }

    #[test]
    fn contract_logic_fails_on_stale_version_or_core() {
        let s = Stamps { chorus_prompt: "1.4".into(), protocol_core_sha: "abc123".into(), role_fragments_sha: "def456".into() };
        assert!(evaluate_stamps(&s, "1.5", "abc123").is_err(), "stale version must fail");
        assert!(evaluate_stamps(&s, "1.4", "deadbeef").is_err(), "stale core must fail");
    }

    #[test]
    fn parse_stamps_extracts_version_and_core() {
        // The "three roles share protocol-core" property reduces, for a unit test, to:
        // parse_stamps reads the stamps correctly. Cross-role equality of the LIVE files is a
        // runtime/integration invariant the guard enforces, not a unit concern.
        let md = "<!-- chorus-prompt: 1.4 -->\n<!-- protocol-core: sha256=abc123 -->\n<!-- role-fragments: sha256=def456 -->\n# Role\n";
        let s = parse_stamps(md).unwrap();
        assert_eq!(s.chorus_prompt, "1.4");
        assert_eq!(s.protocol_core_sha, "abc123");
        assert_eq!(s.role_fragments_sha, "def456");
    }
}
