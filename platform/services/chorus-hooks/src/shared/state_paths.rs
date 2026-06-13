//! Centralized path constants for chorus-hooks (#2076, #1308).
//! Every hardcoded path in the codebase should reference these constants.
//! CHORUS_ROOT env var is the single source of truth for the repo root path.

use std::sync::LazyLock;

// Asymmetric contract by design (#2505 / #2565): prod falls back silently to
// avoid crashing personal infra on Jeff's two Macs if the env var is missing;
// tests panic explicitly on missing env to verify the contract is honored.
// If chorus ever ships beyond Jeff's hardware, swap this to
// .expect("CHORUS_ROOT must be set") with a clear error message.
static CHORUS_ROOT_INNER: LazyLock<String> = LazyLock::new(|| {
    std::env::var("CHORUS_ROOT")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/Users/jeffbridwell/CascadeProjects/chorus".to_string())
});

/// Resolve CHORUS_ROOT: env var with fallback to compile-time default.
/// All path constants derive from this value.
pub fn chorus_root() -> &'static str {
    &CHORUS_ROOT_INNER
}

/// Repository root — alias for chorus_root(). #2505: was a hardcoded Mac
/// path which broke on Linux CI once the Mac→runner symlink was retired.
/// Now delegates to chorus_root() so call sites respect CHORUS_ROOT env.
/// Kept the REPO_ROOT name (as a fn) so existing call sites can be rewritten
/// REPO_ROOT → repo_root() without semantic change.
pub fn repo_root() -> &'static str { chorus_root() }

/// Team scan directory — role state JSON files
pub const SCAN_DIR: &str = "/tmp/claude-team-scan";

/// Voice inbox directory — nudge queue files
pub const VOICE_INBOX_DIR: &str = "/tmp/voice-inbox";

/// Chorus log file — spine events (the log file, not the script).
/// Lives in ~/.chorus/ alongside index.db etc. (CSC: Runtime Artifacts).
/// Was platform/logs/chorus.log (in working tree, set by #2505); moved
/// 2026-05-04 after branch checkouts clobbered the unstaged file mid-write.
pub fn chorus_log_file() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/jeffbridwell".to_string());
    format!("{}/.chorus/chorus.log", home)
}

/// Chorus log script — emits spine events (the CLI tool)
pub fn chorus_log_script() -> String {
    format!("{}/platform/scripts/chorus-log", chorus_root())
}

/// Messages DB — the chorus-messaging SQLite store (DEC-107 persist path).
/// The #3218 nudge drain is a THIRD LOCAL reader of this (PreToolUse + Stop);
/// chorus-messaging owns writes. NEVER reach it via the :3475 API per tool call
/// (it wedges under load) — local rusqlite only.
pub fn messages_db() -> String {
    format!("{}/platform/pulse/messages.db", chorus_root())
}

/// Hook server socket
pub const HOOK_SOCKET: &str = "/tmp/chorus-hooks.sock";

/// Hook server PID file
pub const HOOK_PID: &str = "/tmp/chorus-hooks.pid";

/// Session init gate directory
pub const SESSION_INIT_DIR: &str = "/tmp/claude-session-init";

/// Chat directory
pub const CHAT_DIR: &str = "/tmp/chorus-chat";

/// Role directory mapping — resolves to chorus-relative path.
/// Post DEC-1816 namespace move, roles live at `roles/<name>`. The #1794 swat
/// updated from the old product-manager/architect/engineer names to bare
/// wren/silas/kade but dropped the `roles/` prefix, which silently broke
/// context_cache and session commands (they looked at /chorus/silas instead
/// of /chorus/roles/silas). Restored here as part of #2113.
pub fn role_dir(role: &str) -> Option<&'static str> {
    match role {
        "wren" => Some("roles/wren"),
        "silas" => Some("roles/silas"),
        "kade" => Some("roles/kade"),
        _ => None,
    }
}

/// Role CWD path
pub fn role_cwd(role: &str) -> Option<String> {
    role_dir(role).map(|d| format!("{}/{}", chorus_root(), d))
}

/// Valid roles
pub const ROLES: &[&str] = &["wren", "silas", "kade"];
