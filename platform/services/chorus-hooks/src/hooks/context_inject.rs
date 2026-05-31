//! Context injection hook (#1838)
//!
//! UserPromptSubmit: when Jeff states a problem or asks a question,
//! automatically search Chorus + memory + git and inject the synthesis
//! into the role's context via stderr. Zero discretion — the role
//! doesn't choose whether to search. The system does it for them.
//!
//! This replaces the "search and throw away results" failure mode.
//! The role sees accumulated context before they start thinking.

// These helpers are consumed by the binary target via check() (main.rs:650); the
// lib target lints them as dead. Suppress to match the crate-wide lib-vs-bin baseline.
#![allow(dead_code)]
#![allow(clippy::cognitive_complexity)]

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

#[allow(clippy::type_complexity)]
static HYBRID_CACHE: LazyLock<Mutex<HashMap<String, (Instant, Vec<(String, String, String, f64)>)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[allow(clippy::type_complexity)]
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
        .filter(|w| w.len() > 1 && !STOP_WORDS.contains(w))
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
    domain_tag: Option<&str>,
) -> Vec<(String, String, String, f64)> {
    let mut sorted = keywords.to_vec();
    sorted.sort();
    // #3134: the domain tag is part of the cache identity — same keywords under
    // a different WIP card must not return the other card's scoped results.
    let key = format!("{}|{}|{}", role, sorted.join(","), domain_tag.unwrap_or(""));

    if let Ok(cache) = HYBRID_CACHE.lock() {
        if let Some((stamp, results)) = cache.get(&key) {
            if stamp.elapsed() < HYBRID_CACHE_TTL {
                return results.clone();
            }
        }
    }

    let results = query_chorus_hybrid(query, domain_tag);
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

/// Build the hybrid-search URL (#3134). The prompt keywords ARE the query —
/// the per-prompt search is driven by what Jeff actually typed. When the role
/// has a WIP card, its domain is passed as an optional `&domain=` tag so results
/// are scoped to the card's area (card-as-tag); with `None` (the common case —
/// no card, venting, ideating) the query runs on the prompt alone. Pure +
/// unit-tested (tests/context_inject_card_tag_3134.rs) so the tag wiring can't
/// silently regress. `pub` for that integration test — it's a stateless helper.
pub fn build_search_url(query: &str, domain_tag: Option<&str>) -> String {
    fn enc(s: &str) -> String {
        s.chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                    c.to_string()
                } else {
                    format!("%{:02X}", c as u32)
                }
            })
            .collect()
    }
    let mut url = format!(
        "http://localhost:3340/api/chorus/search?q={}&mode=hybrid&limit=5",
        enc(query)
    );
    if let Some(domain) = domain_tag.map(str::trim).filter(|s| !s.is_empty()) {
        url.push_str(&format!("&domain={}", enc(domain)));
    }
    url
}

/// Query Chorus API hybrid search — FTS + semantic + SPARQL via RRF (#2003)
/// Replaces direct SQLite FTS with API call for richer context (233ms measured).
/// #3134: optional `domain_tag` scopes results to the WIP card's area.
fn query_chorus_hybrid(query: &str, domain_tag: Option<&str>) -> Vec<(String, String, String, f64)> {
    let url = build_search_url(query, domain_tag);

    let resp = match ureq::get(&url)
        .call()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("context-inject: chorus search fetch FAILED (grounding lost this prompt): {}", e);
            return vec![];
        }
    };

    let body: serde_json::Value = match resp.into_json() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("context-inject: chorus search response parse FAILED: {}", e);
            return vec![];
        }
    };

    let mut results = Vec::new();
    if let Some(items) = body.get("results").and_then(|r| r.as_array()) {
        for item in items.iter().take(5) {
            let role = item.get("role").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let content = item.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let ts = item.get("timestamp").and_then(|v| v.as_str()).unwrap_or("").to_string();
            // #3147 — carry the relevance score so ranking is tunable from the log.
            // RRF (fusion) is primary; semantic is the fallback; 0.0 if neither present.
            let score = item.get("_rrf_score").and_then(|v| v.as_f64())
                .or_else(|| item.get("_semantic_score").and_then(|v| v.as_f64()))
                .unwrap_or(0.0);
            if !content.is_empty() {
                results.push((role, content, ts, score));
            }
        }
    }

    results
}

/// Recent error-level log lines from Loki (#3032). Chorus search does NOT index
/// logs, so this is the complementary half of the forcing function: the live
/// "what's breaking right now" signal, injected every prompt. Fail-open and
/// tightly time-boxed (300ms) — a slow or down Loki must never block a prompt.
fn query_recent_log_errors() -> Vec<(String, String)> {
    let now_ns: u128 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let start_ns = now_ns.saturating_sub(900_000_000_000); // last 15 minutes

    // #3035: failures don't log at level=error (they're warn/info/none with an
    // event name ending .failed/.error). Match the real failure events, not a
    // level that never appears. (The level=error query returned 0 in prod.)
    let logql = r#"{job=~".+"} |~ "\"event\":\"[a-z._]*\\.(failed|error)\"""#;
    let resp = match ureq::get("http://localhost:3102/loki/api/v1/query_range")
        .query("query", logql)
        .query("start", &start_ns.to_string())
        .query("end", &now_ns.to_string())
        .query("limit", "5")
        .query("direction", "backward")
        .call()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("context-inject: Loki fetch FAILED (live error signal lost this prompt): {}", e);
            return vec![];
        }
    };

    let body: serde_json::Value = match resp.into_json() {
        Ok(v) => v,
        Err(_) => return vec![],
    };

    parse_loki_errors(&body)
}

