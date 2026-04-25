//! #2450 — SessionStart injects live principles into additionalContext.
//!
//! Jeff's experience: every role boots with the current principle set
//! visible in their first turn (rendered from /api/loom/principles), a
//! sibling principles-hash so cross-role drift is detectable, and a graceful
//! degraded surface when the API is unavailable or empty.
//!
//! These tests use the CHORUS_PRINCIPLES_FIXTURE_FILE override to bypass HTTP
//! and feed deterministic JSON. CHORUS_PRINCIPLES_CACHE_FILE redirects the
//! cache path so concurrent test runs don't collide.

use std::fs;
use std::path::Path;

const SHIM: &str = env!("CARGO_BIN_EXE_chorus-hook-shim");

fn fixture_three_principles(path: &Path) {
    fs::write(path, r#"{
      "_meta": {"source": "fixture"},
      "data": {"principles": [
        {"id": "hemenway-observe", "label": "Observe", "comment": "Watch before acting.", "techReading": "", "jeffReading": "", "order": 1},
        {"id": "hemenway-stack", "label": "Stack functions", "comment": "Multiple yields.", "techReading": "", "jeffReading": "", "order": 2},
        {"id": "hemenway-edge", "label": "Use edge", "comment": "Diversity at boundaries.", "techReading": "", "jeffReading": "", "order": 3}
      ]}
    }"#).unwrap();
}

fn fixture_empty(path: &Path) {
    fs::write(path, r#"{"_meta":{},"data":{"principles":[]}}"#).unwrap();
}

fn run_session_start(role: &str, env: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = std::process::Command::new(SHIM);
    cmd.args(["session-start", role]);
    for (k, v) in env { cmd.env(k, v); }
    cmd.output().expect("session-start should execute")
}

fn read_additional_context(out: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let v: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("stdout must be valid JSON: {}\nGot: {}", e, &stdout));
    v["hookSpecificOutput"]["additionalContext"].as_str().unwrap_or("").to_string()
}

/// AC #1 + #2 + #6 (rendered): boot envelope contains the principle set
/// fetched from the API under "## Principles (live from graph)" with at
/// least one principle label visible.
#[test]
fn session_start_injects_principles_section_from_api() {
    let tmp = tempfile::tempdir().unwrap();
    let fixture = tmp.path().join("principles.json");
    let cache = tmp.path().join("principles-cache.json");
    fixture_three_principles(&fixture);

    let out = run_session_start("silas", &[
        ("CHORUS_PRINCIPLES_FIXTURE_FILE", fixture.to_str().unwrap()),
        ("CHORUS_PRINCIPLES_CACHE_FILE", cache.to_str().unwrap()),
    ]);
    assert!(out.status.success(), "session-start should succeed: {:?}", String::from_utf8_lossy(&out.stderr));

    let ctx = read_additional_context(&out);
    assert!(
        ctx.contains("## Principles (live from graph)"),
        "additionalContext must contain the principles section. Got tail: {}",
        &ctx[ctx.len().saturating_sub(800)..]
    );
    assert!(
        ctx.contains("Observe") && ctx.contains("hemenway-observe"),
        "principle labels + ids must be in the section"
    );
    assert!(
        ctx.contains("Stack functions") && ctx.contains("Use edge"),
        "all 3 fixture principles must render"
    );
}

/// AC #3: principle-set hash written to /tmp/session-start-<role>-principles.hash
/// after boot — sibling to #2311's protocol_core_hash, used for cross-role
/// drift detection.
#[test]
fn session_start_writes_principles_hash_file() {
    let tmp = tempfile::tempdir().unwrap();
    let fixture = tmp.path().join("principles.json");
    let cache = tmp.path().join("principles-cache.json");
    fixture_three_principles(&fixture);
    let role = "wren";
    let hash_file = format!("/tmp/session-start-{}-principles.hash", role);
    let _ = fs::remove_file(&hash_file);

    let out = run_session_start(role, &[
        ("CHORUS_PRINCIPLES_FIXTURE_FILE", fixture.to_str().unwrap()),
        ("CHORUS_PRINCIPLES_CACHE_FILE", cache.to_str().unwrap()),
    ]);
    assert!(out.status.success());

    let hash = fs::read_to_string(&hash_file).expect("hash file must be written on boot");
    let hash = hash.trim();
    assert_eq!(hash.len(), 64, "must be a sha256 hex digest, got: {:?}", hash);
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()), "must be all hex");
}

/// AC #3 (drift detection shape): same principle set across roles produces
/// the same hash; differing sets produce different hashes.
#[test]
fn principles_hash_is_stable_across_roles_and_sensitive_to_set() {
    let tmp = tempfile::tempdir().unwrap();
    let fixture_a = tmp.path().join("a.json");
    let fixture_b = tmp.path().join("b.json");
    let cache = tmp.path().join("cache.json");
    fixture_three_principles(&fixture_a);
    fs::write(&fixture_b, r#"{"data":{"principles":[
        {"id": "hemenway-observe", "label": "Observe", "comment": "x", "techReading": "", "jeffReading": "", "order": 1}
    ]}}"#).unwrap();

    let out_silas_a = run_session_start("silas", &[
        ("CHORUS_PRINCIPLES_FIXTURE_FILE", fixture_a.to_str().unwrap()),
        ("CHORUS_PRINCIPLES_CACHE_FILE", cache.to_str().unwrap()),
    ]);
    assert!(out_silas_a.status.success());
    let h_silas_a = fs::read_to_string("/tmp/session-start-silas-principles.hash").unwrap().trim().to_string();

    let out_kade_a = run_session_start("kade", &[
        ("CHORUS_PRINCIPLES_FIXTURE_FILE", fixture_a.to_str().unwrap()),
        ("CHORUS_PRINCIPLES_CACHE_FILE", cache.to_str().unwrap()),
    ]);
    assert!(out_kade_a.status.success());
    let h_kade_a = fs::read_to_string("/tmp/session-start-kade-principles.hash").unwrap().trim().to_string();

    let out_silas_b = run_session_start("silas", &[
        ("CHORUS_PRINCIPLES_FIXTURE_FILE", fixture_b.to_str().unwrap()),
        ("CHORUS_PRINCIPLES_CACHE_FILE", cache.to_str().unwrap()),
    ]);
    assert!(out_silas_b.status.success());
    let h_silas_b = fs::read_to_string("/tmp/session-start-silas-principles.hash").unwrap().trim().to_string();

    assert_eq!(h_silas_a, h_kade_a, "same principle set => same hash across roles");
    assert_ne!(h_silas_a, h_silas_b, "different principle set => different hash");
}

