// #4429 — the context-synthesis gate logs, it never refuses a write.
// Measured 2026-10-04 across all three roles' transcripts since 09-16: the gate
// refused a write 148 times and 17 of those were followed straight away by a
// safety-classifier stop, after which every later turn in that session was
// refused. Rewording the refusal (#4391, 09-27) did not help: 8 of the next 34.
// A refused write sends the role back to re-ask itself; that loop is what Jeff
// named the core issue. The gate keeps its info! log lines (decision=advisory).

fn gate_source() -> String {
    let path = format!("{}/src/hooks/memory_gate.rs", std::env::var("CARGO_MANIFEST_DIR").unwrap());
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("gate source missing at {path}: {e}"))
}

/// Everything before `#[cfg(test)]` — the gate itself, not its unit tests.
fn gate_body(src: &str) -> &str {
    src.split("#[cfg(test)]").next().unwrap_or(src)
}

fn refusals(body: &str) -> usize {
    body.matches("HookResponse::deny(").count()
}

#[test]
fn context_synthesis_gate_never_refuses_a_write() {
    let src = gate_source();
    let body = gate_body(&src);
    // A guard whose target moved must fail loudly, not pass vacuously.
    assert!(body.contains("pub fn check("), "memory_gate.rs no longer has check(); re-point this guard");
    assert!(body.contains("gate = \"context-synthesis\""), "gate log lines are gone; re-point this guard");
    assert_eq!(refusals(body), 0, "the context-synthesis gate refuses a write again (#4429)");
    assert!(body.contains("decision = \"advisory\""), "advisory log lines missing");
}

/// Negative proof: the counter sees a refusal when one is there, as in the
/// gate before #4429 (three `return HookResponse::deny(&permission_deny_json(...))`).
#[test]
fn guard_catches_a_refusal_when_present() {
    let before = "pub fn check() { gate = \"context-synthesis\"; \
                  return HookResponse::deny(&permission_deny_json(SEARCHED_NO_PLAN)); }\n\
                  #[cfg(test)] mod tests { HookResponse::deny( }";
    assert_eq!(refusals(gate_body(before)), 1);
}