/// Pure extractor (testable): pull up to 5 compact "event role" summaries from a
/// Loki query_range JSON body. JSON log lines collapse to "event role"; non-JSON
/// lines fall back to a trimmed raw snippet.
fn parse_loki_errors(body: &serde_json::Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(streams) = body.pointer("/data/result").and_then(|r| r.as_array()) {
        for stream in streams {
            if let Some(values) = stream.get("values").and_then(|v| v.as_array()) {
                for pair in values {
                    let arr = match pair.as_array() {
                        Some(a) => a,
                        None => continue,
                    };
                    let line = arr.get(1).and_then(|v| v.as_str()).unwrap_or("");
                    let summary = serde_json::from_str::<serde_json::Value>(line)
                        .ok()
                        .and_then(|j| {
                            let ev = j.get("event").and_then(|v| v.as_str())?;
                            let role = j.get("role").and_then(|v| v.as_str()).unwrap_or("");
                            Some(format!("{} {}", ev, role).trim().to_string())
                        })
                        .unwrap_or_else(|| line.replace('\n', " ").chars().take(160).collect());
                    if !summary.is_empty() {
                        out.push((String::new(), summary));
                    }
                }
            }
        }
    }
    out.truncate(5);
    out
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
        // #2234 Step 6 prototype: surface count + pull-pointer instead of inlining card titles.
        // Full card detail is available at /api/chorus/context/board/wip — agents that need it
        // fetch and cite. This removes ~5 card-title lines (~300 bytes) per turn.
        out.push_str(&format!("  board.wip: {} cards → GET /api/chorus/context/board/wip\n", wip.len()));
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
    let log_path = crate::shared::state_paths::chorus_log_file();
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
    let resp = match ureq::get(&url).call() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("context-inject: athena domain fetch FAILED: {}", e);
            return None;
        }
    };
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

/// All chorus-project memory dirs under ~/.claude/projects.
/// #3032: the old path hardcoded "-Users-jeffbridwell-CascadeProjects/memory" —
/// missing the "-chorus" project suffix — so read_dir failed and the per-prompt
/// memory scan silently returned nothing. Derive instead: match any project key
/// containing "chorus" that has a memory/ dir (robust to role-suffixed keys too).
fn chorus_memory_dirs() -> Vec<std::path::PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/jeffbridwell".to_string());
    chorus_memory_dirs_in(&format!("{}/.claude/projects", home))
}

/// Pure (testable): chorus-keyed project `memory/` dirs under a projects root.
fn chorus_memory_dirs_in(projects: &str) -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(rd) = std::fs::read_dir(projects) {
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().contains("chorus") {
                let m = e.path().join("memory");
                if m.is_dir() {
                    dirs.push(m);
                }
            }
        }
    }
    dirs
}

/// Scan memory files for related decisions and feedback
fn scan_memory(keywords: &[String]) -> Vec<String> {
    let mut hits = Vec::new();
    let mut seen_files = std::collections::HashSet::new();

    for memory_dir in chorus_memory_dirs() {
        let dir = match std::fs::read_dir(&memory_dir) {
            Ok(d) => d,
            Err(_) => continue,
        };

        for entry in dir.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "md") {
                continue;
            }
            if path.file_name().is_some_and(|n| n == "MEMORY.md") {
                continue;
            }
            let fname = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            if !seen_files.insert(fname.clone()) {
                continue; // same filename already scanned in another project dir
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
                        hits.push(format!("[{}] {}", fname, first_line));
                    }
                }
            }
        }
    }

    hits.truncate(5);
    hits
}

/// Manifest envelope — orientation half of the per-prompt context (#2249 Phase 1).
/// Always built now (#3048); paired with the dynamic search/Loki synthesis.
/// ~2KB identity + orientation + endpoint list. No pre-synthesized blobs.
pub fn build_manifest_envelope(role: &str, card: Option<&str>, health: &str, team_wip: usize, role_wip: usize) -> String {
    let wip_line = match card {
        Some(c) => format!("You are {}. You are currently building {}.", role, c),
        None => format!("You are {}. You have no WIP card.", role),
    };
    format!(
        "<chorus-context role=\"{}\">\n\
        \n\
        {}\n\
        \n\
        Pulse (at glance):\n\
          health: {}\n\
          team WIP: {} · your WIP: {}\n\
          index freshness: ok\n\
        \n\
        Pull-first rule. When forming a claim about current state, query the endpoint and cite its timestamp.\n\
        \n\
        Context endpoints:\n\
          GET /api/chorus/context/board/wip?role={}  — current WIP\n\
          GET /api/chorus/context/roles              — all roles, state, card\n\
          GET /api/chorus/context/health             — system health\n\
          GET /api/chorus/context/alerts             — firing alerts\n\
          GET /api/chorus/context/spine?limit=10     — recent spine events\n\
        \n\
        Knowledge endpoints:\n\
          GET /api/chorus/knowledge/domains          — domain list\n\
          GET /api/chorus/knowledge/domains/{{name}}   — full domain detail\n\
          GET /api/chorus/knowledge/search?q=...     — graph + FTS\n\
        </chorus-context>",
        role, wip_line, health, team_wip, role_wip, role
    )
}

