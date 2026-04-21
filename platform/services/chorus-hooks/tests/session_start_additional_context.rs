//! #2311 rescope — SessionStart emits additionalContext JSON directly,
//! no "please read /tmp/session-start-<role>.md" prose step. Context lands
//! in model view via the existing Claude Code SessionStart primitive.
//!
//! Contract: stdout of `chorus-hook-shim session-start <role>` must be valid
//! JSON with hookSpecificOutput.hookEventName == "SessionStart" and
//! hookSpecificOutput.additionalContext containing the boot payload.

use std::fs;
use std::path::Path;

const SHIM: &str = env!("CARGO_BIN_EXE_chorus-hook-shim");

/// AC #1 (rescope): session-start emits hookSpecificOutput JSON on stdout.
#[test]
fn session_start_emits_additional_context_json() {
    let output = std::process::Command::new(SHIM)
        .args(["session-start", "silas"])
        .output()
        .expect("session-start should execute");

    assert!(output.status.success(), "session-start should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);

    let v: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!(
            "stdout must be valid JSON for Claude Code SessionStart hook. \
             Got: {:?}\nParse error: {}",
            &stdout[..stdout.len().min(500)], e
        ));

    let hso = v.get("hookSpecificOutput").unwrap_or_else(|| panic!(
        "stdout JSON must have hookSpecificOutput field. Got: {}",
        v
    ));

    let event_name = hso.get("hookEventName").and_then(|x| x.as_str()).unwrap_or("");
    assert_eq!(
        event_name, "SessionStart",
        "hookSpecificOutput.hookEventName must be 'SessionStart'"
    );

    let ctx = hso.get("additionalContext").and_then(|x| x.as_str()).unwrap_or("");
    assert!(
        !ctx.is_empty(),
        "hookSpecificOutput.additionalContext must be non-empty"
    );
    assert!(
        ctx.len() > 50,
        "additionalContext must carry real boot payload (got {} bytes)",
        ctx.len()
    );
}

/// AC #1 (rescope): additionalContext contains the role's session-start content.
#[test]
fn session_start_additional_context_contains_boot_payload() {
    let output = std::process::Command::new(SHIM)
        .args(["session-start", "silas"])
        .output()
        .expect("session-start should execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(stdout.trim())
        .expect("stdout must be JSON");
    let ctx = v.get("hookSpecificOutput")
        .and_then(|h| h.get("additionalContext"))
        .and_then(|x| x.as_str())
        .expect("additionalContext must exist");

    // Session-start file content should appear in additionalContext.
    let file = fs::read_to_string("/tmp/session-start-silas.md")
        .expect("session-start file must be written");

    // A distinctive first-line marker from the cache output must be in both.
    let first_line = file.lines().next().unwrap_or("");
    assert!(
        !first_line.is_empty(),
        "session-start file must have content"
    );
    assert!(
        ctx.contains(first_line),
        "additionalContext must contain the session-start file's first line.\n\
         first_line: {:?}\n\
         additionalContext head: {:?}",
        first_line,
        &ctx[..ctx.len().min(200)]
    );
}

/// AC binary-gate (rescope): on successful boot (protocol check pass),
/// .done marker written; .pending present only if protocol check fails.
#[test]
fn session_start_writes_done_on_protocol_pass() {
    let init_dir = "/tmp/claude-session-init";
    let done = format!("{}/silas.done", init_dir);
    let _ = fs::remove_file(&done);

    let output = std::process::Command::new(SHIM)
        .args(["session-start", "silas"])
        .output()
        .expect("session-start should execute");

    assert!(output.status.success());

    // If silas CLAUDE.md matches live protocol (the normal case on a clean tree),
    // session-start should have written .done. If protocol check fails, the
    // test can't assert .done exists — but can assert the banner landed in
    // additionalContext. Cover both.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(stdout.trim())
        .expect("stdout must be JSON");
    let ctx = v.get("hookSpecificOutput")
        .and_then(|h| h.get("additionalContext"))
        .and_then(|x| x.as_str())
        .unwrap_or("");

    let is_violation = ctx.contains("PROTOCOL VIOLATION") || ctx.contains("STALE CLAUDE.md");

    if is_violation {
        assert!(
            !Path::new(&done).exists(),
            "when protocol violation, .done must NOT be written"
        );
    } else {
        assert!(
            Path::new(&done).exists(),
            "when protocol check passes, .done must be written by session-start, \
             not by a later Read handler"
        );
    }
}
