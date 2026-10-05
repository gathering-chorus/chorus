// #4429 — a stop gate bounces a reply at most once per turn. When Claude Code
// sends stop_hook_active=true the role is already rewriting because a stop gate
// bounced it; the handler must allow before any gate runs. 2026-10-04: one
// Wren reply was bounced word-cap → word-cap (same stale count) → stated-intent
// → stated-intent, and Kade's sessions died after the same chain.

fn stop_hook_body() -> String {
    let path = format!("{}/src/main.rs", std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("main.rs missing at {path}: {e}"));
    let start = src.find("async fn stop_hook(").expect("stop_hook handler gone; re-point this guard");
    let rest = &src[start..];
    let end = rest.find("\n}\n").expect("stop_hook end not found");
    rest[..end].to_string()
}

/// Byte offset of the yield, and of the first gate that can block, in order.
fn yield_before_gates(body: &str) -> Result<(), String> {
    let y = body.find("\"stop_hook_active\"").ok_or("no stop_hook_active yield in stop_hook")?;
    for gate in ["autonomy_guard::check", "stated_intent_block", "owes_response", "word_cap::check_cap"] {
        if let Some(g) = body.find(gate) {
            if g < y {
                return Err(format!("{gate} runs before the stop_hook_active yield"));
            }
        }
    }
    Ok(())
}

#[test]
fn continuation_after_a_bounce_is_never_bounced_again() {
    let body = stop_hook_body();
    assert!(body.contains("stated_intent_block"), "gate set moved; re-point this guard");
    yield_before_gates(&body).unwrap();
    assert!(body.contains("stop.gate.yielded"), "yield must log what it skipped");
}

/// Negative proof: the order check fails on the pre-#4429 handler shape, where
/// gates ran with no yield, and where a gate sits ahead of the yield.
#[test]
fn guard_catches_missing_or_late_yield() {
    let no_yield = "let mut response = hooks::autonomy_guard::check(&input, &state).await;\n\
                    if let Some(block) = stated_intent_block(&response).await {}";
    assert!(yield_before_gates(no_yield).is_err());
    let late = "stated_intent_block(); if raw.get(\"stop_hook_active\") {}";
    assert!(yield_before_gates(late).is_err());
}
