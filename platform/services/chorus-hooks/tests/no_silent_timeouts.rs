// #3048 — context_inject must push live search + Loki context every prompt,
// and must NEVER drop it silently. This guards the three failure modes we hit:
//   1. per-call request timeouts (search/athena/Loki) — fail-open to empty
//   2. a 700ms envelope_budget that skipped athena/Loki when the envelope ran long
//   3. the #2249 CONTEXT_PUSH_MODE manifest short-circuit that returned early
//      and left the whole search/Loki path as dead code
//
// All three were "fast and wrong, silently" — exactly what Jeff rejected (A:
// wait the few seconds, get the context). These are source-level guards so a
// future change can't quietly re-introduce any of them.

use std::fs;
use std::path::PathBuf;

fn src() -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/hooks/context_inject.rs");
    fs::read_to_string(&p).expect("read context_inject.rs")
}

#[test]
fn context_inject_has_no_request_timeouts() {
    let count = src().matches(".timeout(").count();
    assert_eq!(
        count, 0,
        "context_inject.rs still has {} `.timeout(` call(s) — per-prompt context \
         calls must run uncapped (#3048). A timeout here silently drops context.",
        count
    );
}

#[test]
fn context_inject_has_no_silent_envelope_budget() {
    assert!(
        !src().contains("envelope_budget"),
        "envelope_budget is back — that 700ms wall-clock cap silently skips \
         athena/Loki when the envelope runs long (#3048). Remove it; never \
         skip a context fetch silently."
    );
}

#[test]
fn context_inject_does_not_short_circuit_the_dynamic_path() {
    let body = src();
    assert!(
        !body.contains("env::var(\"CONTEXT_PUSH_MODE\")"),
        "the CONTEXT_PUSH_MODE env fork is back — the manifest branch returned early \
         and shadowed the search/Loki path (#2249 → dead code). The manifest and the \
         dynamic synthesis must BOTH run every prompt (#3048)."
    );
    assert!(
        body.contains("cached_query_chorus_hybrid") && body.contains("query_recent_log_errors"),
        "the search + Loki halves must both be present in context_inject (#3048)."
    );
}
