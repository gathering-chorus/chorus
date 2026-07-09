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

// #3631 (Kade's review) — the legacy /tmp consts HOOK_SOCKET/HOOK_PID were
// DELETED. They were unused, but a `pub const` pointing at world-writable /tmp
// is a loaded gun: any future code that grabs it silently reintroduces the exact
// path this card abandons. The only socket/pid paths are hook_socket_durable()
// / hook_pid_durable() (~/.chorus/run), which hard-fail rather than fall back.

/// Hook server PID file — durable contract home (#3606). ~/.chorus/run is
/// never OS-evicted; orphan detection and tests read this path.
pub fn hook_pid_durable() -> String {
    format!("{}/chorus-hooks.pid", hook_run_dir())
}

/// #3631 — the hook control socket, moved OFF world-writable /tmp into the
/// durable run dir (~/.chorus/run, 0700). Two reasons, both load-bearing:
/// (1) /tmp is OS-evicted and world-writable → the 14h flap + a stale-socket
/// race; (2) the daemon enforces every security guard, so a 0o777 control
/// socket any local process could connect to / delete was a real hole. Both
/// the daemon (main.rs) and the shim (shim.rs) resolve THIS one function so
/// they can never drift apart. The socket file itself is chmod 0600.
pub fn hook_socket_durable() -> String {
    format!("{}/chorus-hooks.sock", hook_run_dir())
}

/// The run dir (~/.chorus/run) — created 0700 by the daemon at startup.
/// #3631 (Kade's review): HARD-FAIL if $HOME is unset — do NOT fall back to
/// /tmp. A silent /tmp fallback would reintroduce the exact evictable,
/// world-writable path this fix exists to abandon; better to refuse to start
/// than to silently run the guard daemon's socket in a world-writable dir.
pub fn hook_run_dir() -> String {
    let home = std::env::var("HOME").expect(
        "chorus-hooks: HOME unset — refusing to fall back to world-writable /tmp \
         for the control socket/pidfile (#3631). Set HOME in the LaunchAgent env.",
    );
    format!("{}/.chorus/run", home)
}

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
