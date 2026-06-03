//! chorus-hooks library surface — exposes modules that need integration-test
//! visibility (#2477). Binaries (chorus-hooks server, chorus-hook-shim CLI)
//! continue to declare modules privately via #[path]; this lib.rs only
//! re-exports the modules tests outside the binary need to import.

// #3150: the bins (chorus-hooks server, chorus-hook-shim) declare these modules
// privately via #[path] and USE their items; this lib re-exports only a narrow
// test surface. So the lib target's dead_code analysis flags ~235 items as "never
// used" that the bins actually use — false positives that swamp the real signal
// (cognitive_complexity, too_many_arguments) and trip the clippy-ratchet on noise.
// Suppress dead_code crate-wide; the real clippy lints stay on.
#![allow(dead_code)]

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
// #3134 — expose the pure per-prompt search URL-builder for its integration
// test (tests/context_inject_card_tag_3134.rs). Stateless helper, narrow surface.
pub use hooks::context_inject::build_search_url;
// #3134 — expose the spine-line formatter for its observability test.
pub use hooks::context_inject::format_spine_line;
// #3191 — expose the pure UserPromptSubmit response router for its delivery test
// (tests/prompt_response_3191.rs): context block → stdout additionalContext, warnings → stderr.
pub use hooks::context_inject::build_user_prompt_response;
// #3191 (relevance half) — the semantic leg (full-prompt query) + the FTS/semantic
// merge, pub for tests/relevance_3191.rs (AC6/AC7/AC8). Pure, stateless helpers.
pub use hooks::context_inject::{build_semantic_url, merge_candidates};
// #3203 — the inject FORCE verdict (forcing pattern / HIP-001), pub for tests/inject_force_3203.rs
pub use hooks::inject_force::{
    inject_engagement_verdict, last_assistant_text, read_surfaced_in, record_surfaced_in,
    EngagementVerdict,
};
pub use state::AppState;
pub use types::HookInput;
// #2790 — chorus-hook-shim invokes canonical_write_guard in-process on every
// Edit/Write/MultiEdit BEFORE forwarding to daemon. HookResponse is the return
// type; shim needs `.stdout` to detect deny. Re-export so the shim binary's
// compilation unit can reach the field. Same narrow-test-surface shape as
// #2177 / #2558 / #2735 (re-exports for binary consumers, not public API).
pub use types::HookResponse;
// #2644 AC2 — narrow test-surface re-export so integration tests can verify
// the CHORUS_FIX_CARD_OVERRIDE deterministic-smoke contract.
pub use types::is_fix_card;
// #2735 — canonical-write-guard test surface
pub mod canonical_write_guard {
    pub use crate::hooks::canonical_write_guard::check;
}
// #2177 — accept_gate test surface (demo evidence reads card comment, not brief file)
pub mod accept_gate {
    pub use crate::hooks::accept_gate::demo_evidence_exists;
}

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
