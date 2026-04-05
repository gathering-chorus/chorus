//! Context injection hook (#1838)
//!
//! UserPromptSubmit: when Jeff states a problem or asks a question,
//! automatically search Chorus + memory + git and inject the synthesis
//! into the role's context via stderr. Zero discretion — the role
//! doesn't choose whether to search. The system does it for them.
//!
//! This replaces the "search and throw away results" failure mode.
//! The role sees accumulated context before they start thinking.

use crate::state::{AppState, ContextSearchResults};
use crate::types::{HookInput, HookResponse};
use tracing::info;

/// Stop words — don't search for these alone
const STOP_WORDS: &[&str] = &[
    "the", "is", "it", "a", "an", "and", "or", "but", "in", "on", "at", "to",
    "for", "of", "with", "by", "from", "not", "no", "do", "does", "did",
    "are", "was", "were", "be", "been", "being", "have", "has", "had",
    "this", "that", "these", "those", "my", "your", "our", "its",
    "i", "me", "we", "you", "he", "she", "they", "them",
    "what", "how", "why", "when", "where", "who", "which",
    "can", "could", "will", "would", "should", "shall",
    "just", "now", "still", "also", "too", "very", "really",
    "help", "find", "figure", "out", "show", "tell", "let",
    "look", "make", "get", "see", "try",
];

/// Extract meaningful keywords from Jeff's message
fn extract_keywords(prompt: &str) -> Vec<String> {
    let lower = prompt.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_' && c != '#')
        .filter(|w| !w.is_empty())
        .collect();

    let mut keywords: Vec<String> = words
        .iter()
        .filter(|w| w.len() > 1 && !STOP_WORDS.contains(&w.as_ref()))
        .map(|w| w.to_string())
        .collect();

    keywords.dedup();
    keywords.truncate(6); // Max 6 keywords
    keywords
}

/// Query Chorus API hybrid search — FTS + semantic + SPARQL via RRF (#2003)
/// Replaces direct SQLite FTS with API call for richer context (233ms measured)
fn query_chorus_hybrid(query: &str) -> Vec<(String, String, String)> {
    let encoded: String = query
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c.to_string()
            } else {
                format!("%{:02X}", c as u32)
            }
        })
        .collect();

    let url = format!(
        "http://localhost:3340/api/chorus/search?q={}&mode=hybrid&limit=5",
        encoded
    );

    let resp = match ureq::get(&url)
        .timeout(std::time::Duration::from_millis(500))
        .call()
    {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    let body: serde_json::Value = match resp.into_json() {
        Ok(v) => v,
        Err(_) => return vec![],
    };

    let mut results = Vec::new();
    if let Some(items) = body.get("results").and_then(|r| r.as_array()) {
        for item in items.iter().take(5) {
            let role = item.get("role").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let content = item.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let ts = item.get("timestamp").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if !content.is_empty() {
                results.push((role, content, ts));
            }
        }
    }

    results
}

/// Scan memory files for related decisions and feedback
fn scan_memory(keywords: &[String]) -> Vec<String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/jeffbridwell".to_string());
    let memory_dir = format!(
        "{}/.claude/projects/-Users-jeffbridwell-CascadeProjects/memory",
        home
    );

    let mut hits = Vec::new();
    let dir = match std::fs::read_dir(&memory_dir) {
        Ok(d) => d,
        Err(_) => return hits,
    };

    for entry in dir.flatten() {
        let path = entry.path();
        if !path.extension().map_or(false, |e| e == "md") {
            continue;
        }
        if path.file_name().map_or(false, |n| n == "MEMORY.md") {
            continue;
        }

        if let Ok(content) = std::fs::read_to_string(&path) {
            let lower = content.to_lowercase();
            let match_count = keywords.iter().filter(|k| lower.contains(k.as_str())).count();
            if match_count >= 2 || (keywords.len() == 1 && match_count == 1) {
                // Extract the first meaningful line after frontmatter
                let body = content
                    .split("---")
                    .nth(2)
                    .unwrap_or(&content)
                    .trim();
                let first_line: String = body.lines().next().unwrap_or("").chars().take(150).collect();
                if !first_line.is_empty() {
                    let fname = path.file_name().unwrap_or_default().to_string_lossy();
                    hits.push(format!("[{}] {}", fname, first_line));
                }
            }
        }
    }

    hits.truncate(5);
    hits
}

