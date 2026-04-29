//! chorus-hooks library surface — exposes modules that need integration-test
//! visibility (#2477). Binaries (chorus-hooks server, chorus-hook-shim CLI)
//! continue to declare modules privately via #[path]; this lib.rs only
//! re-exports the modules tests outside the binary need to import.

pub mod mcp_client;
// #2505 — expose `shared` so integration tests can call
// `chorus_hooks::shared::state_paths::chorus_root()` instead of hardcoding
// the Mac path. main.rs and shim.rs continue to declare `mod shared;` for
// their own bin compilation; this is a parallel re-export for tests/.
pub mod shared;
// #2558 — expose hooks/state/types so integration tests can call gate
// functions directly with constructed HookInput + AppState, escaping the
// daemon-vs-test race on global /tmp/claude-session-init. Same pattern as
// #2505's shared exposure: parallel re-export for tests/, binaries declare
// their own mod ... for bin compilation.
pub mod hooks;
pub mod state;
pub mod types;
mod session_cache;
