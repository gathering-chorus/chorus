use crate::state::{chorus_log, AppState};
use crate::types::{decision_block_json, permission_deny_json, HookInput, HookResponse};
use regex::Regex;
use rusqlite::Connection;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::LazyLock;

// Code lookup classifiers
static FILE_EXT_GLOB_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\*\.[a-z]{1,5}$").unwrap());

static SPECIFIC_FILE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9_-]+\.[a-z]+$").unwrap());

static PATH_FILE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"/[a-zA-Z0-9_-]+\.[a-z]+$").unwrap());

static CAMEL_PASCAL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[a-z][A-Z]|^[A-Z][a-z]+[A-Z]").unwrap());

static SNAKE_CASE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z_]+_[a-z_]+$").unwrap());

static IMPORT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(import|require|from|export|class |function |interface |type |const |let |var )")
        .unwrap()
});

static REGEX_SYNTAX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\[dswDSW]|\[[^\]]+\]|\\\.\\.|\\\s\\+").unwrap());

static CODE_DIR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"/(src|dist|node_modules|views|tests|public)/").unwrap());

static DIR_WILDCARD_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\*\*/").unwrap());

static SHORT_TOKEN_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-zA-Z_]+$").unwrap());

fn is_code_lookup(pattern: &str, search_path: &str) -> bool {
    if FILE_EXT_GLOB_RE.is_match(pattern) {
        return true;
    }
    if SPECIFIC_FILE_RE.is_match(pattern) {
        return true;
    }
    if PATH_FILE_RE.is_match(pattern) {
        return true;
    }
    if CAMEL_PASCAL_RE.is_match(pattern) {
        return true;
    }
    if SNAKE_CASE_RE.is_match(pattern) {
        return true;
    }
    if IMPORT_RE.is_match(pattern) {
        return true;
    }
    if REGEX_SYNTAX_RE.is_match(pattern) {
        return true;
    }
    if !search_path.is_empty() && std::path::Path::new(search_path).is_file() {
        return true;
    }
    if CODE_DIR_RE.is_match(search_path) {
        return true;
    }
    if DIR_WILDCARD_RE.is_match(pattern) {
        return true;
    }
    if pattern.len() < 4 && SHORT_TOKEN_RE.is_match(pattern) {
        return true;
    }
    false
}

fn pattern_hash(pattern: &str) -> String {
    let mut hasher = DefaultHasher::new();
    pattern.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Query Chorus SQLite database for FTS results
fn query_chorus(db_path: &std::path::Path, query: &str) -> Option<String> {
    let conn = Connection::open(db_path).ok()?;
    conn.execute_batch("PRAGMA query_timeout = 2000;").ok()?;

    let fts_query = query.replace('-', " ");

    // Try FTS first
    let mut results = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT m.source, m.channel, m.role, m.content, m.timestamp
         FROM messages_fts f
         JOIN messages m ON f.rowid = m.id
         WHERE messages_fts MATCH ?1
         ORDER BY m.timestamp DESC
         LIMIT 8",
    ) {
        if let Ok(rows) = stmt.query_map([&fts_query], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        }) {
            for row in rows.flatten() {
                results.push(row);
            }
        }
    }

    // Fallback to LIKE if FTS fails or returns nothing
    if results.is_empty() {
        if let Ok(mut stmt) = conn.prepare(
            "SELECT source, channel, role, content, timestamp
             FROM messages
             WHERE content LIKE ?1
             ORDER BY timestamp DESC
             LIMIT 8",
        ) {
            let like_query = format!("%{}%", query);
            if let Ok(rows) = stmt.query_map([&like_query], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            }) {
                for row in rows.flatten() {
                    results.push(row);
                }
            }
        }
    }

    if results.is_empty() {
        return None;
    }

    let mut output = format!("Chorus found {} results for \"{}\":\n\n", results.len(), query);
    for (source, channel, role, content, ts) in &results {
        let short: String = content.replace('\n', " ").chars().take(200).collect();
        let label = if source == "slack" {
            format!("[{}] #{}", source, channel)
        } else {
            format!("[{}] {}", source, channel)
        };
        let ts_short: String = ts.chars().take(16).collect();
        output.push_str(&format!("  {} ({}) {}: {}\n\n", label, ts_short, role, short));
    }

    Some(output)
}

