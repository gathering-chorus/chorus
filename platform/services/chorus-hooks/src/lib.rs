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