/// Parse pulse JSON for orientation band data.
fn parse_pulse_orientation(role: &str) -> (String, usize, usize, Option<String>) {
    let path = "/tmp/pulse-latest.json";
    let Ok(content) = std::fs::read_to_string(path) else {
        return ("unknown".into(), 0, 0, None);
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) else {
        return ("unknown".into(), 0, 0, None);
    };
    let health = v["health"]["status"].as_str().unwrap_or("unknown").to_string();
    let team_wip = v["board"]["wip_count"].as_u64().unwrap_or(0) as usize;
    let role_wip = v["board"]["wip_cards"].as_array()
        .map(|arr| arr.iter().filter(|c| c["owner"].as_str().unwrap_or("").to_lowercase() == role).count())
        .unwrap_or(0);
    let card = v["roles"][role]["card"].as_u64().map(|id| format!("#{}", id));
    (health, team_wip, role_wip, card)
}

/// The role's WIP card domain from the pulse snapshot, if any (#3134). Used as
/// the optional card-as-tag filter on the per-prompt search. Returns None when
/// the role has no WIP card — the common case — so the search runs prompt-only.
fn role_wip_domain(role: &str) -> Option<String> {
    let body = std::fs::read_to_string("/tmp/pulse-latest.json").ok()?;
    let v: serde_json::Value = serde_json::from_str(&body).ok()?;
    let wip = v.pointer("/board/wip_cards")?.as_array()?;
    let card = wip.iter().find(|c| {
        c.get("owner").and_then(|o| o.as_str()).map(|s| s.to_lowercase()) == Some(role.to_lowercase())
    })?;
    card.get("domain")
        .and_then(|d| d.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// #3134 — format one context-inject OUTCOME spine line (pure, testable).
/// Matches the chorus.log JSONL shape so promtail/Loki ingest it like any spine
/// event, which makes the per-prompt result queryable via `logs_*`/pulse.
// #3147 — flat structured-record formatter (8 fixed metric fields, not a complex fn);
// #[allow] is the codebase's intentional-many-arg marker, omitted in #3134. Unblocks
// the clippy-ratchet (too_many_arguments isn't in baseline) without loosening it.
#[allow(clippy::too_many_arguments)]
pub fn format_spine_line(
    ts: &str, role: &str, event: &str,
    chorus_hits: usize, memory_hits: usize, log_errors: usize,
    injected_bytes: usize, elapsed_ms: u64,
) -> String {
    format!(
        r#"{{"timestamp":"{}","level":"info","appName":"chorus-events","component":"context-inject","event":"{}","role":"{}","chorus_hits":{},"memory_hits":{},"log_errors":{},"injected_bytes":{},"elapsed_ms":{}}}"#,
        ts, event, role, chorus_hits, memory_hits, log_errors, injected_bytes, elapsed_ms
    )
}

/// #3134 — emit the per-prompt context-inject outcome to the spine
/// (`~/.chorus/chorus.log`). The `info!` calls in check() only reach daemon
/// stderr → /tmp/chorus-api.log (the 3-sink split that made the GET/USE outcome
/// unmeasurable). Append-only, fail-open — logging never blocks a prompt.
fn emit_spine_observation(
    role: &str, event: &str,
    chorus_hits: usize, memory_hits: usize, log_errors: usize,
    injected_bytes: usize, elapsed_ms: u64,
) {
    let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string();
    let line = format_spine_line(&ts, role, event, chorus_hits, memory_hits, log_errors, injected_bytes, elapsed_ms);
    emit_inject_event(&line);
}

// #3147 — append one JSONL line to the spine log (→ promtail → Loki). Same path as
// emit_spine_observation; factored out so the request/response events reuse it.
// chorus.log is NOT the semantic index (spine ingestion removed #3136), so inject
// observability lands in Loki only — telemetry, not searchable knowledge.
fn emit_inject_event(line: &str) {
    let path = crate::shared::state_paths::chorus_log_file();
    let _ = std::fs::OpenOptions::new().create(true).append(true).open(&path)
        .and_then(|mut f| { use std::io::Write; writeln!(f, "{}", line) });
}

// #3147 — what context_inject asked for (REQUEST) and what came back (RESPONSE),
// as named-field structs so the formatters take 2 args, not a dozen positional ones
// (clippy::too_many_arguments). This is also the shape Kade's crate emit_request /
// emit_response API will take, so it's the right interim form, not throwaway.
pub struct InjectRequest<'a> {
    pub inject_id: &'a str,
    pub role: &'a str,
    pub session_id: &'a str,
    pub prompt: &'a str,
    pub keywords: &'a [String],
    pub domain_tag: Option<&'a str>,
    pub calls: &'a [(&'a str, String, bool)],
}

