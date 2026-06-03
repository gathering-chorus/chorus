//! #3191 AC6/AC7/AC8 — context-inject RELEVANCE half.
//!
//! Bug: `extract_keywords` ranks survivors by WORD LENGTH and `search_query` ANDs the
//! top-2, so a few-sentence prompt collapses to its two longest words ANDed — dropping
//! the actually-relevant terms. Proof: "...scarecrow from wizard of oz" → q=`scarecrow
//! talked` → `[]` while the scarecrow story sat in the index.
//!
//! Verified live (`:3340`): FTS (`mode=relevance`) ANDs tokens — a full prompt zeroes out
//! (heidegger full sentence → 0) and short terms drop; the SEMANTIC leg (lance vector,
//! `mode=semantic`) takes the FULL PROMPT, never ANDs, and surfaces the meaning-match.
//! FTS and semantic want DIFFERENT query forms, so the inject queries BOTH and merges:
//! authority via FTS+keywords (#3171 preserved), meaning via semantic+full-prompt.
//!
//! These tests pin QUERY CONSTRUCTION (AC8) and the MERGE (AC6/AC7) as pure functions.
//! RED before build_semantic_url / merge_candidates exist.

use chorus_hooks::{build_semantic_url, merge_candidates};

// AC8 — the semantic leg queries the FULL PROMPT TEXT, not two length-ranked ANDed words.
#[test]
fn semantic_leg_queries_full_prompt_not_top2_keywords() {
    let prompt = "have i ever talked about the scarecrow from wizard of oz";
    let url = build_semantic_url(prompt, None);
    assert!(url.contains("mode=semantic"), "semantic leg must use mode=semantic; got: {url}");
    assert!(url.contains("wizard"), "full prompt must reach the query (wizard dropped); got: {url}");
    assert!(url.contains("scarecrow"), "subject must survive; got: {url}");
    let q = url.split("q=").nth(1).unwrap_or("").split('&').next().unwrap_or("");
    assert!(q.contains("oz"), "short-but-relevant 'oz' must survive (length-keyword path dropped it); got q={q}");
    assert!(q.len() > "scarecrow%20talked".len(), "query must be the full prompt, not top-2 keywords; got q={q}");
}

// The semantic leg carries the WIP card domain tag the same way the FTS leg does (#3134).
#[test]
fn semantic_url_carries_domain_tag() {
    let url = build_semantic_url("the scarecrow", Some("chorus"));
    assert!(url.contains("&domain=chorus"), "semantic leg must carry the card domain tag; got: {url}");
}

// AC6/AC7 — FTS/authority leg + semantic/meaning leg merge: both reach the limited slots,
// content dupes collapse.
#[test]
fn merge_keeps_both_legs_and_dedups() {
    let fts = vec![
        ("wren".to_string(), "authority doc A".to_string(), "t1".to_string(), 0.9_f64),
        ("wren".to_string(), "shared B".to_string(), "t2".to_string(), 0.5),
    ];
    let sem = vec![
        ("jeff".to_string(), "shared B".to_string(), "t2".to_string(), 0.8_f64), // dup of fts[1]
        ("jeff".to_string(), "scarecrow story C".to_string(), "t3".to_string(), 0.7),
    ];
    let merged = merge_candidates(&fts, &sem, 5);
    let contents: Vec<&str> = merged.iter().map(|(_, c, _, _)| c.as_str()).collect();
    assert!(contents.contains(&"authority doc A"), "authority leg must survive; got {contents:?}");
    assert!(contents.contains(&"scarecrow story C"), "semantic story must survive; got {contents:?}");
    assert_eq!(contents.iter().filter(|c| **c == "shared B").count(), 1, "dup content must collapse; got {contents:?}");
}

// Under a tight limit the semantic leg must still get a slot (interleave).
#[test]
fn merge_interleaves_so_semantic_survives_limit() {
    let fts = vec![
        ("w".to_string(), "a".to_string(), String::new(), 0.0_f64),
        ("w".to_string(), "b".to_string(), String::new(), 0.0),
        ("w".to_string(), "c".to_string(), String::new(), 0.0),
    ];
    let sem = vec![("j".to_string(), "story".to_string(), String::new(), 0.0_f64)];
    let merged = merge_candidates(&fts, &sem, 2);
    let contents: Vec<&str> = merged.iter().map(|(_, c, _, _)| c.as_str()).collect();
    assert!(contents.contains(&"story"), "semantic must get a slot even at limit=2; got {contents:?}");
    assert_eq!(merged.len(), 2, "must honor the limit; got {contents:?}");
}