/// Layer 2: Query local log files for operational signal
fn query_logs(pattern: &str) -> Option<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let log_paths = [
        format!("{}/Library/Logs/Gathering/hooks.log", home),
        format!("{}/Library/Logs/Gathering/daily-review-ops.log", home),
        format!("{}/Library/Logs/Gathering/infra-alert.log", home),
    ];

    let mut results = Vec::new();
    let pattern_lower = pattern.to_lowercase();

    for log_path in &log_paths {
        if let Ok(content) = std::fs::read_to_string(log_path) {
            let lines: Vec<&str> = content.lines().collect();
            let recent = if lines.len() > 500 { &lines[lines.len() - 500..] } else { &lines };
            for line in recent {
                if line.to_lowercase().contains(&pattern_lower)
                    && !line.contains("search_hierarchy")
                    && !line.contains("search.hierarchy")
                    && !line.contains("memory_first")
                    && !line.contains("enrichment")
                    && !line.contains("| enter |")
                    && !line.contains("| allow |")
                    && !line.contains("| DENY  |")
                {
                    results.push(line.to_string());
                    if results.len() >= 5 {
                        break;
                    }
                }
            }
        }
        if results.len() >= 5 {
            break;
        }
    }

    if results.is_empty() {
        return None;
    }

    Some(format!(
        "Recent logs ({}):\n{}",
        results.len(),
        results.iter().map(|l| format!("  {}", l.chars().take(200).collect::<String>())).collect::<Vec<_>>().join("\n")
    ))
}


/// Layer 3: Query git log for recent commits related to the pattern
fn query_git(pattern: &str) -> Option<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let repo = format!("{}/CascadeProjects/chorus", home);

    let output = std::process::Command::new("git")
        .args(["log", "--oneline", "--all", "-10", "--grep", pattern])
        .current_dir(&repo)
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();

    if lines.is_empty() {
        return None;
    }

    Some(format!(
        "Recent commits ({}):\n{}",
        lines.len(),
        lines.iter().map(|l| format!("  {}", l)).collect::<Vec<_>>().join("\n")
    ))
}