pub struct InjectResponse<'a> {
    pub inject_id: &'a str,
    pub role: &'a str,
    // ranked order; rank = position; score = rrf/semantic relevance (#3147 — plumbed through query_chorus_hybrid)
    pub candidates: &'a [(String, String, String, f64)],
    pub memory_hits: usize,
    pub pulse_present: bool,
    pub spine_events: usize,
    pub athena_present: bool,
    pub log_errors: usize,
    pub assembled_bytes: usize,
    pub elapsed_ms: u64,
    pub drops: &'a [&'a str], // sources that returned nothing (the silent-drop class, #3048)
}

// Pure + testable. Paired to the response by inject_id (a span the controller's
// emit_request will mint once Kade's crate lands; minted inline here for now).
pub fn format_inject_request(ts: &str, r: &InjectRequest) -> String {
    let calls_json: Vec<serde_json::Value> = r.calls.iter()
        .map(|(target, query, fired)| serde_json::json!({"target": target, "query": query, "fired": fired}))
        .collect();
    serde_json::json!({
        "timestamp": ts, "level": "info", "appName": "chorus-events",
        "component": "context-inject", "event": "context.inject.request",
        "inject_id": r.inject_id, "role": r.role, "session_id": r.session_id,
        "prompt": r.prompt, "keywords": r.keywords, "domain_tag": r.domain_tag,
        "calls": calls_json,
    }).to_string()
}

pub fn format_inject_response(ts: &str, r: &InjectResponse) -> String {
    let cands: Vec<serde_json::Value> = r.candidates.iter().enumerate()
        .map(|(i, (source, content, cts, score))| {
            let snippet: String = content.replace('\n', " ").chars().take(120).collect();
            serde_json::json!({"source": source, "rank": i + 1, "snippet": snippet, "ts": cts, "score": score})
        })
        .collect();
    serde_json::json!({
        "timestamp": ts, "level": "info", "appName": "chorus-events",
        "component": "context-inject", "event": "context.inject.response",
        "inject_id": r.inject_id, "role": r.role,
        "candidates": cands,
        "memory_hits": r.memory_hits, "pulse": r.pulse_present, "spine_events": r.spine_events,
        "athena": r.athena_present, "log_errors": r.log_errors,
        "assembled_bytes": r.assembled_bytes, "elapsed_ms": r.elapsed_ms, "drops": r.drops,
    }).to_string()
}

// #3147 — emit with a fresh timestamp.
fn emit_inject_request(r: &InjectRequest) {
    let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string();
    emit_inject_event(&format_inject_request(&ts, r));
}