/// AC #4: API unavailable (no fixture, bad URL, no cache) → graceful degraded
/// banner in content, session boots successfully (does NOT exit non-zero).
#[test]
fn session_start_handles_api_unavailable_gracefully() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = tmp.path().join("nonexistent-cache.json");

    let out = run_session_start("silas", &[
        // Point MCP base at a port nothing is listening on (#2477 — was HTTP API URL pre-MCP migration).
        ("CHORUS_MCP_BASE_URL", "http://127.0.0.1:1"),
        ("CHORUS_PRINCIPLES_CACHE_FILE", cache.to_str().unwrap()),
    ]);
    assert!(out.status.success(), "boot must succeed even when principles API fails");

    let ctx = read_additional_context(&out);
    assert!(
        ctx.contains("## Principles (live from graph)"),
        "section header should still render so the missing principles surface visibly"
    );
    assert!(
        ctx.to_lowercase().contains("unreachable") || ctx.to_lowercase().contains("unavailable"),
        "must include unreachable/unavailable marker"
    );
}

/// AC #4 (cache fallback): API unavailable but cache present → render
/// principles from cache with a STALE marker.
#[test]
fn session_start_falls_back_to_cache_with_stale_marker() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = tmp.path().join("principles-cache.json");
    // Pre-seed the cache with three principles
    fixture_three_principles(&cache);

    let out = run_session_start("silas", &[
        ("CHORUS_MCP_BASE_URL", "http://127.0.0.1:1"),
        ("CHORUS_PRINCIPLES_CACHE_FILE", cache.to_str().unwrap()),
    ]);
    assert!(out.status.success());

    let ctx = read_additional_context(&out);
    assert!(ctx.contains("## Principles (live from graph)"));
    assert!(ctx.to_uppercase().contains("STALE"), "must mark cached fallback as STALE");
    assert!(ctx.contains("Observe"), "cached principles must render in section");
}

/// AC #4 (malformed-200 path, kade #2450 review): API returns HTTP 200 with
/// unparseable body → falls back to cache same as a connection failure.
/// Without this, the symptom is "200 OK in logs, no principles in role" —
/// the least-diagnostic failure mode.
#[test]
fn session_start_handles_malformed_json_response_via_cache() {
    let tmp = tempfile::tempdir().unwrap();
    let fixture = tmp.path().join("malformed.json");
    let cache = tmp.path().join("cache.json");
    fs::write(&fixture, "not json {{{ broken").unwrap();
    fixture_three_principles(&cache);

    let out = run_session_start("silas", &[
        ("CHORUS_PRINCIPLES_FIXTURE_FILE", fixture.to_str().unwrap()),
        ("CHORUS_PRINCIPLES_CACHE_FILE", cache.to_str().unwrap()),
    ]);
    assert!(out.status.success(), "boot must succeed on malformed-200");

    let ctx = read_additional_context(&out);
    assert!(ctx.contains("## Principles (live from graph)"));
    // Same surface as connection failure: STALE cache fallback OR unavailable banner.
    let has_stale = ctx.to_uppercase().contains("STALE");
    let has_unavailable = ctx.to_lowercase().contains("unreachable")
        || ctx.to_lowercase().contains("unavailable");
    assert!(
        has_stale || has_unavailable,
        "malformed-200 must surface either STALE-cache or unreachable marker — \
         silent 200-with-empty-section is the failure mode this guards against"
    );
}

/// AC #5: API responds with empty set → loud banner in content, boot
/// continues (degraded), spine event recorded. Does NOT exit non-zero.
#[test]
fn session_start_fail_loud_on_empty_principles() {
    let tmp = tempfile::tempdir().unwrap();
    let fixture = tmp.path().join("empty.json");
    let cache = tmp.path().join("cache.json");
    fixture_empty(&fixture);

    let out = run_session_start("silas", &[
        ("CHORUS_PRINCIPLES_FIXTURE_FILE", fixture.to_str().unwrap()),
        ("CHORUS_PRINCIPLES_CACHE_FILE", cache.to_str().unwrap()),
    ]);
    assert!(out.status.success(), "boot must continue (alarm not crash)");

    let ctx = read_additional_context(&out);
    assert!(ctx.contains("## Principles (live from graph)"));
    let upper = ctx.to_uppercase();
    assert!(
        upper.contains("ALARM") || upper.contains("EMPTY") || upper.contains("BROKEN"),
        "empty-set must surface a visible loud-fail alarm in content"
    );
}
