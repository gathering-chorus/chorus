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

#[test]
fn prompt_drives_query_with_no_card() {
    // no WIP card → query is the prompt, NO &domain= clause
    let url = build_search_url("seeds pipeline broken", None);
    assert!(url.contains("q=seeds%20pipeline%20broken"), "prompt is the query: {url}");
    assert!(url.contains("mode=hybrid"), "hybrid mode preserved: {url}");
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
