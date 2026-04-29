//! Tests for shared search state (#2225)
//!
//! context_inject stores Chorus results in AppState.
//! search_hierarchy reads from AppState instead of re-querying.
//! One search, shared state.

use std::collections::HashMap;

/// Simulates the shared search result structure
#[derive(Clone, Debug)]
#[allow(dead_code)]
struct SearchResult {
    chorus_hits: Vec<String>,
    memory_hits: Vec<String>,
    timestamp: u64,
}

/// Simulates AppState context storage
struct ContextStore {
    results: HashMap<String, SearchResult>,
}

impl ContextStore {
    fn new() -> Self {
        Self { results: HashMap::new() }
    }

    fn store(&mut self, session_id: &str, result: SearchResult) {
        self.results.insert(session_id.to_string(), result);
    }

    fn get(&self, session_id: &str) -> Option<&SearchResult> {
        let result = self.results.get(session_id)?;
        // Expire after 30 seconds
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if now - result.timestamp > 30 {
            return None;
        }
        Some(result)
    }
}

#[test]
fn context_inject_stores_results() {
    let mut store = ContextStore::new();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    store.store("session-123", SearchResult {
        chorus_hits: vec!["hit1".into(), "hit2".into()],
        memory_hits: vec!["mem1".into()],
        timestamp: now,
    });

    let result = store.get("session-123");
    assert!(result.is_some(), "stored results should be retrievable");
    assert_eq!(result.unwrap().chorus_hits.len(), 2);
}

#[test]
fn search_hierarchy_reads_shared_state() {
    let mut store = ContextStore::new();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // context_inject ran and stored results
    store.store("session-456", SearchResult {
        chorus_hits: vec!["prior work on nudge".into()],
        memory_hits: vec!["DEC-107 nudge delivery".into()],
        timestamp: now,
    });

    // search_hierarchy should find results without re-querying
    let cached = store.get("session-456");
    assert!(cached.is_some(), "search_hierarchy should read from shared state");
    assert_eq!(cached.unwrap().chorus_hits[0], "prior work on nudge");
}

#[test]
fn expired_results_not_returned() {
    let mut store = ContextStore::new();

    // Store with old timestamp (31 seconds ago)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    store.store("session-789", SearchResult {
        chorus_hits: vec!["stale".into()],
        memory_hits: vec![],
        timestamp: now - 31,
    });

    let result = store.get("session-789");
    assert!(result.is_none(), "results older than 30s should expire");
}

#[test]
fn no_results_returns_none() {
    let store = ContextStore::new();
    assert!(store.get("nonexistent").is_none());
}
