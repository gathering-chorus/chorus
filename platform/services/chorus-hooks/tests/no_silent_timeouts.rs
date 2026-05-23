// #3048 — the per-prompt context_inject path must NOT carry request timeouts.
// AC1: all per-prompt timeouts removed (search, athena, Loki) so the calls
// complete instead of failing open to empty context, silently.
// AC4 (spirit): a silent timeout must never be re-added — this test is the guard.
//
// Red before the fix (three `.timeout(...)` calls present); green after.

use std::fs;
use std::path::PathBuf;

#[test]
fn context_inject_has_no_request_timeouts() {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/hooks/context_inject.rs");
    let body = fs::read_to_string(&src).expect("read context_inject.rs");
    let count = body.matches(".timeout(").count();
    assert_eq!(
        count, 0,
        "context_inject.rs still has {} `.timeout(` call(s) — per-prompt context calls \
         must run uncapped (#3048). A timeout here silently drops search/Loki context.",
        count
    );
}
