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
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};
use tracing::info;

// #2231: per-turn cost tuning — caches for expensive context primitives.
// Injection output stays byte-identical for same inputs within TTL.
const PULSE_STALE_THRESHOLD: Duration = Duration::from_secs(30);
const HYBRID_CACHE_TTL: Duration = Duration::from_secs(30);
const ATHENA_CACHE_TTL: Duration = Duration::from_secs(60);

static HYBRID_CACHE: LazyLock<Mutex<HashMap<String, (Instant, Vec<(String, String, String)>)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static ATHENA_CACHE: LazyLock<Mutex<HashMap<String, (Instant, Option<String>)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

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

/// Return true when /tmp/pulse-latest.json is missing or older than the
/// staleness threshold. Used to gate the per-prompt pulse rebuild spawn —
/// the daemon (#1881) already refreshes on its own schedule.
fn pulse_snapshot_stale() -> bool {
    std::fs::metadata("/tmp/pulse-latest.json")
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .map(|age| age > PULSE_STALE_THRESHOLD)
        .unwrap_or(true)
}

/// Cached wrapper around `query_chorus_hybrid`. Keyed by role + sorted
/// keywords so two prompts with the same meaningful tokens share results
/// within HYBRID_CACHE_TTL.
fn cached_query_chorus_hybrid(
    role: &str,
    keywords: &[String],
    query: &str,
) -> Vec<(String, String, String)> {
    let mut sorted = keywords.to_vec();
    sorted.sort();
    let key = format!("{}|{}", role, sorted.join(","));

    if let Ok(cache) = HYBRID_CACHE.lock() {
        if let Some((stamp, results)) = cache.get(&key) {
            if stamp.elapsed() < HYBRID_CACHE_TTL {
                return results.clone();
            }
        }
    }

    let results = query_chorus_hybrid(query);
    if let Ok(mut cache) = HYBRID_CACHE.lock() {
        cache.insert(key, (Instant::now(), results.clone()));
    }
    results
}

