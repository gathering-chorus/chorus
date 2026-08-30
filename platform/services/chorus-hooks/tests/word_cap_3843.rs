//! #3843 — word-cap Stop-hook leg.
//!
//! Two suites. (1) The SHARED FIXTURE: config/word-cap-fixtures.json is the
//! cross-language contract with #3818's TS counter — every case must produce
//! the identical count here, or Jeff gets bounced on one surface and not the
//! other for the same text. The fixture file is the agreement; a missing file
//! is a loud failure, never a skip (#3734 — a guard whose target is deleted
//! must fail loudly). (2) The RESOLVER: every failure path fails OPEN to 100
//! — the negative proofs that a dead store or a fat-fingered property value
//! cannot mute the team.
//!
//! Hermetic: the resolver tests bring their own TCP stub; nothing here talks
//! to a live chorus-api or Fuseki.

use chorus_hooks::word_cap::{check_cap, count_words, resolve_cap, FAIL_OPEN_CAP};
use std::io::{Read, Write};
use std::net::TcpListener;

fn fixtures_path() -> std::path::PathBuf {
    // CARGO_MANIFEST_DIR = platform/services/chorus-hooks → repo root is ../../..
    // WORD_CAP_FIXTURES overrides for pre-land development against the #3818
    // werk copy; the default is the committed repo file.
    if let Ok(p) = std::env::var("WORD_CAP_FIXTURES") {
        return p.into();
    }
    std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../../../config/word-cap-fixtures.json")
}

/// AC1 + AC4 — the Rust counter agrees with the shared fixture on every case.
#[test]
fn counter_matches_every_shared_fixture_case() {
    let path = fixtures_path();
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "shared fixture missing at {} ({e}) — the cross-language contract \
             with #3818 is gone, and a gate whose target is missing fails \
             loudly, never vacuously (#3734)",
            path.display()
        )
    });
    let doc: serde_json::Value = serde_json::from_str(&raw).expect("fixture is valid JSON");
    let cases = doc["cases"].as_array().expect("fixture has a cases array");
    assert!(cases.len() >= 17, "fixture shrank below the agreed case count");
    let mut failures = Vec::new();
    for case in cases {
        let name = case["name"].as_str().expect("case has a name");
        let text = case["text"].as_str().expect("case has text");
        let expect = case["expect"].as_u64().expect("case has expect") as usize;
        let got = count_words(text);
        if got != expect {
            failures.push(format!("  {name}: expected {expect}, got {got}"));
        }
    }
    assert!(
        failures.is_empty(),
        "Rust counter disagrees with the shared fixture — the TS leg and this \
         leg would bounce different surfaces for the same text:\n{}",
        failures.join("\n")
    );
}

/// Negative proof for the gate itself (#3734): an over-cap text IS refused —
/// the check can go red, with the true count in the verdict.
#[test]
fn over_cap_text_is_refused_with_the_count() {
    let text = "one two three four five six";
    let over = check_cap(text, 5).expect_err("6 words against a cap of 5 must refuse");
    assert_eq!(over.words, 6);
    assert_eq!(over.cap, 5);
    assert!(check_cap(text, 6).is_ok(), "at-cap passes; the boundary is <=");
}

/// One-shot stub server: accepts a single connection, returns `response`.
fn stub_server(response: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        if let Ok((mut sock, _)) = listener.accept() {
            let mut buf = [0u8; 2048];
            let _ = sock.read(&mut buf);
            let _ = sock.write_all(response.as_bytes());
        }
    });
    format!("http://{addr}")
}

fn json_ok(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// AC3 negative proofs — every resolver failure path fails OPEN to 100.
#[test]
fn resolver_unreachable_fails_open() {
    // A bound-then-dropped listener guarantees a dead port.
    let dead = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}", l.local_addr().unwrap())
    };
    assert_eq!(resolve_cap("unreachable-role", &dead), FAIL_OPEN_CAP);
}

#[test]
fn resolver_non_ok_status_fails_open() {
    let base = stub_server("HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    assert_eq!(resolve_cap("non-ok-role", &base), FAIL_OPEN_CAP);
}

#[test]
fn resolver_malformed_body_fails_open() {
    let resp = Box::leak(json_ok("this is not json").into_boxed_str());
    let base = stub_server(resp);
    assert_eq!(resolve_cap("malformed-role", &base), FAIL_OPEN_CAP);
}

#[test]
fn resolver_missing_value_fails_open() {
    let resp = Box::leak(json_ok(r#"{"resolved":true}"#).into_boxed_str());
    let base = stub_server(resp);
    assert_eq!(resolve_cap("missing-value-role", &base), FAIL_OPEN_CAP);
}

/// A fat-fingered zero or negative cap must not be able to mute the team.
#[test]
fn resolver_non_positive_cap_is_refused_and_fails_open() {
    let base = stub_server(Box::leak(json_ok(r#"{"value":0}"#).into_boxed_str()));
    assert_eq!(resolve_cap("zero-cap-role", &base), FAIL_OPEN_CAP);
    let base = stub_server(Box::leak(json_ok(r#"{"value":-5}"#).into_boxed_str()));
    assert_eq!(resolve_cap("negative-cap-role", &base), FAIL_OPEN_CAP);
}

/// Happy path + cache: a good value is honoured, and honoured again from the
/// TTL cache after the server is gone (one graph read per minute, not per turn).
#[test]
fn resolver_reads_value_and_caches_it() {
    let base = stub_server(Box::leak(json_ok(r#"{"value":150}"#).into_boxed_str()));
    assert_eq!(resolve_cap("cachetest", &base), 150);
    // The stub accepted its single connection; a second resolve inside the TTL
    // must come from the cache — a live call now would fail open to 100.
    assert_eq!(resolve_cap("cachetest", &base), 150);
}

/// A failure is NOT cached: after a fail-open, a working store is read again
/// immediately (mirrors the TS leg — only success writes the cache).
#[test]
fn resolver_does_not_cache_the_fail_open() {
    let dead = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}", l.local_addr().unwrap())
    };
    assert_eq!(resolve_cap("failcache", &dead), FAIL_OPEN_CAP);
    let base = stub_server(Box::leak(json_ok(r#"{"value":42}"#).into_boxed_str()));
    assert_eq!(resolve_cap("failcache", &base), 42);
}

/// The gate can never wedge a turn (#3672's lesson, bounded in code): the
/// same over-cap text refuses WORD_CAP_REFUSAL_CAP times then degrades, and a
/// REWRITTEN reply is a fresh debt with fresh chances — while a clean stop
/// resets the counter entirely.
#[test]
fn refusals_are_bounded_and_a_rewrite_resets_the_debt() {
    use chorus_hooks::word_cap::{note_refusal, reset_refusals, WORD_CAP_REFUSAL_CAP};
    let role = "bounded-role";
    reset_refusals(role);
    assert_eq!(note_refusal(role, "debt-a"), 0, "first refusal");
    assert_eq!(note_refusal(role, "debt-a"), 1, "second refusal");
    assert!(
        note_refusal(role, "debt-a") >= WORD_CAP_REFUSAL_CAP,
        "third fire is past the cap — the turn may end (degrade, no fire loop)"
    );
    assert_eq!(note_refusal(role, "debt-b"), 0, "a rewritten reply is a fresh debt");
    reset_refusals(role);
    assert_eq!(note_refusal(role, "debt-b"), 0, "a clean stop resets everything");
}
