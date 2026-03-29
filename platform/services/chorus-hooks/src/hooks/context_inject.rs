//! Context injection hook (#1838)
//!
//! UserPromptSubmit: when Jeff states a problem or asks a question,
//! automatically search Chorus + memory + git and inject the synthesis
//! into the role's context via stderr. Zero discretion — the role
//! doesn't choose whether to search. The system does it for them.
//!
//! This replaces the "search and throw away results" failure mode.
//! The role sees accumulated context before they start thinking.

use crate::types::{HookInput, HookResponse};
use rusqlite::Connection;
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
    "fix", "check", "look", "make", "get", "see", "try",
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

/// Query Chorus SQLite FTS — reuses the pattern from search_hierarchy.rs
fn query_chorus_fts(db_path: &std::path::Path, query: &str) -> Vec<(String, String, String)> {
    let conn = match Connection::open(db_path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let _ = conn.execute_batch("PRAGMA query_timeout = 500;");

    let fts_query = query.replace('-', " ");
    let mut results = Vec::new();

    if let Ok(mut stmt) = conn.prepare(
        "SELECT m.role, m.content, m.timestamp
         FROM messages_fts f
         JOIN messages m ON f.rowid = m.id
         WHERE messages_fts MATCH ?1
         ORDER BY m.timestamp DESC
         LIMIT 5",
    ) {
        if let Ok(rows) = stmt.query_map([&fts_query], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        }) {
            for row in rows.flatten() {
                results.push(row);
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
pub async fn check(input: &HookInput) -> HookResponse {
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

    // Search Chorus FTS
    let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/jeffbridwell".to_string());
    let db_path = std::path::PathBuf::from(&home).join(".chorus/index.db");
    let chorus_results = query_chorus_fts(&db_path, &query);

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
        context.push_str(&format!("\nChorus ({} hits):\n", chorus_results.len()));
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

    context.push_str("\nUse this context. Don't search for what's already here.\n");
    context.push_str("</context-synthesis>");

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
}
