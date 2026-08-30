//! @test-type: unit — signal is fixture-data: pure hasher, no io beyond the fixture read
//! #3864 — reply-delivery correlation, Rust leg.
//!
//! The SHARED FIXTURE suite: config/reply-hash-fixtures.json is the
//! cross-language contract with Clearing's TS hasher — every case must
//! produce the identical hash here, or reply.emitted / reply.rendered pairs
//! never join and every reply reads as a delivery gap. A missing fixture
//! file is a loud failure, never a skip (#3734 — a guard whose target is
//! deleted must fail loudly).

use chorus_hooks::reply_delivery::content_hash;

fn fixtures_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("REPLY_HASH_FIXTURES") {
        return p.into();
    }
    std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../../../config/reply-hash-fixtures.json")
}

#[test]
fn rust_hash_agrees_with_shared_fixture_on_every_case() {
    let path = fixtures_path();
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "reply-hash fixture MISSING at {} ({e}) — the cross-language \
             contract is gone, not skippable",
            path.display()
        )
    });
    let v: serde_json::Value = serde_json::from_str(&raw).expect("fixture parses");
    let cases = v["cases"].as_array().expect("cases array");
    assert!(!cases.is_empty(), "fixture has zero cases — vacuous contract");
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let text = c["text"].as_str().unwrap();
        let want = c["hash"].as_str().unwrap();
        let got = content_hash(text);
        assert_eq!(
            got, want,
            "case '{name}': Rust hash {got} != fixture {want} — the TS leg \
             will compute {want} and the pair will never join"
        );
    }
}

/// NEGATIVE PROOF (#3734): the hash must actually distinguish different
/// replies — two distinct texts hashing alike would silently correlate the
/// wrong pair and hide a real gap.
#[test]
fn different_texts_hash_differently() {
    assert_ne!(content_hash("reply one"), content_hash("reply two"));
}

/// The whole reason canonicalization exists: this side keeps the chorus
/// header and joins blocks with '\n'; the tailer strips it and joins with
/// spaces. Same reply, different bytes, ONE hash.
#[test]
fn header_and_join_style_differences_canonicalize_together() {
    let rust_view = "--- Silas | 2026-08-13 | #3864 | Werk v1.6.0 ---\nline one\nline two";
    let tailer_view = "line one line two";
    assert_eq!(content_hash(rust_view), content_hash(tailer_view));
}
