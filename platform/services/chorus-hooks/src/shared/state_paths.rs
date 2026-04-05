//! Centralized path constants for chorus-hooks (#2076).
//! Every hardcoded path in the codebase should reference these constants.

/// Team scan directory — role state JSON files
pub const SCAN_DIR: &str = "/tmp/claude-team-scan";

/// Voice inbox directory — nudge queue files
pub const VOICE_INBOX_DIR: &str = "/tmp/voice-inbox";

/// Chorus log file — spine events (the log file, not the script)
pub const CHORUS_LOG_FILE: &str = "/Users/jeffbridwell/CascadeProjects/chorus/platform/logs/chorus.log";

/// Chorus log script — emits spine events (the CLI tool)
pub const CHORUS_LOG_SCRIPT: &str = "/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log";

/// Repository root
pub const REPO_ROOT: &str = "/Users/jeffbridwell/CascadeProjects";

/// Hook server socket
pub const HOOK_SOCKET: &str = "/tmp/chorus-hooks.sock";

/// Hook server PID file
pub const HOOK_PID: &str = "/tmp/chorus-hooks.pid";

/// Session init gate directory
pub const SESSION_INIT_DIR: &str = "/tmp/claude-session-init";

/// Chat directory
pub const CHAT_DIR: &str = "/tmp/chorus-chat";

/// Role directory mapping
pub fn role_dir(role: &str) -> Option<&'static str> {
    match role {
        "wren" => Some("product-manager"),
        "silas" => Some("architect"),
        "kade" => Some("engineer"),
        _ => None,
    }
}

/// Role CWD path
pub fn role_cwd(role: &str) -> Option<String> {
    role_dir(role).map(|d| format!("{}/{}", REPO_ROOT, d))
}

/// Valid roles
pub const ROLES: &[&str] = &["wren", "silas", "kade"];
