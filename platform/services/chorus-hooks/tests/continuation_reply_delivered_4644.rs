// @domain: spine
// #4644 — the reply a continuation ends with stands, so it is delivered. #4429 made
// the stop_hook_active branch return before every gate; it also returned before
// reply.emitted and the Buzz delivery, so every reply a stop gate bounced and the
// role rewrote never reached the room (Wren's C4 answer, 2026-10-18 18:33).

fn body_of(src: &str, header: &str) -> String {
    let start = src.find(header).unwrap_or_else(|| panic!("{header} gone; re-point this guard"));
    let rest = &src[start..];
    let end = rest.find("\n}\n").expect("function end not found");
    rest[..end].to_string()
}

fn main_rs() -> String {
    let path = format!("{}/src/main.rs", std::env::var("CARGO_MANIFEST_DIR").unwrap());
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("main.rs missing at {path}: {e}"))
}

/// The continuation branch of stop_hook hands off to a function that delivers.
fn continuation_delivers(stop_hook: &str, continuation: &str) -> Result<(), String> {
    let y = stop_hook.find("\"stop_hook_active\"").ok_or("no stop_hook_active branch")?;
    let branch_end = stop_hook[y..].find("\n }").map(|e| y + e).ok_or("branch end not found")?;
    if !stop_hook[y..branch_end].contains("deliver_continuation_reply") {
        return Err("the stop_hook_active branch returns without delivering the reply".into());
    }
    if !continuation.contains("emit_and_deliver_reply") {
        return Err("deliver_continuation_reply does not emit and deliver".into());
    }
    Ok(())
}

#[test]
fn a_rewritten_reply_reaches_the_room() {
    let src = main_rs();
    let stop = body_of(&src, "async fn stop_hook(");
    let cont = body_of(&src, "async fn deliver_continuation_reply(");
    continuation_delivers(&stop, &cont).unwrap();
    let deliver = body_of(&src, "async fn emit_and_deliver_reply(");
    assert!(deliver.contains("\"reply.emitted\"") && deliver.contains("deliver_and_emit"),
        "emit_and_deliver_reply must stamp reply.emitted and deliver over Buzz");
}

/// Negative proof: the pre-#4644 branch (log, then allow) is caught.
#[test]
fn guard_catches_a_branch_that_drops_the_reply() {
    let old = "if raw.get(\"stop_hook_active\").and_then(|v| v.as_bool()) == Some(true) {\n \
               crate::state::chorus_log(\"stop.gate.yielded\", r, &[]).await;\n \
               return Json(HookResponse::allow());\n }";
    assert!(continuation_delivers(old, "emit_and_deliver_reply").is_err());
}
