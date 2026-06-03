//! #3134 — card-as-tag on the per-prompt context search.
//!
//! The per-prompt search is driven by what Jeff typed (the prompt keywords ARE
//! the query). When the role has a WIP card, its domain is layered on as an
//! optional `&domain=` tag so results are scoped to the card's area. With no
//! card — the common case (venting, ideating, "whatever") — the search runs on
//! the prompt alone, with no board/domain clause driving it.
//!
//! AC: prompt drives the query; card domain is an optional tag; zero-card works.

use chorus_hooks::build_search_url;
use chorus_hooks::format_spine_line;

#[test]
fn spine_line_is_valid_json_with_outcome_fields() {
    // #3134 observability: the per-prompt outcome must land on the spine as a
    // parseable JSONL event with the measurable fields (hits + injected bytes).
    let line = format_spine_line("2026-05-30T10:00:00.000+0000", "wren", "context.inject.injected", 5, 3, 1, 2048, 240);
    // valid JSON
    let v: serde_json::Value = serde_json::from_str(&line).expect("spine line must be valid JSON");
    assert_eq!(v["event"], "context.inject.injected");
    assert_eq!(v["role"], "wren");
    assert_eq!(v["chorus_hits"], 5);
    assert_eq!(v["injected_bytes"], 2048);
    assert_eq!(v["component"], "context-inject");
    assert_eq!(v["appName"], "chorus-events"); // so promtail/Loki ingest it
}

#[test]
fn prompt_drives_query_with_no_card() {
    // no WIP card → query is the prompt, NO &domain= clause
    let url = build_search_url("seeds pipeline broken", None);
    assert!(url.contains("q=seeds%20pipeline%20broken"), "prompt is the query: {url}");
    // build_search_url is the FTS/authority leg: mode=relevance since #3171 (was hybrid in
    // #3134; this assertion was left stale by #3171). The semantic leg is build_semantic_url (#3191).
    assert!(url.contains("mode=relevance"), "FTS leg uses relevance (#3171): {url}");
    assert!(!url.contains("&domain="), "no card → no domain tag: {url}");
}

#[test]
fn card_domain_is_appended_as_optional_tag() {
    // WIP card present → domain tag sharpens, but the PROMPT is still the query
    let url = build_search_url("nudge delivery", Some("chorus"));
    assert!(url.contains("q=nudge%20delivery"), "prompt is still the query: {url}");
    assert!(url.contains("&domain=chorus"), "card domain appended as tag: {url}");
}

#[test]
fn blank_domain_degrades_to_no_tag() {
    // an empty / whitespace domain must not emit a dangling &domain=
    assert!(!build_search_url("x ray", Some("")).contains("&domain="));
    assert!(!build_search_url("x ray", Some("   ")).contains("&domain="));
}
