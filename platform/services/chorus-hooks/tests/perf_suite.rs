// @domain: spine — placed by chorus-crawl's domain rules (#4538)
// @needs: nightly — five wall-clock budgets (#4539: card runs leave it to the nightly)
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

fn chat_script() -> String { format!("{}/platform/scripts/chat.sh", chorus_root()) }
fn chorus_log() -> String { format!("{}/platform/scripts/chorus-log", chorus_root()) }
// #2287 reopened 2026-10-03 (Kade, #4228 run 3): the shim the test just built,
// never <root>/target/release, which a werk never builds (NotFound panic).
fn shim_bin() -> String { env!("CARGO_BIN_EXE_chorus-hook-shim").to_string() }
// Prior work: #2287 introduced this 500ms budget on the bash nudge script's
// dry-run path; #2614 hermeticity-gated it behind RUN_INTEGRATION.
// Current state: #2804 cut bash off the canonical send path; #2809 deletes the
// script entirely. The budget can no longer measure what it was filed to guard.
// Approach: drop the test outright. Pulse worker latency (the canonical
// delivery primitive) is covered by the delivery-worker test suite — that's
// the right place for new latency budgets if we want them.

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

    let mut samples = Vec::new();
    let mut all_ok = true;
    for _ in 0..5 {
        let t = Instant::now();
        let out = Command::new("bash")
            .arg(chat_script())
            .args(["say", &chat_id, "silas", "perf timing test"])
            .env("CHAT_DRY_RUN", "1")
            .env("DEPLOY_ROLE", "silas")
            .output()
            .expect("chat say must run");
        samples.push(t.elapsed().as_millis());
        all_ok &= out.status.success();
    }

    // Cleanup
    let _ = std::fs::remove_file(format!("/tmp/chorus-chat/{}.md", chat_id));
    let _ = std::fs::remove_file(format!("/tmp/chorus-chat/tick-{}", chat_id));

    assert!(all_ok, "chat say must exit 0");
    let elapsed = median_ms(samples.clone());
    assert!(elapsed < 200,
        "chat.sh say took {}ms (median of {:?}), budget 200ms (#2287)",
        elapsed, samples);
}

/// #4645 — one wall-clock sample measures the box as much as the code. On
/// 2026-10-19 one `say` took 226ms on a loaded box (it passed the six nights
/// before); measured at load 70 the same call ran 61-94ms with one 265ms spike.
/// The median of five is over budget only when most runs are slow, so a red
/// means `say` got slower, not that one run hit a busy moment.
fn median_ms(mut samples: Vec<u128>) -> u128 {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

#[test]
fn median_ignores_two_spikes_but_not_three_slow_runs() {
    // the 10-19 shape: mostly fast, a spike or two → under budget
    assert!(median_ms(vec![80, 265, 85, 226, 90]) < 200);
    // negative proof: three of five slow is a slower `say` → over budget
    assert!(median_ms(vec![210, 80, 230, 90, 250]) >= 200);
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
    // #2287 (2026-10-03): the first exec of a freshly built binary pays macOS's
    // first-run cost — measured 0.50s for a fresh copy vs 0.01s on its second run,
    // and this test read 2622ms / 2430ms on the first run after a build, ~670ms after.
    // A werk always builds fresh. One untimed run, so the budget measures pulse
    // assembly, not the OS seeing a new binary.
    let _ = Command::new(shim_bin()).arg("pulse").env("DEPLOY_ROLE", "silas").output();
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

// #4539 — moved here from pulse_service.rs: the one wall-clock test there.
// This file is `@needs: nightly`; the rest of pulse_service stays in card runs.
/// Pulse completes under budget.
/// Original budget was 200ms (#1896). Real-world stand-alone is 500-600ms as of
/// 2026-04-17; pulse picked up more sources without budget retuning. Under
/// parallel cargo test load (the nightly sweep runs ~20 test binaries
/// concurrently), pulse competes for CPU and can hit 1000-1500ms.
/// 2000ms here is a regression-catcher under contention, not a perf target —
/// #2158 is the card to drive the stand-alone number back toward 200ms.
///
/// macOS-only: same dev-env-budget reasoning as
/// perf_suite::pulse_assembly_under_1500ms (e0d7422a). CI Linux runner
/// pulse takes >2s cold; that's runner-perf, not regression signal.
#[cfg(target_os = "macos")]
#[test]
fn pulse_runs_under_budget() {
    let _ = std::process::Command::new(
        env!("CARGO_BIN_EXE_chorus-hook-shim")
    ).arg("pulse").output();

    let content = std::fs::read_to_string("/tmp/pulse-latest.json").unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();

    let elapsed = v.get("elapsed_ms").and_then(|e| e.as_u64()).unwrap_or(9999);
    assert!(elapsed < 2000, "pulse must complete in <2000ms under parallel load (#2158 to tighten), got {}ms", elapsed);
}
