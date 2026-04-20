//! #2301: Role settings.json files export DEPLOY_ROLE for each role session.
//!
//! Behavior: when Claude Code starts in roles/<role>/, it reads
//! .claude/settings.json and applies the env block. DEPLOY_ROLE is then
//! available to every bash subprocess spawned from that session — the
//! binary contract (#2287) is self-satisfied, no inline env required.

use std::fs;

fn role_settings_path(role: &str) -> String {
    // CARGO_MANIFEST_DIR = chorus/platform/services/chorus-hooks
    // roles/<role>/.claude/settings.json lives three levels up.
    format!(
        "{}/../../../roles/{}/.claude/settings.json",
        env!("CARGO_MANIFEST_DIR"),
        role
    )
}

#[test]
fn silas_settings_exports_deploy_role() {
    let content = fs::read_to_string(role_settings_path("silas"))
        .expect("silas/.claude/settings.json must exist");
    let v: serde_json::Value = serde_json::from_str(&content)
        .expect("silas settings.json must be valid JSON");
    assert_eq!(
        v["env"]["DEPLOY_ROLE"].as_str(),
        Some("silas"),
        "silas settings.json must set DEPLOY_ROLE=silas in env block"
    );
}

#[test]
fn wren_settings_exports_deploy_role() {
    let content = fs::read_to_string(role_settings_path("wren"))
        .expect("wren/.claude/settings.json must exist");
    let v: serde_json::Value = serde_json::from_str(&content)
        .expect("wren settings.json must be valid JSON");
    assert_eq!(
        v["env"]["DEPLOY_ROLE"].as_str(),
        Some("wren"),
        "wren settings.json must set DEPLOY_ROLE=wren in env block"
    );
}

#[test]
fn kade_settings_exports_deploy_role() {
    let content = fs::read_to_string(role_settings_path("kade"))
        .expect("kade/.claude/settings.json must exist");
    let v: serde_json::Value = serde_json::from_str(&content)
        .expect("kade settings.json must be valid JSON");
    assert_eq!(
        v["env"]["DEPLOY_ROLE"].as_str(),
        Some("kade"),
        "kade settings.json must set DEPLOY_ROLE=kade in env block"
    );
}

/// The three role files must have matching role names (no copy-paste mistakes).
#[test]
fn role_settings_have_matching_role_names() {
    for role in &["silas", "wren", "kade"] {
        let content = fs::read_to_string(role_settings_path(role)).unwrap();
        let v: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(
            v["env"]["DEPLOY_ROLE"].as_str().unwrap(),
            *role,
            "role directory {} must set DEPLOY_ROLE={} — mismatch would corrupt attribution",
            role, role
        );
    }
}

/// #2311: every role's settings.json must register a SessionStart hook that arms
/// the session_init_gate. Without this, the gate is inert from turn zero and a
/// role can fabricate the Chorus Prompt header without reading session-start.md.
/// Any regression that drops this block silently re-opens the forge-the-header bug.
#[test]
fn role_settings_register_session_start_hook() {
    for role in &["silas", "wren", "kade"] {
        let content = fs::read_to_string(role_settings_path(role)).unwrap();
        let v: serde_json::Value = serde_json::from_str(&content).unwrap();

        let hooks = v["hooks"]["SessionStart"].as_array().unwrap_or_else(|| {
            panic!(
                "{} settings.json must register hooks.SessionStart — without it, session_init_gate stays inert",
                role
            )
        });
        assert!(
            !hooks.is_empty(),
            "{} SessionStart array must contain at least one hook entry",
            role
        );

        let inner = hooks[0]["hooks"].as_array().unwrap_or_else(|| {
            panic!("{} SessionStart[0].hooks must be an array", role)
        });
        assert!(
            !inner.is_empty(),
            "{} SessionStart[0].hooks must contain a command",
            role
        );

        let cmd = inner[0]["command"].as_str().unwrap_or_else(|| {
            panic!("{} SessionStart command must be a string", role)
        });
        assert!(
            cmd.contains("chorus-hook-shim"),
            "{} SessionStart must invoke chorus-hook-shim, got: {}",
            role, cmd
        );
        assert!(
            cmd.contains(&format!("session-start {}", role)),
            "{} SessionStart command must pass matching role arg, got: {}",
            role, cmd
        );
    }
}