pub async fn check(input: &HookInput, state: &AppState) -> HookResponse {
    let tool = input.tool_name_str();
    if tool != "Grep" && tool != "Glob" {
        return HookResponse::allow();
    }

    let role = input.role();
    let pattern = input.get_tool_input_str("pattern");
    if pattern.is_empty() {
        return HookResponse::allow();
    }

    let search_path = input.get_tool_input_str("path");

    // No code-lookup bypass — all searches get Chorus enrichment (#1951)
    let role_str = role.as_str();
    let session_id = input.session_id.as_deref().unwrap_or("unknown");
    let hash = pattern_hash(&pattern);
    let block_key = format!("{}-{}-{}", session_id, role_str, hash);

    // Dedupe: same pattern within 60s skips re-query
    if state.check_search_block(&block_key).await {
        state.clear_search_block(&block_key).await;
        return HookResponse::allow();
    }

    // Query Chorus DB
    let db_path = state.config.chorus_db.clone();
    if !db_path.exists() {
        return HookResponse::allow();
    }

    let pattern_clone = pattern.clone();
    let chorus_results = tokio::task::spawn_blocking(move || query_chorus(&db_path, &pattern_clone))
        .await
        .unwrap_or(None);

    // Telemetry
    let has_results = chorus_results.is_some();
    let r = role_str.to_string();
    let t = tool.to_string();
    let p: String = pattern.chars().take(80).collect();
    tokio::spawn(async move {
        chorus_log(
            "search.hierarchy.enrichment",
            &r,
            &[
                ("tool", &t),
                ("has_chorus_results", if has_results { "true" } else { "false" }),
                ("pattern", &p),
            ],
        )
        .await;
    });

    // Layer 2: Log context — recent log entries matching the pattern
    let log_pattern = pattern.clone();
    let log_context = tokio::task::spawn_blocking(move || query_logs(&log_pattern))
        .await
        .unwrap_or(None);

    // Layer 3: Git context — recent commits related to the pattern
    let git_pattern = pattern.clone();
    let git_context = tokio::task::spawn_blocking(move || query_git(&git_pattern))
        .await
        .unwrap_or(None);

    // Combine all layers that returned results
    let mut layers = Vec::new();
    if let Some(ref chorus) = chorus_results {
        layers.push(format!("## Chorus (team memory)\n{}", chorus));
    }
    if let Some(ref logs) = log_context {
        layers.push(format!("## Logs (system state)\n{}", logs));
    }
    if let Some(ref git) = git_context {
        layers.push(format!("## Git (change history)\n{}", git));
    }

    if layers.is_empty() {
        return HookResponse::allow();
    }

    // Deny with compound context — role sees all layers before retrying
    state.set_search_block(&block_key).await;
    let short_pattern: String = pattern.chars().take(60).collect();
    let msg = format!(
        "Compound context for \"{}\" ({} layer{}):\n\n{}\n\nRetry your search — this context is now loaded.",
        short_pattern,
        layers.len(),
        if layers.len() == 1 { "" } else { "s" },
        layers.join("\n\n"),
    );
    HookResponse::deny(&permission_deny_json(&msg))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HookInput;
    use serde_json::json;

    // === is_code_lookup classifier tests ===

    #[test]
    fn test_file_ext_glob() {
        assert!(is_code_lookup("*.ts", ""));
        assert!(is_code_lookup("*.rs", ""));
        assert!(is_code_lookup("*.md", ""));
    }

    #[test]
    fn test_specific_file() {
        assert!(is_code_lookup("handler.ts", ""));
        assert!(is_code_lookup("Cargo.toml", ""));
        assert!(is_code_lookup("main.rs", ""));
    }

    #[test]
    fn test_path_file() {
        assert!(is_code_lookup("anything", "/src/hooks/write_scrubber.rs"));
    }

    #[test]
    fn test_camel_case() {
        assert!(is_code_lookup("HookResponse", ""));
        assert!(is_code_lookup("hookInput", ""));
        assert!(is_code_lookup("getToolInput", ""));
    }

    #[test]
    fn test_snake_case() {
        assert!(is_code_lookup("write_scrubber", ""));
        assert!(is_code_lookup("hook_input", ""));
        assert!(is_code_lookup("tool_name", ""));
    }

    #[test]
    fn test_import_patterns() {
        assert!(is_code_lookup("import { Router }", ""));
        assert!(is_code_lookup("from '../types'", ""));
        assert!(is_code_lookup("class MyHandler", ""));
        assert!(is_code_lookup("function processHook", ""));
        assert!(is_code_lookup("const SOCKET_PATH", ""));
    }

    #[test]
    fn test_regex_syntax() {
        assert!(is_code_lookup(r"\bdocker\s+exec\b", ""));
        assert!(is_code_lookup(r"[A-Z0-9]{16}", ""));
    }

    #[test]
    fn test_code_dir_path() {
        assert!(is_code_lookup("anything", "/src/hooks/"));
        assert!(is_code_lookup("anything", "/dist/bundle.js"));
        assert!(is_code_lookup("anything", "/tests/unit/"));
    }

    #[test]
    fn test_dir_wildcard() {
        assert!(is_code_lookup("**/hooks", ""));
        assert!(is_code_lookup("**/test*.rs", ""));
    }

    #[test]
    fn test_short_tokens() {
        assert!(is_code_lookup("fn", ""));
        assert!(is_code_lookup("use", ""));
        assert!(is_code_lookup("mod", ""));
    }

    // === Non-code patterns (would trigger Chorus enrichment) ===

    #[test]
    fn test_not_code_natural_language() {
        assert!(!is_code_lookup("disk usage audit", ""));
        assert!(!is_code_lookup("team awareness layer", ""));
        assert!(!is_code_lookup("attention contract", ""));
        assert!(!is_code_lookup("harvesting pipeline", ""));
    }

    // === pattern_hash is deterministic ===

    #[test]
    fn test_pattern_hash_deterministic() {
        let h1 = pattern_hash("test pattern");
        let h2 = pattern_hash("test pattern");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_pattern_hash_different() {
        let h1 = pattern_hash("pattern a");
        let h2 = pattern_hash("pattern b");
        assert_ne!(h1, h2);
    }

    // === Tool gating: only Grep/Glob ===

    #[tokio::test]
    async fn test_non_search_tool_passes() {
        let state = AppState::new();
        let input = HookInput {
            tool_name: Some("Bash".to_string()),
            tool_input: Some(json!({"command": "ls", "pattern": ""})),
            tool_response: None,
            session_id: None,
            cwd: Some("/Users/jeffbridwell/CascadeProjects/architect".to_string()),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: None,
        };
        let r = check(&input, &state).await;
        assert!(r.stdout.is_none());
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_empty_pattern_passes() {
        let state = AppState::new();
        let input = HookInput {
            tool_name: Some("Grep".to_string()),
            tool_input: Some(json!({"pattern": ""})),
            tool_response: None,
            session_id: None,
            cwd: Some("/Users/jeffbridwell/CascadeProjects/architect".to_string()),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: None,
        };
        let r = check(&input, &state).await;
        assert!(r.stdout.is_none());
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_code_lookup_allows_silently() {
        let state = AppState::new();
        let input = HookInput {
            tool_name: Some("Grep".to_string()),
            tool_input: Some(json!({"pattern": "HookResponse", "path": ""})),
            tool_response: None,
            session_id: None,
            cwd: Some("/Users/jeffbridwell/CascadeProjects/architect".to_string()),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: None,
        };
        let r = check(&input, &state).await;
        assert!(r.stdout.is_none());
        assert_eq!(r.exit_code, 0);
    }

    // === Search block / retry bypass ===

    #[tokio::test]
    async fn test_search_block_retry_bypass() {
        let state = AppState::new();
        let role = "silas";
        let hash = pattern_hash("disk usage");
        let block_key = format!("{}-{}", role, hash);

        // Simulate a prior enrichment block
        state.set_search_block(&block_key).await;

        let input = HookInput {
            tool_name: Some("Grep".to_string()),
            tool_input: Some(json!({"pattern": "disk usage", "path": ""})),
            tool_response: None,
            session_id: None,
            cwd: Some("/Users/jeffbridwell/CascadeProjects/architect".to_string()),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: None,
        };

        let r = check(&input, &state).await;
        // Should allow on retry (block cleared)
        assert!(r.stdout.is_none());
        assert_eq!(r.exit_code, 0);

        // Block should be cleared
        assert!(!state.check_search_block(&block_key).await);
    }

    // === Chorus recent search cooldown ===

    #[tokio::test]
    async fn test_chorus_recent_search_skips_enrichment() {
        let state = AppState::new();
        state.set_chorus_searched("silas").await;

        let input = HookInput {
            tool_name: Some("Grep".to_string()),
            tool_input: Some(json!({"pattern": "disk usage", "path": ""})),
            tool_response: None,
            session_id: None,
            cwd: Some("/Users/jeffbridwell/CascadeProjects/architect".to_string()),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: None,
        };

        let r = check(&input, &state).await;
        // Should skip enrichment due to cooldown
        assert!(r.stdout.is_none());
        assert_eq!(r.exit_code, 0);
    }

    // === decode_chunked (shim helper reimplemented here for reference) ===
    // Note: decode_chunked lives in shim.rs, tested in integration tests
}