fn emit_inject_response(r: &InjectResponse) {
    let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string();
    emit_inject_event(&format_inject_response(&ts, r));
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

    // #3147 — one inject_id spans this prompt's request + response events (the pairing
    // key). Minted inline now; migrates to the span Kade's emit_request() will return.
    let session_id = input.session_id.as_deref().unwrap_or("unknown");
    let inject_id = format!("inj-{}-{}", role_name, chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0));

    // #3048: push BOTH the manifest (orientation: pulse + endpoints + nudges) AND
    // the live search/Loki synthesis (grounding) on every prompt. Pre-#3048 the
    // manifest returned early HERE, so the #3032 search/Loki path below was dead
    // code — shadowed by #2249's manifest-default. Now the manifest is built into
    // a block and the function falls through to the dynamic synthesis, which is
    // appended before returning. No CONTEXT_PUSH_MODE fork: both always run.
    let manifest_block = {
        let (health, team_wip, role_wip, card) = parse_pulse_orientation(&role_name);
        let envelope = build_manifest_envelope(&role_name, card.as_deref(), &health, team_wip, role_wip);
        let log_path_str = crate::shared::state_paths::chorus_log_file();
        let log_path = std::path::Path::new(&log_path_str);
        crate::hooks::nudge_poll::augment_envelope_with_nudges(
            &role_name, &envelope, log_path, 50_000, 10,
        )
    };

    // Pulse: assemble team state snapshot. A background daemon already refreshes
    // /tmp/pulse-latest.json on schedule (#1881); only spawn a rebuild when the
    // snapshot is stale past PULSE_STALE_THRESHOLD. Pre-#2231 this spawned
    // unconditionally on every prompt, ~200ms of redundant work per turn.
    if pulse_snapshot_stale() {
        let shim = format!("{}/platform/services/chorus-hooks/target/release/chorus-hook-shim",
            crate::shared::state_paths::repo_root());
        let _ = std::process::Command::new(&shim).arg("pulse")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }

    // #3048: no latency cap. Jeff's call (A): wait the few seconds and get the
    // context. Pre-#3048 a 700ms wall-clock budget silently dropped athena + Loki
    // when the envelope ran long — the same fail-open-silent bug as the per-call
    // timeouts, one level up. Removed. envelope_start is kept only to MEASURE and
    // log the real cost (AC5), never to skip a fetch.
    let envelope_start = std::time::Instant::now();

    // Search Chorus API — hybrid mode (FTS + semantic + SPARQL). The PROMPT
    // drives the query; the WIP card domain (if any) rides along as an optional
    // `&domain=` tag (#3134) — card-as-tag, not card-as-driver. No card → the
    // search runs prompt-only. Cached per (role, keywords-hash, domain-tag).
    let card_tag = role_wip_domain(&role_name);

    // #3147 — emit the REQUEST (what we're about to ask for) BEFORE the fan-out, so it
    // lands even if every source drops. The 6 calls mirror the spawn_blocking set below.
    let request_calls: Vec<(&str, String, bool)> = vec![
        ("chorus", build_search_url(&query, card_tag.as_deref()), true),
        ("memory", keywords.join(" "), true),
        ("pulse", "snapshot".to_string(), true),
        ("spine", "recent-8".to_string(), true),
        ("athena", role_name.clone(), true),
        ("logs", "loki-errors-15m".to_string(), true),
    ];
    emit_inject_request(&InjectRequest {
        inject_id: &inject_id, role: &role_name, session_id, prompt,
        keywords: &keywords, domain_tag: card_tag.as_deref(), calls: &request_calls,
    });

    // #3134: PARALLEL fan-out. The six sub-queries are independent reads with no
    // ordering dependency until assembly — running them sequentially made the
    // per-prompt latency the SUM of two chorus-api round-trips + Loki + the file
    // reads (search/memory/pulse/spine/athena/logs). spawn_blocking starts each
    // on the blocking pool immediately; we then await all, so wall-clock = the
    // slowest single query, not the sum. Output is byte-identical (the
    // envelope-spec bats locks the assembled block — only the fetch is concurrent).
    let (chorus_results, memory_hits, pulse_block, spine_events, athena_block, log_errors) = {
        let (r1, kw1, q1, tag1) = (role_name.clone(), keywords.clone(), query.clone(), card_tag.clone());
        let kw2 = keywords.clone();
        let r3 = role_name.clone();
        let t_chorus = tokio::task::spawn_blocking(move || cached_query_chorus_hybrid(&r1, &kw1, &q1, tag1.as_deref()));
        let t_memory = tokio::task::spawn_blocking(move || scan_memory(&kw2));
        let t_pulse = tokio::task::spawn_blocking(read_pulse_snapshot);
        let t_spine = tokio::task::spawn_blocking(|| query_recent_spine(8));
        let t_athena = tokio::task::spawn_blocking(move || cached_query_athena_domain(&r3));
        let t_logs = tokio::task::spawn_blocking(query_recent_log_errors);
        (
            t_chorus.await.unwrap_or_default(),
            t_memory.await.unwrap_or_default(),
            t_pulse.await.unwrap_or_default(),
            t_spine.await.unwrap_or_default(),
            t_athena.await.unwrap_or_default(),
            t_logs.await.unwrap_or_default(),
        )
    };

    // Build the context block — always inject the three primitives if any are
    // present, regardless of whether search turned up hits.
    let mut context = String::from("\n<context-synthesis>\n");
    context.push_str(&format!("Keywords: {}\n", query));

    // #3134: lead with the PROMPT-driven signal — the Chorus hybrid hits and
    // memory matches for what Jeff actually asked. These used to render last,
    // buried under the always-on board primitives; now they come first so the
    // relevant context isn't drowned. The board primitives (Pulse/Spine/Athena/
    // Logs) follow as orientation — still present (the envelope-spec contract
    // locks them), just demoted below the prompt answer.
    if !chorus_results.is_empty() {
        context.push_str(&format!("\nChorus hybrid ({} hits):\n", chorus_results.len()));
        for (role, content, ts, _score) in &chorus_results {
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

    if let Some(pulse) = &pulse_block {
        context.push('\n');
        context.push_str("## Pulse\n");
        context.push_str(pulse);
        // #2234 Step 6 prototype: endpoint manifest for Context API pull.
        // Cite what you read: "per /api/chorus/context/board/wip @ HH:MM, ..."
        context.push_str("  context api:\n");
        context.push_str("    board:  GET /api/chorus/context/board/wip?role=");
        context.push_str(&role_name);
        context.push('\n');
        context.push_str("    roles:  GET /api/chorus/context/roles\n");
        context.push_str("    health: GET /api/chorus/context/health\n");
    }

    if !spine_events.is_empty() {
        context.push('\n');
        context.push_str(&format!("## Spine ({} recent events)\n", spine_events.len()));
        for (ts, role, event) in &spine_events {
            context.push_str(&format!("  [{}] {} → {}\n", ts, role, event));
        }
    }

    if let Some(athena) = &athena_block {
        context.push('\n');
        context.push_str("## Athena\n");
        context.push_str(athena);
    }

    if !log_errors.is_empty() {
        context.push('\n');
        context.push_str(&format!("## Logs ({} recent errors, 15m)\n", log_errors.len()));
        for (_ts, summary) in &log_errors {
            context.push_str(&format!("  {}\n", summary));
        }
    }

    // If every dynamic source is empty, still push the manifest — orientation is
    // useful even when search/Loki/spine turn up nothing this prompt.
    if pulse_block.is_none() && spine_events.is_empty() && athena_block.is_none()
        && chorus_results.is_empty() && memory_hits.is_empty() && log_errors.is_empty() {
        let elapsed = envelope_start.elapsed().as_millis() as u64;
        info!(
            gate = "context-inject",
            event = "no-results",
            role = %role_name,
            query = %query,
            elapsed_ms = elapsed,
        );
        // #3134: observability — outcome on the spine, not just daemon stderr.
        emit_spine_observation(&role_name, "context.inject.empty", 0, 0, 0, manifest_block.len(), elapsed);
        // #3147 — response event for the empty path: all six sources dropped.
        emit_inject_response(&InjectResponse {
            inject_id: &inject_id, role: &role_name, candidates: &[],
            memory_hits: 0, pulse_present: false, spine_events: 0, athena_present: false,
            log_errors: 0, assembled_bytes: manifest_block.len(), elapsed_ms: elapsed,
            drops: &["chorus", "memory", "pulse", "spine", "athena", "logs"],
        });
        return HookResponse::warn_stderr(&format!("\n{}\n", manifest_block));
    }

    context.push_str("\nMANDATORY: You MUST reference this context before responding. Do not search filesystem or git for information already provided here. If Chorus returned results, cite them. Ignoring injected context is a protocol violation.\n");
    context.push_str("</context-synthesis>");

    // Store results in AppState for other hooks to read (#2225). session_id declared
    // at the top (#3147) so the request event can carry it.
    state.store_context_results(session_id, ContextSearchResults {
        chorus_hits: chorus_results.iter().map(|(r, c, t, _)| (r.clone(), c.clone(), t.clone())).collect(),
        memory_hits: memory_hits.clone(),
        query: query.clone(),
        stored_at: chrono::Utc::now().timestamp(),
    }).await;

    let cycle_id = state.get_cycle_id(session_id).await.unwrap_or_default();
    let elapsed = envelope_start.elapsed().as_millis() as u64;
    info!(
        gate = "context-inject",
        event = "injected",
        role = %role_name,
        query = %query,
        cycle_id = %cycle_id,
        chorus_hits = chorus_results.len(),
        memory_hits = memory_hits.len(),
        log_errors = log_errors.len(),
        elapsed_ms = elapsed,
    );

    // #3048: manifest (orientation) + dynamic synthesis (grounding), both pushed.
    let out = format!("\n{}\n{}", manifest_block, context);
    // #3147 — RESPONSE event: candidates (ranked), per-source presence, drops, bytes, ms.
    let mut drops: Vec<&str> = Vec::new();
    if chorus_results.is_empty() { drops.push("chorus"); }
    if memory_hits.is_empty() { drops.push("memory"); }
    if pulse_block.is_none() { drops.push("pulse"); }
    if spine_events.is_empty() { drops.push("spine"); }
    if athena_block.is_none() { drops.push("athena"); }
    if log_errors.is_empty() { drops.push("logs"); }
    emit_inject_response(&InjectResponse {
        inject_id: &inject_id, role: &role_name, candidates: &chorus_results,
        memory_hits: memory_hits.len(), pulse_present: pulse_block.is_some(),
        spine_events: spine_events.len(), athena_present: athena_block.is_some(),
        log_errors: log_errors.len(), assembled_bytes: out.len(), elapsed_ms: elapsed,
        drops: &drops,
    });
    // #3134: observability — emit the actual outcome (hits + injected bytes) to
    // the spine so GET/USE can be measured, not just the daemon's stderr log.
    emit_spine_observation(
        &role_name, "context.inject.injected",
        chorus_results.len(), memory_hits.len(), log_errors.len(),
        out.len(), elapsed,
    );
    HookResponse::warn_stderr(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // #3147 — the REQUEST event captures exactly what context_inject asked for.
    #[test]
    fn inject_request_captures_what_was_asked() {
        let kw = vec!["self".to_string(), "healing".to_string()];
        let calls = vec![
            ("chorus", "http://localhost:3340/api/chorus/search?q=self+healing".to_string(), true),
            ("memory", "self healing".to_string(), true),
        ];
        let line = format_inject_request("2026-05-30T18:00:00.000+0000", &InjectRequest {
            inject_id: "inj-wren-42", role: "wren", session_id: "sess-1",
            prompt: "so chorus is self-healing", keywords: &kw, domain_tag: Some("loom"), calls: &calls,
        });
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["event"], "context.inject.request");
        assert_eq!(v["inject_id"], "inj-wren-42");
        assert_eq!(v["prompt"], "so chorus is self-healing");
        assert_eq!(v["keywords"][1], "healing");
        assert_eq!(v["domain_tag"], "loom");
        assert_eq!(v["calls"][0]["target"], "chorus");
        assert_eq!(v["calls"][0]["fired"], true);
    }

    // #3147 — the RESPONSE event captures the ranked candidates + the silent drops.
    #[test]
    fn inject_response_captures_candidates_and_drops() {
        let cands = vec![
            ("claude".to_string(), "what did you get\nas a chorus-inject".to_string(), "2026-05-30T18:00".to_string(), 0.065),
            ("principle".to_string(), "Collaborate with succession".to_string(), "2026-05-26T17:40".to_string(), 0.061),
        ];
        let line = format_inject_response("2026-05-30T18:00:01.000+0000", &InjectResponse {
            inject_id: "inj-wren-42", role: "wren", candidates: &cands,
            memory_hits: 5, pulse_present: true, spine_events: 8, athena_present: false,
            log_errors: 0, assembled_bytes: 4876, elapsed_ms: 3278, drops: &["athena", "logs"],
        });
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["event"], "context.inject.response");
        assert_eq!(v["inject_id"], "inj-wren-42");
        assert_eq!(v["candidates"][0]["source"], "claude");
        assert_eq!(v["candidates"][0]["rank"], 1);
        assert!(v["candidates"][0]["snippet"].as_str().unwrap().contains("what did you get as a chorus-inject"));
        assert_eq!(v["candidates"][1]["rank"], 2);
        assert_eq!(v["memory_hits"], 5);
        assert_eq!(v["athena"], false);
        assert_eq!(v["assembled_bytes"], 4876);
        assert_eq!(v["elapsed_ms"], 3278);
        assert_eq!(v["drops"][0], "athena");
    }

    // #3147 (real scope) — the RESPONSE must carry the relevance SCORE per candidate, not just
    // rank+snippet. Tuning ranking is the whole point; you can't tune what isn't logged. Today the
    // candidate tuple is (source, content, ts) with no score, so this is RED until the score is
    // plumbed through query_chorus_hybrid (the line-620 follow-on). NOTE: this unit test is
    // scaffolding only — the real bar is a live inject response carrying a real _rrf_score.
    #[test]
    fn inject_response_candidate_carries_score() {
        let cands = vec![
            ("claude".to_string(), "some content".to_string(), "2026-05-30T18:00".to_string(), 0.065),
        ];
        let line = format_inject_response("2026-05-30T18:00:01.000+0000", &InjectResponse {
            inject_id: "inj-wren-99", role: "wren", candidates: &cands,
            memory_hits: 1, pulse_present: true, spine_events: 1, athena_present: false,
            log_errors: 0, assembled_bytes: 100, elapsed_ms: 10, drops: &[],
        });
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert!(v["candidates"][0]["score"].is_number(),
            "candidate must carry its relevance score (rrf/semantic), not just rank — got: {}", v["candidates"][0]);
    }

    // #3147 — request + response are the SAME prompt, paired by inject_id.
    #[test]
    fn request_and_response_pair_by_inject_id() {
        let req = format_inject_request("t", &InjectRequest {
            inject_id: "inj-X", role: "wren", session_id: "s", prompt: "p",
            keywords: &[], domain_tag: None, calls: &[],
        });
        let resp = format_inject_response("t", &InjectResponse {
            inject_id: "inj-X", role: "wren", candidates: &[],
            memory_hits: 0, pulse_present: false, spine_events: 0, athena_present: false,
            log_errors: 0, assembled_bytes: 0, elapsed_ms: 0, drops: &[],
        });
        let rv: serde_json::Value = serde_json::from_str(&req).unwrap();
        let pv: serde_json::Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(rv["inject_id"], pv["inject_id"]);
    }

    // #3147 — "just show me": run the real formatters against a LIVE chorus search
    // for a real prompt, pretty-print the request+response. Not a UI — a runnable
    // window. `cargo test inject_live_demo -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn inject_live_demo() {
        fn show(label: &str, line: &str) {
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            println!("\n=== {} ===\n{}", label, serde_json::to_string_pretty(&v).unwrap());
        }
        let prompt = "so chorus is self-healing lol";
        let keywords = extract_keywords(prompt);
        let query = keywords.join(" ");
        let candidates = query_chorus_hybrid(&query, None); // LIVE HTTP to :3340
        let calls = vec![
            ("chorus", build_search_url(&query, None), true),
            ("memory", query.clone(), true),
            ("pulse", "snapshot".to_string(), true),
            ("spine", "recent-8".to_string(), true),
            ("athena", "wren".to_string(), true),
            ("logs", "loki-errors-15m".to_string(), true),
        ];
        let id = "inj-demo-1";
        show("REQUEST", &format_inject_request("2026-05-30T14:50:00.000+0000", &InjectRequest {
            inject_id: id, role: "wren", session_id: "sess-demo", prompt,
            keywords: &keywords, domain_tag: None, calls: &calls,
        }));
        let drops: Vec<&str> = if candidates.is_empty() { vec!["chorus"] } else { vec![] };
        show("RESPONSE", &format_inject_response("2026-05-30T14:50:03.000+0000", &InjectResponse {
            inject_id: id, role: "wren", candidates: &candidates,
            memory_hits: 0, pulse_present: false, spine_events: 0, athena_present: false,
            log_errors: 0, assembled_bytes: 0, elapsed_ms: 0, drops: &drops,
        }));
    }

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

    // #3032: Logs section — pure Loki-body parser.
    #[test]
    fn parse_loki_errors_extracts_event_role_and_caps_at_5() {
        let body = serde_json::json!({
            "data": { "result": [{
                "stream": { "job": "daemon-logs" },
                "values": [
                    ["1779000000000000000", "{\"event\":\"crawler.domain.failed\",\"role\":\"silas\",\"level\":\"error\"}"],
                    ["1779000000000000001", "{\"event\":\"mcp.tool.error\",\"level\":\"error\"}"],
                    ["1779000000000000002", "raw non-json error line"],
                    ["1779000000000000003", "{\"event\":\"a\",\"role\":\"x\"}"],
                    ["1779000000000000004", "{\"event\":\"b\",\"role\":\"y\"}"],
                    ["1779000000000000005", "{\"event\":\"c\",\"role\":\"z\"}"]
                ]
            }]}
        });
        let out = parse_loki_errors(&body);
        assert!(out.len() <= 5, "caps at 5, got {}", out.len());
        assert_eq!(out[0].1, "crawler.domain.failed silas");
        assert_eq!(out[1].1, "mcp.tool.error", "no role → just event");
        assert_eq!(out[2].1, "raw non-json error line", "non-json falls back to snippet");
    }

    #[test]
    fn parse_loki_errors_empty_on_no_results() {
        assert!(parse_loki_errors(&serde_json::json!({ "data": { "result": [] } })).is_empty());
        assert!(parse_loki_errors(&serde_json::json!({ "nope": 1 })).is_empty());
    }

    // #3032: memory-scan path — the dead-hardcode regression. The fix derives
    // chorus-keyed project memory dirs; this proves it finds them and skips others.
    #[test]
    fn chorus_memory_dirs_finds_chorus_keyed_project_with_memory() {
        let tmp = std::env::temp_dir().join(format!("ci-3032-{}", std::process::id()));
        std::fs::create_dir_all(tmp.join("-Users-x-CascadeProjects-chorus").join("memory")).unwrap();
        std::fs::create_dir_all(tmp.join("-Users-x-somethingelse").join("memory")).unwrap();
        std::fs::create_dir_all(tmp.join("-Users-x-chorus-nomemory")).unwrap(); // chorus key, no memory dir

        let dirs = chorus_memory_dirs_in(tmp.to_str().unwrap());
        assert!(
            dirs.iter().any(|d| d.ends_with("memory") && d.to_string_lossy().contains("chorus")),
            "finds the chorus project memory dir: {:?}", dirs
        );
        assert!(
            !dirs.iter().any(|d| d.to_string_lossy().contains("somethingelse")),
            "skips non-chorus projects: {:?}", dirs
        );
        assert!(
            !dirs.iter().any(|d| d.to_string_lossy().contains("nomemory")),
            "skips chorus-keyed dirs that lack a memory/ subdir: {:?}", dirs
        );
        std::fs::remove_dir_all(&tmp).ok();
    }

    // #3048 — real-vs-real: call check() against the LIVE search (:3340) + Loki
    // (:3102). Proves the runtime path actually pushes non-empty context (catches
    // the 0-byte bug) instead of source-grepping. #[ignore]: needs live services,
    // so it never runs in CI — run explicitly: `cargo test --lib -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn check_pushes_nonempty_context_against_live_services() {
        let state = AppState::new();
        let input = HookInput {
            tool_name: None,
            tool_input: None,
            tool_response: None,
            session_id: Some("itest-3048".into()),
            cwd: Some("/Users/jeffbridwell/CascadeProjects/chorus/roles/wren".into()),
            prompt: Some(
                "what did wren and kade ship today and what pipeline errors happened recently"
                    .into(),
            ),
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("wren".into()),
            chorus_worktree_override: None,
        };
        let resp = check(&input, &state).await;
        let out = resp.stderr.clone().unwrap_or_default();
        eprintln!(
            "[itest #3048] injected {} bytes | search={} logs={} spine={} pulse={}",
            out.len(),
            out.contains("Chorus hybrid"),
            out.contains("## Logs"),
            out.contains("## Spine"),
            out.contains("Pulse"),
        );
        assert!(
            !out.is_empty(),
            "context_inject returned EMPTY for a real prompt — the 0-byte bug (#3048). \
             check() must always push at least the manifest, never nothing."
        );
    }
}
