//! Chorus perf regression suite (#2287)
//!
//! Five timing tests, one per chorus service. Runs on every gate:code
//! via `cargo test`. Fails the gate if a service regresses past its budget.
//!
//! Design: `designing/docs/chorus-perf-suite-design.md`
//!
//! All tests run with DEPLOY_ROLE=silas set — per chorus-contracts.md C1,
//! DEPLOY_ROLE=unset is a violation and not a supported test condition.

use std::process::Command;
use std::time::Instant;
use chorus_hooks::shared::state_paths::chorus_root;

/// #2614: perf tests fire real curl POST against localhost:3475 messaging API
/// and write to /tmp/chorus-chat/. On a developer machine they leave artifacts
/// in shared paths and inject test traffic into Bridge. Gated behind
/// `RUN_INTEGRATION=1`; default `cargo test` skips them with reason.
fn skip_unless_integration(reason: &str) -> bool {
    if std::env::var("RUN_INTEGRATION").is_err() {
        eprintln!("SKIP: axis-4 — {reason} (set RUN_INTEGRATION=1 to run)");
        return true;
    }
    false
}

fn nudge_script() -> String { format!("{}/platform/scripts/nudge", chorus_root()) }
fn chat_script() -> String { format!("{}/platform/scripts/chat.sh", chorus_root()) }
fn chorus_log() -> String { format!("{}/platform/scripts/chorus-log", chorus_root()) }
fn shim_bin() -> String { format!("{}/platform/services/chorus-hooks/target/release/chorus-hook-shim", chorus_root()) }
/// Nudge dry-run budget: 500ms. Measured baseline ~150ms (#2283 post-lsof-fix).
/// Exercises: detect_sender, persist curl, queue decision. Skips osascript.
#[test]
fn nudge_dry_run_under_500ms() {
    if skip_unless_integration("invokes nudge script with real role names") { return; }
    let t = Instant::now();
    let out = Command::new("bash")
        .arg(nudge_script())
        .args(["wren", "perf-nudge-dry-run"])
        .env("DEPLOY_ROLE", "silas")
        .env("CHORUS_INJECT_DRY_RUN", "1")
        .output()
        .expect("nudge script must run");
    let elapsed = t.elapsed().as_millis();

    assert!(out.status.success(), "nudge dry-run must exit 0, stderr: {}",
        String::from_utf8_lossy(&out.stderr));
    assert!(elapsed < 500,
        "nudge dry-run took {}ms, budget 500ms — {}x regression. lsof back in hot path? (#2287)",
        elapsed, elapsed / 500);
}

/// Persist-to-Bridge budget: 100ms. Measured baseline ~30ms.
/// Exercises: curl POST to localhost:3475/api/nudge.
#[test]
fn nudge_persist_under_100ms() {
    if skip_unless_integration("POSTs real nudge to localhost:3475/api/nudge") { return; }
    let t = Instant::now();
    let out = Command::new("curl")
        .args([
            "-s", "-X", "POST",
            "http://localhost:3475/api/nudge",
            "-H", "Content-Type: application/json",
            "-d", r#"{"from":"silas","to":"wren","content":"perf-persist","traceId":"perf-suite"}"#,
            "--connect-timeout", "2",
        ])
        .output();
    let elapsed = t.elapsed().as_millis();

    assert!(out.is_ok(), "curl must not error");
    assert!(elapsed < 100,
        "nudge persist took {}ms, budget 100ms. Bridge API slow? (#2287)",
        elapsed);
}

/// Chat.sh say budget: 200ms. Exercises: persist + tick marker write + file append.
/// CHAT_DRY_RUN=1 skips the nudge delivery at the end of say.
#[test]
fn chat_say_under_200ms() {
    if skip_unless_integration("writes /tmp/chorus-chat/, races live chats") { return; }
    // Start a throwaway chat first
    let start = Command::new("bash")
        .arg(chat_script())
        .args(["start", "silas", "wren", "perf-chat-test"])
        .env("CHAT_DRY_RUN", "1")
        .env("DEPLOY_ROLE", "silas")
        .output()
        .expect("chat start must run");
    let stdout = String::from_utf8_lossy(&start.stdout);
    let chat_id = stdout.lines().rfind(|l| !l.trim().is_empty())
        .unwrap_or("").trim().to_string();
    assert!(!chat_id.is_empty(), "chat start must return an ID");

    let t = Instant::now();
    let out = Command::new("bash")
        .arg(chat_script())
        .args(["say", &chat_id, "silas", "perf timing test"])
        .env("CHAT_DRY_RUN", "1")
        .env("DEPLOY_ROLE", "silas")
        .output()
        .expect("chat say must run");
    let elapsed = t.elapsed().as_millis();

    // Cleanup
    let _ = std::fs::remove_file(format!("/tmp/chorus-chat/{}.md", chat_id));
    let _ = std::fs::remove_file(format!("/tmp/chorus-chat/tick-{}", chat_id));

    assert!(out.status.success(), "chat say must exit 0");
    assert!(elapsed < 200,
        "chat.sh say took {}ms, budget 200ms (#2287)",
        elapsed);
}

/// Pulse assembly budget: 1500ms. Current baseline 628-761ms (#2172).
/// Target is sub-100ms per #2172, but until that ships the budget is loose
/// to avoid gating on unresolved perf work elsewhere.
/// Exercises: reading role states, spine events, assembling JSON.
///
/// macOS-only: budget calibrated against warm Mac caches (~1.2s on dev). Cold
/// Linux CI runner takes ~2.2s for the same path. Perf gating belongs to the
/// dev env it was measured against; CI runs untimed via the other tests.
#[cfg(target_os = "macos")]
#[test]
fn pulse_assembly_under_1500ms() {
    if skip_unless_integration("invokes shim with DEPLOY_ROLE=silas, hits role-state files") { return; }
    let t = Instant::now();
    let out = Command::new(shim_bin())
        .arg("pulse")
        .env("DEPLOY_ROLE", "silas")
        .output()
        .expect("pulse subcommand must run");
    let elapsed = t.elapsed().as_millis();

    assert!(out.status.success(), "pulse must exit 0, stderr: {}",
        String::from_utf8_lossy(&out.stderr));
    assert!(elapsed < 1500,
        "pulse assembly took {}ms, budget 1500ms. #2172 target is sub-100ms. (#2287)",
        elapsed);
}

/// Spine emit budget: 200ms. Measured baseline ~90ms.
/// Exercises: chorus-log script invocation + JSON write to log file.
#[test]
fn spine_emit_under_200ms() {
    if skip_unless_integration("writes to platform/logs/chorus.log via real chorus-log") { return; }
    let t = Instant::now();
    let out = Command::new(chorus_log())
        .args(["perf.test.event", "silas", "suite=perf,test=spine_emit"])
        .env("DEPLOY_ROLE", "silas")
        .output()
        .expect("chorus-log must run");
    let elapsed = t.elapsed().as_millis();

    assert!(out.status.success(), "chorus-log must exit 0");
    assert!(elapsed < 200,
        "spine emit took {}ms, budget 200ms (#2287)",
        elapsed);
}
