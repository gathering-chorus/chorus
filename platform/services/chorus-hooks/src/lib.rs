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
// #2558 — narrow test surface (per silas gate:arch on #2558). Integration
// tests need session_init_gate's check_with_dir + AppState + HookInput;
// expose ONLY those symbols, not the whole modules. Broad `pub mod` would
// let tests grow to depend on private-shaped items and lock them as API.
// This is the deliberate test API surface — add new entries here only when
// a tests/ file requires them. Modules stay private; named symbols
// re-export. Marked test-surface, not stable public API.
mod hooks;
mod state;
mod types;
mod session_cache;
pub use hooks::session_init_gate;
pub use state::AppState;
pub use types::HookInput;

/// Remove the daemon's runtime files (socket + pid) on clean shutdown.
///
/// #2559: shutdown_signal previously removed only the socket, leaving a
/// stale PID file. Scripts running `kill $(cat /tmp/chorus-hooks.pid)`
/// could then target a recycled-PID unrelated process. Paths flow as
/// parameters (matches #2558 parameter > shared-state pattern). Idempotent
/// under double-call (SIGINT+SIGTERM race) and missing files: `let _ =`
/// discards both Ok and Err (NotFound), so a second call is a no-op.
pub fn cleanup_runtime_files(socket: &std::path::Path, pid: &std::path::Path) {
    let _ = std::fs::remove_file(socket);
    let _ = std::fs::remove_file(pid);
}