/// Cached wrapper around `query_athena_domain`. Keyed by role with
/// ATHENA_CACHE_TTL — the role's WIP domain rarely changes within a minute.
fn cached_query_athena_domain(role: &str) -> Option<String> {
    if let Ok(cache) = ATHENA_CACHE.lock() {
        if let Some((stamp, value)) = cache.get(role) {
            if stamp.elapsed() < ATHENA_CACHE_TTL {
                return value.clone();
            }
        }
    }
    let value = query_athena_domain(role);
    if let Ok(mut cache) = ATHENA_CACHE.lock() {
        cache.insert(role.to_string(), (Instant::now(), value.clone()));
    }
    value
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

/// Read the latest pulse snapshot and return a compact summary block.
/// Returns None if the snapshot file is missing or unparseable.
fn read_pulse_snapshot() -> Option<String> {
    let body = std::fs::read_to_string("/tmp/pulse-latest.json").ok()?;
    let v: serde_json::Value = serde_json::from_str(&body).ok()?;

    let mut out = String::new();
    if let Some(status) = v.pointer("/health/status").and_then(|s| s.as_str()) {
        let failures = v.pointer("/health/failures").and_then(|f| f.as_i64()).unwrap_or(0);
        let warns = v.pointer("/health/warning_count").and_then(|f| f.as_i64()).unwrap_or(0);
        out.push_str(&format!("  health: {} (failures={}, warnings={})\n", status, failures, warns));
    }
    if let Some(wip) = v.pointer("/board/wip_cards").and_then(|c| c.as_array()) {
        out.push_str(&format!("  wip_cards: {}\n", wip.len()));
        for card in wip.iter().take(5) {
            let id = card.get("id").and_then(|i| i.as_i64()).unwrap_or(0);
            let title = card.get("title").and_then(|t| t.as_str()).unwrap_or("");
            let owner = card.get("owner").and_then(|o| o.as_str()).unwrap_or("");
            let short: String = title.chars().take(80).collect();
            out.push_str(&format!("    #{} [{}] {}\n", id, owner, short));
        }
    }
    if let Some(roles) = v.pointer("/roles").and_then(|r| r.as_object()) {
        for (role, data) in roles.iter().take(3) {
            let state = data.get("state").and_then(|s| s.as_str()).unwrap_or("?");
            let card = data.get("card").and_then(|c| c.as_i64()).map(|i| format!("#{}", i)).unwrap_or_default();
            out.push_str(&format!("  role {}: state={} card={}\n", role, state, card));
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

/// Read the last N spine events directly from chorus.log. The spine is the
/// durable append-only event stream; no HTTP round-trip needed. Each line is
/// a JSON object with timestamp/role/event fields.
fn query_recent_spine(limit: usize) -> Vec<(String, String, String)> {
    let log_path = format!("{}/platform/logs/chorus.log",
        crate::shared::state_paths::REPO_ROOT);
    let content = match std::fs::read_to_string(&log_path) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let mut out = Vec::new();
    for line in content.lines().rev() {
        if out.len() >= limit { break; }
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let ts = v.get("timestamp").and_then(|s| s.as_str()).unwrap_or("").chars().take(19).collect::<String>();
        let role = v.get("role").and_then(|s| s.as_str()).unwrap_or("").to_string();
        let event = v.get("event").and_then(|s| s.as_str()).unwrap_or("").to_string();
        if !event.is_empty() {
            out.push((ts, role, event));
        }
    }
    out.reverse(); // oldest → newest for display
    out
}

/// Query Athena for the role's current domain context. Reads the role's WIP card
/// domain label from /tmp/pulse-latest.json, then fetches the domain description.
fn query_athena_domain(role: &str) -> Option<String> {
    let body = std::fs::read_to_string("/tmp/pulse-latest.json").ok()?;
    let v: serde_json::Value = serde_json::from_str(&body).ok()?;

    // Find this role's WIP card, pull domain from its labels
    let wip = v.pointer("/board/wip_cards")?.as_array()?;
    let card = wip.iter().find(|c| {
        c.get("owner").and_then(|o| o.as_str()).map(|s| s.to_lowercase()) == Some(role.to_lowercase())
    })?;
    let domain = card.get("domain").and_then(|d| d.as_str())?;

    let url = format!("http://localhost:3340/api/chorus/domain/{}", domain);
    let resp = ureq::get(&url).timeout(std::time::Duration::from_millis(500)).call().ok()?;
    let body: serde_json::Value = resp.into_json().ok()?;
    let desc = body.get("description").and_then(|d| d.as_str()).unwrap_or("");
    let cards_total = body.pointer("/cards/total").and_then(|c| c.as_i64()).unwrap_or(0);
    let cards_wip = body.pointer("/cards/wip").and_then(|c| c.as_i64()).unwrap_or(0);
    let mut out = format!("  domain: {} ({} cards total, {} WIP)\n", domain, cards_total, cards_wip);
    if !desc.is_empty() {
        let short: String = desc.chars().take(200).collect();
        out.push_str(&format!("  {}\n", short));
    }

    // #2178: render each entity with owner + short description + reads/writes.
    // Prefers itemDetails (rich entity records from the enriched API); falls
    // back to items (label array) only if itemDetails is absent. AC-8
    // retire-as-you-add: the fallback exists as a safety net for older
    // chorus-api deployments that haven't shipped #2178 yet; once chorus-api
    // is pinned at >= 998e33ab the fallback is dead code and gets removed.
    if let Some(sections) = body.get("sections").and_then(|s| s.as_object()) {
        const KEYS: &[&str] = &["services", "integrations", "persistence", "pipeline", "scenarios", "contract", "gaps"];
        for key in KEYS {
            let Some(section) = sections.get(*key) else { continue };
            if let Some(details) = section.get("itemDetails").and_then(|d| d.as_array()) {
                if details.is_empty() { continue }
                out.push_str(&format!("  {}:\n", key));
                for d in details.iter().take(4) {
                    let label = d.get("label").and_then(|l| l.as_str()).unwrap_or("?");
                    let owner = d.get("owner").and_then(|o| {
                        if let Some(s) = o.as_str() { Some(s.to_string()) }
                        else if let Some(a) = o.as_array() {
                            Some(a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect::<Vec<_>>().join("+"))
                        } else { None }
                    });
                    let inherited = d.get("ownerInherited").and_then(|b| b.as_bool()).unwrap_or(false);
                    let owner_tag = match owner {
                        Some(o) if inherited => format!(" ({} inherited)", o),
                        Some(o) => format!(" ({})", o),
                        None => String::new(),
                    };
                    let desc = d.get("description").and_then(|c| c.as_str()).map(|s| {
                        let short: String = s.chars().take(80).collect();
                        format!(" — {}", short)
                    }).unwrap_or_default();
                    let flow = {
                        let reads = d.get("reads").and_then(|r| r.as_array())
                            .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(", "));
                        let writes = d.get("writes").and_then(|w| w.as_array())
                            .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(", "));
                        match (reads.as_deref().filter(|s| !s.is_empty()), writes.as_deref().filter(|s| !s.is_empty())) {
                            (Some(r), Some(w)) => format!(" [reads {} / writes {}]", r, w),
                            (Some(r), None) => format!(" [reads {}]", r),
                            (None, Some(w)) => format!(" [writes {}]", w),
                            _ => String::new(),
                        }
                    };
                    out.push_str(&format!("    - {}{}{}{}\n", label, owner_tag, desc, flow));
                }
                if details.len() > 4 {
                    out.push_str(&format!("    (+{} more)\n", details.len() - 4));
                }
            } else if let Some(items) = section.get("items").and_then(|i| i.as_array()) {
                // Fallback: older chorus-api without itemDetails — label-only.
                if items.is_empty() { continue }
                let labels: Vec<String> = items.iter().take(4)
                    .filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                if labels.is_empty() { continue }
                let more = if items.len() > labels.len() {
                    format!(" (+{} more)", items.len() - labels.len())
                } else { String::new() };
                out.push_str(&format!("  {}: {}{}\n", key, labels.join(", "), more));
            }
        }
    }
    Some(out)
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

    // Pulse: assemble team state snapshot. A background daemon already refreshes
    // /tmp/pulse-latest.json on schedule (#1881); only spawn a rebuild when the
    // snapshot is stale past PULSE_STALE_THRESHOLD. Pre-#2231 this spawned
    // unconditionally on every prompt, ~200ms of redundant work per turn.
    if pulse_snapshot_stale() {
        let shim = format!("{}/platform/services/chorus-hooks/target/release/chorus-hook-shim",
            crate::shared::state_paths::REPO_ROOT);
        let _ = std::process::Command::new(&shim).arg("pulse")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }

    // Search Chorus API — hybrid mode (FTS + semantic + SPARQL). Cached per
    // (role, keywords-hash) with HYBRID_CACHE_TTL so successive prompts that
    // share keywords don't re-query.
    let chorus_results = cached_query_chorus_hybrid(&role_name, &keywords, &query);

    // Scan memory files
    let memory_hits = scan_memory(&keywords);

    // Foundational context primitives — read every prompt, not just when search
    // returns hits. These are the team's shared present tense: team state,
    // recent events, active domain. Jeff has asked repeatedly for them on every
    // envelope; the spec test in platform/tests/context-inject-envelope-spec.bats
    // locks that contract.
    let pulse_block = read_pulse_snapshot();
    let spine_events = query_recent_spine(8);
    let athena_block = cached_query_athena_domain(&role_name);

    // Build the context block — always inject the three primitives if any are
    // present, regardless of whether search turned up hits.
    let mut context = String::from("\n<context-synthesis>\n");
    context.push_str(&format!("Keywords: {}\n", query));

    if let Some(pulse) = &pulse_block {
        context.push_str("\n");
        context.push_str("## Pulse\n");
        context.push_str(pulse);
    }

    if !spine_events.is_empty() {
        context.push_str("\n");
        context.push_str(&format!("## Spine ({} recent events)\n", spine_events.len()));
        for (ts, role, event) in &spine_events {
            context.push_str(&format!("  [{}] {} → {}\n", ts, role, event));
        }
    }

    if let Some(athena) = &athena_block {
        context.push_str("\n");
        context.push_str("## Athena\n");
        context.push_str(athena);
    }

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

    // If every source is empty, skip injection entirely.
    if pulse_block.is_none() && spine_events.is_empty() && athena_block.is_none()
        && chorus_results.is_empty() && memory_hits.is_empty() {
        info!(
            gate = "context-inject",
            event = "no-results",
            role = %role_name,
            query = %query,
        );
        return HookResponse::allow();
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

    let cycle_id = state.get_cycle_id(session_id).await.unwrap_or_default();
    info!(
        gate = "context-inject",
        event = "injected",
        role = %role_name,
        query = %query,
        cycle_id = %cycle_id,
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