/// Main hook: extract keywords, search, synthesize, inject
/// Stores results in AppState so other hooks can read them (#2225)
pub async fn check(input: &HookInput, state: &AppState) -> HookResponse {
    let prompt = input.prompt.as_deref().unwrap_or("");

    // Skip very short messages (continuations, "yes", "no", etc.)
    if prompt.len() < 10 {
        return HookResponse::allow();
    }

    let keywords = extract_keywords(prompt);
    if keywords.is_empty() {
        return HookResponse::allow();
    }

    let role_name = format!("{:?}", input.role()).to_lowercase();
    let query = keywords.join(" ");

    // Search Chorus API — hybrid mode (FTS + semantic + SPARQL)
    let chorus_results = query_chorus_hybrid(&query);

    // Scan memory files
    let memory_hits = scan_memory(&keywords);

    // If nothing found, don't inject noise
    if chorus_results.is_empty() && memory_hits.is_empty() {
        info!(
            gate = "context-inject",
            event = "no-results",
            role = %role_name,
            query = %query,
        );
        return HookResponse::allow();
    }

    // Build the context block
    let mut context = String::from("\n<context-synthesis>\n");
    context.push_str(&format!("Keywords: {}\n", query));

    if !chorus_results.is_empty() {
        context.push_str(&format!("\nChorus hybrid ({} hits):\n", chorus_results.len()));
        for (role, content, ts) in &chorus_results {
            let short: String = content.replace('\n', " ").chars().take(200).collect();
            let ts_short: String = ts.chars().take(16).collect();
            context.push_str(&format!("  [{}] {} — {}\n", ts_short, role, short));
        }
    }

    if !memory_hits.is_empty() {
        context.push_str(&format!("\nMemory ({} hits):\n", memory_hits.len()));
        for hit in &memory_hits {
            context.push_str(&format!("  {}\n", hit));
        }
    }

    context.push_str("\nMANDATORY: You MUST reference this context before responding. Do not search filesystem or git for information already provided here. If Chorus returned results, cite them. Ignoring injected context is a protocol violation.\n");
    context.push_str("</context-synthesis>");

    // Store results in AppState for other hooks to read (#2225)
    let session_id = input.session_id.as_deref().unwrap_or("unknown");
    state.store_context_results(session_id, ContextSearchResults {
        chorus_hits: chorus_results.clone(),
        memory_hits: memory_hits.clone(),
        query: query.clone(),
        stored_at: chrono::Utc::now().timestamp(),
    }).await;

    info!(
        gate = "context-inject",
        event = "injected",
        role = %role_name,
        query = %query,
        chorus_hits = chorus_results.len(),
        memory_hits = memory_hits.len(),
    );

    HookResponse::warn_stderr(&context)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_meaningful_keywords() {
        let kw = extract_keywords("seeds is broken help me find root cause");
        assert!(kw.contains(&"seeds".to_string()));
        assert!(kw.contains(&"broken".to_string()));
        assert!(kw.contains(&"root".to_string()));
        assert!(kw.contains(&"cause".to_string()));
        assert!(!kw.contains(&"is".to_string()));
        assert!(!kw.contains(&"me".to_string()));
    }

    #[test]
    fn skips_stop_words_only() {
        let kw = extract_keywords("is it the");
        assert!(kw.is_empty());
    }

    #[test]
    fn extracts_from_problem_statement() {
        let kw = extract_keywords("clearing tiles dont show card numbers");
        assert!(kw.contains(&"clearing".to_string()));
        assert!(kw.contains(&"tiles".to_string()));
        assert!(kw.contains(&"card".to_string()));
        assert!(kw.contains(&"numbers".to_string()));
    }

    #[test]
    fn limits_to_six_keywords() {
        let kw = extract_keywords("seeds pipeline broken webhook twilio adapter fuseki persistence routing defaults correlation");
        assert!(kw.len() <= 6);
    }

    #[test]
    fn keeps_action_verbs_fix_and_check() {
        let kw = extract_keywords("fix the seed pipeline");
        assert!(kw.contains(&"fix".to_string()), "fix should be kept");
        assert!(kw.contains(&"seed".to_string()));
        assert!(kw.contains(&"pipeline".to_string()));

        let kw2 = extract_keywords("check if nudges are working");
        assert!(kw2.contains(&"check".to_string()), "check should be kept");
        assert!(kw2.contains(&"nudges".to_string()));
        assert!(kw2.contains(&"working".to_string()));
    }
}
