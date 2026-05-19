//! #2450 — Live principles injection into SessionStart envelope.
//!
//! Boot fetches the canonical principle set from /api/loom/principles, renders
//! a `## Principles (live from graph)` section into additionalContext, writes
//! a sibling hash file for cross-role drift detection, and falls back to a
//! last-known-good cache when the API is unavailable.
//!
//! Test override: CHORUS_PRINCIPLES_FIXTURE_FILE bypasses HTTP and reads the
//! response from disk. CHORUS_PRINCIPLES_API_URL overrides the endpoint.
//! CHORUS_PRINCIPLES_CACHE_FILE overrides cache path.
//!
//! Fail-loud rule: empty principle set surfaces a banner in content AND a
//! session.principles.empty spine event, but does NOT exit non-zero — a dead
//! session is worse than a degraded session with a visible alarm.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;

const DEFAULT_API_URL: &str = "http://localhost:3340/api/loom/principles";
const DEFAULT_CACHE_PATH: &str = "/tmp/principles-cache.json";
// #3007 — MCP transport moved off chorus-api (:3340) to chorus-mcp (:3341) in
// #2998. Default must target the dedicated server; the chorus-api /mcp mount
// returns 404 and every session-start falls into Stale fallback. The constant
// is named so a future port move is a one-line edit + one-line test update.
const DEFAULT_MCP_BASE: &str = "http://localhost:3341";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Principle {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub comment: String,
    #[serde(default, rename = "techReading")]
    pub tech_reading: String,
    #[serde(default, rename = "jeffReading")]
    pub jeff_reading: String,
    #[serde(default)]
    pub order: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct ApiEnvelope {
    data: ApiData,
}
#[derive(Debug, Deserialize)]
struct ApiData {
    principles: Vec<Principle>,
}

pub enum FetchResult {
    Fresh(Vec<Principle>),
    Stale(Vec<Principle>),
    EmptyFromApi,
    Unavailable(String),
}

pub fn api_url() -> String {
    std::env::var("CHORUS_PRINCIPLES_API_URL").unwrap_or_else(|_| DEFAULT_API_URL.to_string())
}

pub fn cache_path() -> String {
    std::env::var("CHORUS_PRINCIPLES_CACHE_FILE").unwrap_or_else(|_| DEFAULT_CACHE_PATH.to_string())
}

pub fn hash_path(role: &str) -> String {
    format!("/tmp/session-start-{}-principles.hash", role)
}

/// #2964 (Silas chorus-health ask): retry policy for the principles fetch.
/// Without retries, transient MCP errors at session boot (chorus-api restarts,
/// slow startup races) silently flip the inject to Stale fallback. Loki shows
/// 67+ stale events in a 24h window before the fix. Retrying with backoff
/// closes most of those — only genuinely unreachable API still falls back.
const MAX_FETCH_RETRIES: u32 = 3;
const INITIAL_BACKOFF_MS: u64 = 200;
const BACKOFF_MULTIPLIER: u64 = 2;

pub fn fetch() -> FetchResult {
    if let Ok(path) = std::env::var("CHORUS_PRINCIPLES_FIXTURE_FILE") {
        match fs::read_to_string(&path) {
            Ok(body) => return parse_or_unavailable(&body),
            Err(e) => return FetchResult::Unavailable(format!("fixture read: {}", e)),
        }
    }

    // #2477 — typed MCP surface replaces direct HTTP.
    // #2964 — wrap the MCP path in a retry-with-backoff loop. Transient errors
    // (init failure, call_tool failure) are common at session boot when the
    // chorus-api process is restarting or slow to bind. Single-attempt → cache
    // produces Stale events that pollute Jeff's signal channel.
    let role = std::env::var("CHORUS_ROLE").unwrap_or_else(|_| "shim".to_string());
    let mcp_base = std::env::var("CHORUS_MCP_BASE_URL")
        .unwrap_or_else(|_| DEFAULT_MCP_BASE.to_string());

    let mut last_error: Option<String> = None;
    let mut backoff_ms = INITIAL_BACKOFF_MS;

    for attempt in 0..MAX_FETCH_RETRIES {
        match try_fetch_once(&mcp_base, &role) {
            Ok(result) => return result,
            Err(e) => {
                last_error = Some(e.clone());
                // Final attempt — no point sleeping again before falling to cache.
                if attempt + 1 == MAX_FETCH_RETRIES {
                    break;
                }
                // Spine event for visibility — operators can see retry pressure
                // without it polluting the stale-event count.
                let _ = crate::chorus_log::run_silent(&[
                    "session.principles.retry".to_string(),
                    role.clone(),
                    format!("attempt={}", attempt + 1),
                    format!("error={}", e),
                ]);
                std::thread::sleep(std::time::Duration::from_millis(backoff_ms));
                backoff_ms = backoff_ms.saturating_mul(BACKOFF_MULTIPLIER);
            }
        }
    }

    // All retries exhausted — fall back to cache + emit Stale.
    fallback_to_cache(last_error.unwrap_or_else(|| "unknown".to_string()))
}

/// One attempt at the full MCP fetch path. Returns Ok(FetchResult) on success
/// (either Fresh or EmptyFromApi — both are non-error terminal outcomes), or
/// Err(reason) on failure that should trigger a retry. #2964 retry-with-backoff.
fn try_fetch_once(mcp_base: &str, role: &str) -> Result<FetchResult, String> {
    let session = crate::mcp_client::init_session(mcp_base, role)
        .map_err(|e| format!("mcp init: {}", e))?;

    let _ = crate::chorus_log::run_silent(&[
        "mcp.tool.invoked".to_string(),
        role.to_string(),
        "tool=chorus_principles_list".to_string(),
        "source=shim".to_string(),
    ]);

    let result = crate::mcp_client::call_tool(
        &session,
        "chorus_principles_list",
        serde_json::json!({}),
    )
    .map_err(|e| format!("mcp call_tool: {}", e))?;

    // #3010 — prefer structuredContent.principles (JSON array) over the
    // text content block. The text-parse path (parse_tool_text below) used
    // a greedy rfind('(') for the id, which fragmented principles whose
    // comments contained parens (e.g. Hemenway catch-and-store's
    // "(in slope, charge, temperature, or otherwise)" captured a comment
    // fragment as the id, dropping three real principles). When the MCP
    // server returns structuredContent we deserialize directly, no parse.
    //
    // text-parse path retained as fallback for one rollout window:
    // covers older chorus-mcp builds that haven't shipped the
    // structuredContent path yet. Retire parse_tool_text in a follow-on
    // card once chorus-mcp is fully shipped (#3010 AC4).
    let principles: Vec<Principle> =
        if let Some(arr) = result.get("structuredContent").and_then(|sc| sc.get("principles")).and_then(|p| p.as_array()) {
            arr.iter()
                .filter_map(|v| serde_json::from_value::<Principle>(v.clone()).ok())
                .collect()
        } else {
            // Fallback: text content block parse. The text shape is
            // "<N> principles:\n- <Label> (<id>) — <comment>\n- ...".
            let text = result
                .get("content")
                .and_then(|c| c.get(0))
                .and_then(|c| c.get("text"))
                .and_then(|t| t.as_str())
                .unwrap_or("");
            parse_tool_text(text)
        };

    if principles.is_empty() {
        return Ok(FetchResult::EmptyFromApi);
    }

    // Update cache with a JSON-shaped representation for the existing
    // fallback_to_cache path. Uses the same envelope shape parse_or_unavailable
    // expects, so future Stale fallback works.
    if let Ok(envelope) = serde_json::to_string(&serde_json::json!({
        "data": {"principles": principles}
    })) {
        let _ = fs::write(cache_path(), envelope);
    }
    Ok(FetchResult::Fresh(principles))
}

/// Parse the chorus_principles_list tool's text output back into structured
/// records. Format produced by the MCP tool: "N principles:\n- Label (id) — comment".
fn parse_tool_text(text: &str) -> Vec<Principle> {
    let mut principles = Vec::new();
    for line in text.lines() {
        // Each principle line looks like "- **Label** (id) — comment" or
        // "- Label (id) — comment". Strip leading "-" + bold markers.
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("- ") else { continue };
        // Remove ** if present
        let rest = rest.trim_start_matches("**");

        // Pull out (id) — find the LAST `(...)` before any em-dash.
        let (label_part, after_paren) = match rest.rfind('(') {
            Some(idx) => {
                let close = rest[idx..].find(')').map(|c| idx + c);
                if let Some(c) = close {
                    (&rest[..idx], &rest[c + 1..])
                } else {
                    continue;
                }
            }
            None => continue,
        };
        let id_open = match rest.rfind('(') { Some(i) => i, None => continue };
        let id_close = match rest[id_open..].find(')') { Some(c) => id_open + c, None => continue };
        let id = rest[id_open + 1..id_close].trim().to_string();
        if id.is_empty() {
            continue;
        }

        let label = label_part.trim_end_matches("**").trim().trim_end_matches(' ').to_string();

        // Comment after `— ` (em-dash) or `- ` if present
        let comment = after_paren
            .trim_start_matches(' ')
            .trim_start_matches('—')
            .trim_start_matches('-')
            .trim_start()
            .to_string();

        principles.push(Principle {
            id,
            label,
            comment,
            tech_reading: String::new(),
            jeff_reading: String::new(),
            order: serde_json::Value::Null,
        });
    }
    principles
}

fn parse_or_unavailable(body: &str) -> FetchResult {
    match serde_json::from_str::<ApiEnvelope>(body) {
        Ok(env) => {
            if env.data.principles.is_empty() {
                FetchResult::EmptyFromApi
            } else {
                FetchResult::Fresh(env.data.principles)
            }
        }
        Err(e) => FetchResult::Unavailable(format!("parse: {}", e)),
    }
}

fn fallback_to_cache(reason: String) -> FetchResult {
    match fs::read_to_string(cache_path()) {
        Ok(body) => match serde_json::from_str::<ApiEnvelope>(&body) {
            Ok(env) if !env.data.principles.is_empty() => FetchResult::Stale(env.data.principles),
            _ => FetchResult::Unavailable(reason),
        },
        Err(_) => FetchResult::Unavailable(reason),
    }
}

/// SHA256 hex over sorted principle ids — stable across run order, sensitive
/// to set membership. Sibling hash to #2311's protocol_core_hash.
pub fn hash_principles(principles: &[Principle]) -> String {
    let mut ids: Vec<&str> = principles.iter().map(|p| p.id.as_str()).collect();
    ids.sort();
    let mut h = Sha256::new();
    for id in ids {
        h.update(id.as_bytes());
        h.update(b"\0");
    }
    format!("{:x}", h.finalize())
}

pub fn render_section(principles: &[Principle], stale: bool) -> String {
    let mut s = String::new();
    s.push_str("\n## Principles (live from graph)\n\n");
    if stale {
        s.push_str("⚠ STALE — Principles API unreachable; rendered from last-known-good cache.\n\n");
    }
    s.push_str(&format!(
        "Source: {} — {} principle{}.\n\n",
        api_url(),
        principles.len(),
        if principles.len() == 1 { "" } else { "s" }
    ));
    for p in principles {
        if p.label.is_empty() {
            s.push_str(&format!("- **{}**", p.id));
        } else {
            s.push_str(&format!("- **{}** ({})", p.label, p.id));
        }
        if !p.comment.is_empty() {
            s.push_str(&format!(" — {}", p.comment));
        }
        s.push('\n');
    }
    s
}

pub fn render_empty_banner() -> String {
    "\n## Principles (live from graph)\n\n\
     ⚠ ALARM — /api/loom/principles returned an empty set. Graph state is broken; \
     boot continuing without principle set. Investigate immediately.\n\n"
        .to_string()
}

pub fn render_unavailable_banner(reason: &str) -> String {
    format!(
        "\n## Principles (live from graph)\n\n\
         ⚠ Principles API unreachable, no cache available. Reason: {}. Booting without principles.\n\n",
        reason
    )
}

pub fn write_hash(role: &str, hash: &str) -> std::io::Result<()> {
    fs::write(hash_path(role), hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    // #3007 regression pin: chorus-mcp split off chorus-api in #2998. If this
    // default drifts back to :3340, every session-start emits
    // session.principles.stale on the 404 from chorus-api's removed /mcp mount.
    #[test]
    fn default_mcp_base_points_to_chorus_mcp_port_3341() {
        assert_eq!(DEFAULT_MCP_BASE, "http://localhost:3341");
    }

    // #3010 AC3 — receipt: canonical Hemenway principle shape from
    // roles/silas/ontology/chorus.ttl. Comment contains nested parens.
    fn hemenway_fixture() -> serde_json::Value {
        serde_json::json!({
            "id": "hemenway-catch-and-store",
            "label": "Catch and store energy and materials",
            "comment": "Identify, collect, and hold useful flows. Every cycle is an opportunity for yield; every gradient (in slope, charge, temperature, or otherwise) is an opportunity for energy."
        })
    }

    // #3010 AC2 — when the MCP response includes structuredContent.principles,
    // the client deserializes the JSON array directly. No prose parse.
    #[test]
    fn structured_content_principles_deserialize_directly() {
        let result = serde_json::json!({
            "content": [{"type": "text", "text": "1 principles:\n- Catch and store energy and materials (hemenway-catch-and-store) — ..."}],
            "structuredContent": {
                "principles": [hemenway_fixture()]
            }
        });

        let principles: Vec<Principle> = result
            .get("structuredContent")
            .and_then(|sc| sc.get("principles"))
            .and_then(|p| p.as_array())
            .expect("structuredContent.principles should be present")
            .iter()
            .filter_map(|v| serde_json::from_value::<Principle>(v.clone()).ok())
            .collect();

        assert_eq!(principles.len(), 1, "exactly one principle in fixture");
    }

    // #3010 AC3 — the Hemenway id is intact via the structuredContent path,
    // not fragmented to the comment's nested-paren content.
    #[test]
    fn hemenway_id_intact_via_structured_content() {
        let p: Principle = serde_json::from_value(hemenway_fixture()).expect("deserialize");
        assert_eq!(
            p.id, "hemenway-catch-and-store",
            "Hemenway id should be the canonical id, not a comment fragment"
        );
        assert_ne!(
            p.id, "in slope, charge, temperature, or otherwise",
            "regression guard: comment-fragment id is exactly the parse_tool_text bug shape"
        );
        assert_eq!(p.label, "Catch and store energy and materials");
    }

    // #3010 — regression pin documenting why we route around parse_tool_text
    // when structuredContent is present. Greedy rfind('(') in parse_tool_text
    // captures the LAST open-paren in the line, which falls inside the comment
    // for any principle whose comment contains parens. This test does NOT
    // assert correctness of parse_tool_text — it documents the buggy behavior
    // so the structuredContent path's value is visible.
    #[test]
    fn parse_tool_text_fragments_hemenway_id_documenting_bug() {
        let prose = "1 principles:\n- Catch and store energy and materials (hemenway-catch-and-store) — Identify, collect, and hold useful flows. Every cycle is an opportunity for yield; every gradient (in slope, charge, temperature, or otherwise) is an opportunity for energy.";
        let parsed = parse_tool_text(prose);
        // The fallback parser produces ONE principle but with the WRONG id —
        // captured from the comment's nested parens, not the real id.
        // structuredContent path bypasses this entirely.
        assert_eq!(parsed.len(), 1, "parse_tool_text recovers one entry from the prose");
        assert_eq!(
            parsed[0].id, "in slope, charge, temperature, or otherwise",
            "buggy id-fragment from rfind('(') greedy match — this is the bug #3010 routes around"
        );
        assert_ne!(
            parsed[0].id, "hemenway-catch-and-store",
            "the canonical id never survives parse_tool_text when comment has parens"
        );
    }
}
