//! Centralized path constants for chorus-hooks (#2076, #1308).
//! Every hardcoded path in the codebase should reference these constants.
//! CHORUS_ROOT env var is the single source of truth for the repo root path.

use std::sync::LazyLock;

static CHORUS_ROOT_INNER: LazyLock<String> = LazyLock::new(|| {
    std::env::var("CHORUS_ROOT")
        .unwrap_or_else(|_| "/Users/jeffbridwell/CascadeProjects/chorus".to_string())
});

/// Resolve CHORUS_ROOT: env var with fallback to compile-time default.
/// All path constants derive from this value.
pub fn chorus_root() -> &'static str {
    &CHORUS_ROOT_INNER
}

/// Repository root — alias for chorus_root(). Existing call sites use REPO_ROOT;
/// update them to use chorus_root() over time.
pub static REPO_ROOT: &str = "/Users/jeffbridwell/CascadeProjects/chorus";

/// Team scan directory — role state JSON files
pub const SCAN_DIR: &str = "/tmp/claude-team-scan";

/// Voice inbox directory — nudge queue files
pub const VOICE_INBOX_DIR: &str = "/tmp/voice-inbox";

/// Chorus log file — spine events (the log file, not the script)
pub fn chorus_log_file() -> String {
    format!("{}/platform/logs/chorus.log", chorus_root())
}

/// Chorus log script — emits spine events (the CLI tool)
pub fn chorus_log_script() -> String {
    format!("{}/platform/scripts/chorus-log", chorus_root())
}

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
        "wren" => Some("wren"),
        "silas" => Some("silas"),
        "kade" => Some("kade"),
        _ => None,
    }
}

/// Role CWD path
pub fn role_cwd(role: &str) -> Option<String> {
    role_dir(role).map(|d| format!("{}/{}", chorus_root(), d))
}

/// Valid roles
pub const ROLES: &[&str] = &["wren", "silas", "kade"];
