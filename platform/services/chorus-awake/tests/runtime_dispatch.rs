use chorus_awake::{configured_runtime_profile, setup_managed_profile};
use std::os::unix::fs::PermissionsExt;

#[test]
fn profile_selection_preserves_unconfigured_legacy_and_explicit_override() {
    assert_eq!(configured_runtime_profile("wren", None, None).unwrap(), None);
    assert_eq!(configured_runtime_profile("wren", None, Some(r#"{"roles":{"kade":"codex"}}"#)).unwrap(), None);
    assert_eq!(configured_runtime_profile("wren", Some("gemini"), Some("invalid")).unwrap(), Some("gemini".into()));
    assert_eq!(configured_runtime_profile("wren", None, Some(r#"{"roles":{"wren":"opencode"}}"#)).unwrap(), Some("opencode".into()));
    assert!(configured_runtime_profile("wren", None, Some("invalid")).is_err());
    assert!(configured_runtime_profile("wren", Some(""), None).is_err());
}

#[test]
fn supervisor_failure_is_propagated_without_claude_fallback() {
    let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("chorus-awake-runtime-{}-{unique}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let stub = dir.join("agent");
    let args = dir.join("args");
    std::fs::write(&stub, "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$CAPTURE_ARGS\"\nexit 17\n").unwrap();
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o700)).unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_chorus-awake"))
        .env_clear().env("HOME", &dir).env("CHORUS_AGENT_BIN", &stub)
        .env("CHORUS_AGENT_PROFILE", "codex-approved").env("CAPTURE_ARGS", &args)
        .env("AWAKE_ROLE_DIR", "/workspace/wren")
        .env("CLAUDE_BIN", "/must-not-run-claude").arg("wren").output().unwrap();
    assert_eq!(result.status.code(), Some(17));
    assert_eq!(std::fs::read_to_string(&args).unwrap(), "launch\nwren\n--profile\ncodex-approved\n--cwd\n/workspace/wren\n");
    std::fs::remove_dir_all(dir).unwrap();
}


#[test]
fn setup_metadata_uses_exact_profile_without_latest_selection() {
    let metadata = r#"{"version":1,"deployments":{"wren":{"opencode":{"profile":"new-profile"}},"kade":{"opencode":{"profile":"kade-profile"}}},"history":{"old-profile":{"profile":"old-profile"}}}"#;
    assert!(setup_managed_profile("wren", "new-profile", Some(metadata)).unwrap());
    assert!(setup_managed_profile("wren", "old-profile", Some(metadata)).unwrap());
    assert!(!setup_managed_profile("wren", "kade-profile", Some(metadata)).unwrap());
    assert!(!setup_managed_profile("wren", "absent", Some(metadata)).unwrap());
    assert!(!setup_managed_profile("wren", "new-profile", None).unwrap());
    assert!(setup_managed_profile("wren", "new-profile", Some("{}" )).is_err());
}

#[test]
fn setup_managed_dispatch_preserves_exact_profile_and_propagates_failure() {
    let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("chorus-awake-setup-{}-{unique}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let stub = dir.join("setup");
    let args = dir.join("args");
    std::fs::write(&stub, "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$CAPTURE_ARGS\"\nexit 19\n").unwrap();
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(dir.join("agent-setup.json"), r#"{"version":1,"history":{"old-opencode-wren":{"profile":"old-opencode-wren"}}}"#).unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_chorus-awake"))
        .env_clear().env("HOME", &dir).env("CHORUS_AGENT_STATE_DIR", &dir)
        .env("CHORUS_AGENT_SETUP_BIN", &stub).env("CHORUS_AGENT_PROFILE", "old-opencode-wren")
        .env("CAPTURE_ARGS", &args).env("AWAKE_ROLE_DIR", "/must-not-override-worktree")
        .env("CHORUS_AGENT_BIN", "/must-not-admit-raw-session").env("CLAUDE_BIN", "/must-not-run-claude")
        .arg("wren").output().unwrap();
    assert_eq!(result.status.code(), Some(19));
    assert_eq!(std::fs::read_to_string(&args).unwrap(), "wake\nwren\n--profile\nold-opencode-wren\n");
    std::fs::remove_dir_all(dir).unwrap();
}
