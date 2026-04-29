use crate::state::{chorus_log, AppState};
use crate::types::{permission_deny_json, HookInput, HookResponse};
use rusqlite::Connection;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

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

/// Layer 2: Query Loki for operational signal, fall back to local files
fn query_logs(pattern: &str) -> Option<String> {
    // Try Loki first — has app logs including seed errors
    if let Some(loki_results) = query_loki(pattern) {
        return Some(loki_results);
    }

    // Fall back to local files
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

/// Query Loki for log entries matching pattern
fn query_loki(pattern: &str) -> Option<String> {
    let safe_pat: String = pattern
        .replace(['"', '\\', '|', '{', '}'], "")
        .chars()
        .take(40)
        .collect();

    if safe_pat.is_empty() {
        return None;
    }

    // Target app logs, use query_range with 24h lookback
    let query = format!("{{job=~\"gathering-app|nifi\"}} |= `{}`", safe_pat);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let start = format!("{}", now - 604800); // 7 days — match Loki retention
    let end = format!("{}", now);

    let resp = ureq::get("http://localhost:3102/loki/api/v1/query_range")
        .query("query", &query)
        .query("start", &start)
        .query("end", &end)
        .query("limit", "5")
        .timeout(std::time::Duration::from_millis(2000))
        .call()
        .ok()?;

    let body: serde_json::Value = resp.into_json().ok()?;
    let results = body.get("data")?.get("result")?.as_array()?;

    if results.is_empty() {
        return None;
    }

    let mut lines = Vec::new();
    for stream in results.iter().take(5) {
        let job = stream
            .get("stream")
            .and_then(|s| s.get("job"))
            .and_then(|j| j.as_str())
            .unwrap_or("unknown");
        if let Some(values) = stream.get("values").and_then(|v| v.as_array()) {
            for val in values.iter().take(2) {
                if let Some(msg) = val.as_array().and_then(|a| a.get(1)).and_then(|v| v.as_str()) {
                    let short: String = msg.chars().take(200).collect();
                    lines.push(format!("  [{}] {}", job, short));
                }
            }
        }
    }

    if lines.is_empty() {
        return None;
    }

    Some(format!("Loki logs ({}):\n{}", lines.len(), lines.join("\n")))
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

/// Layer 4: Query board for cards matching the pattern
fn query_cards(pattern: &str) -> Option<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let script = format!("{}/CascadeProjects/chorus/platform/scripts/cards", home);

    let output = std::process::Command::new("bash")
        .args([&script, "list"])
        .env("PATH", format!("{}/CascadeProjects/chorus/platform/scripts:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin", home))
        .env("HOME", &home)
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let pattern_lower = pattern.to_lowercase();
    let lines: Vec<&str> = stdout
        .lines()
        .filter(|l| l.to_lowercase().contains(&pattern_lower))
        .take(8)
        .collect();

    if lines.is_empty() {
        return None;
    }

    Some(format!(
        "Board cards ({}):\n{}",
        lines.len(),
        lines.iter().map(|l| format!("  {}", l.trim())).collect::<Vec<_>>().join("\n")
    ))
}

/// Extract search term from git command (--grep value or filename for blame)
fn extract_git_search_term(cmd: &str) -> String {
    if let Some(pos) = cmd.find("--grep") {
        let after = &cmd[pos + 6..];
        let after = after.trim_start_matches('=').trim_start();
        if after.starts_with('"') || after.starts_with('\'') {
            let quote = after.chars().next().unwrap();
            if let Some(end) = after[1..].find(quote) {
                return after[1..1 + end].to_string();
            }
        }
        return after.split_whitespace().next().unwrap_or("").to_string();
    }
    let trimmed = cmd.trim();
    if trimmed.to_lowercase().starts_with("git blame") {
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        for part in parts.iter().skip(2) {
            if !part.starts_with('-') {
                return part.to_string();
            }
        }
    }
    // Fallback: use last non-flag argument
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    for part in parts.iter().rev() {
        if !part.starts_with('-') && *part != "git" && *part != "log" && *part != "show" {
            return part.to_string();
        }
    }
    String::new()
}

pub async fn check(input: &HookInput, state: &AppState) -> HookResponse {
    let tool = input.tool_name_str();

    // Reboot exemption (#1866) — skip all search enrichment during close-out
    let role_str = input.role().as_str().to_string();
    let reboot_flag = format!("/tmp/reboot-{}.active", role_str);
    if std::path::Path::new(&reboot_flag).exists() {
        return HookResponse::allow();
    }

    // Detect git-as-search via Bash — same gate as Grep/Glob
    let (is_search, pattern) = if tool == "Grep" || tool == "Glob" {
        (true, input.get_tool_input_str("pattern"))
    } else if tool == "Bash" {
        let cmd = input.get_tool_input_str("command");
        let cmd_lower = cmd.to_lowercase();
        let trimmed = cmd_lower.trim_start();
        if trimmed.starts_with("git log")
            || trimmed.starts_with("git blame")
            || trimmed.starts_with("git show")
            || trimmed.contains("| git log")
            || trimmed.contains("&& git log")
        {
            let search_term = extract_git_search_term(&cmd);
            (true, search_term)
        } else {
            (false, String::new())
        }
    } else {
        (false, String::new())
    };

    if !is_search {
        return HookResponse::allow();
    }

    let role = input.role();
    if pattern.is_empty() {
        return HookResponse::allow();
    }

    let _search_path = if tool == "Grep" || tool == "Glob" {
        input.get_tool_input_str("path")
    } else {
        String::new()
    };

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

    // Check shared state from context_inject first (#2225)
    // If context_inject already ran this prompt cycle, use its results
    let cached = state.get_context_results(session_id).await;
    let chorus_results = if let Some(ref cached) = cached {
        // Reformat cached results to match expected format
        let formatted = format!(
            "Chorus found {} results (from context-inject cache):\n\n{}",
            cached.chorus_hits.len(),
            cached.chorus_hits.iter()
                .map(|(role, content, ts)| {
                    let short: String = content.replace('\n', " ").chars().take(200).collect();
                    let ts_short: String = ts.chars().take(16).collect();
                    format!("  [{}] {} — {}\n", ts_short, role, short)
                })
                .collect::<String>()
        );
        if cached.chorus_hits.is_empty() { None } else { Some(formatted) }
    } else {
        // No cached results — query Chorus DB directly (fallback)
        let db_path = state.config.chorus_db.clone();
        if !db_path.exists() {
            None
        } else {
            let pattern_clone = pattern.clone();
            tokio::task::spawn_blocking(move || query_chorus(&db_path, &pattern_clone))
                .await
                .unwrap_or(None)
        }
    };

    // Telemetry
    let has_results = chorus_results.is_some();
    let used_cache = cached.is_some();
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
                ("used_cache", if used_cache { "true" } else { "false" }),
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

    // Layer 4: Cards context — board state for the pattern
    let card_pattern = pattern.clone();
    let card_context = tokio::task::spawn_blocking(move || query_cards(&card_pattern))
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
    if let Some(ref cards) = card_context {
        layers.push(format!("## Cards (board state)\n{}", cards));
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
    use crate::shared::state_paths::chorus_root;
    use serde_json::json;

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
            cwd: Some(format!("{}/architect", chorus_root())),
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
            cwd: Some(format!("{}/architect", chorus_root())),
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
        let session_id = "unknown"; // matches None session_id default
        let role = "silas";
        let hash = pattern_hash("disk usage");
        let block_key = format!("{}-{}-{}", session_id, role, hash);

        // Simulate a prior enrichment block
        state.set_search_block(&block_key).await;

        let input = HookInput {
            tool_name: Some("Grep".to_string()),
            tool_input: Some(json!({"pattern": "disk usage", "path": ""})),
            tool_response: None,
            session_id: None,
            cwd: Some(format!("{}/architect", chorus_root())),
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

    // === decode_chunked (shim helper reimplemented here for reference) ===
    // Note: decode_chunked lives in shim.rs, tested in integration tests
}
