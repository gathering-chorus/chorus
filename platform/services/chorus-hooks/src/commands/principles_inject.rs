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
use std::time::Duration;

const DEFAULT_API_URL: &str = "http://localhost:3340/api/loom/principles";
const DEFAULT_CACHE_PATH: &str = "/tmp/principles-cache.json";

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

pub fn fetch() -> FetchResult {
    if let Ok(path) = std::env::var("CHORUS_PRINCIPLES_FIXTURE_FILE") {
        match fs::read_to_string(&path) {
            Ok(body) => return parse_or_unavailable(&body),
            Err(e) => return FetchResult::Unavailable(format!("fixture read: {}", e)),
        }
    }

    // #2477 — typed MCP surface replaces direct HTTP. Falls back to cache on
    // MCP unreachable / parse error (same fold as the prior HTTP path).
    let role = std::env::var("CHORUS_ROLE").unwrap_or_else(|_| "shim".to_string());
    let mcp_base = std::env::var("CHORUS_MCP_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:3340".to_string());

    let session = match crate::mcp_client::init_session(&mcp_base, &role) {
        Ok(s) => s,
        Err(e) => return fallback_to_cache(format!("mcp init: {}", e)),
    };

    // #2477 — spine emit at the caller layer (mcp_client is dep-light by design).
    let _ = crate::chorus_log::run_silent(&[
        "mcp.tool.invoked".to_string(),
        role.clone(),
        "tool=chorus_principles_list".to_string(),
        "source=shim".to_string(),
    ]);

    let result = match crate::mcp_client::call_tool(
        &session,
        "chorus_principles_list",
        serde_json::json!({}),
    ) {
        Ok(r) => r,
        Err(e) => return fallback_to_cache(format!("mcp call_tool: {}", e)),
    };

    // The chorus_principles_list tool returns a text content block; parse it
    // back into the canonical envelope so the rest of principles_inject works
    // unchanged. The text shape is "<N> principles:\n- <Label> (<id>) — <comment>\n- ...".
    let text = result
        .get("content")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("");

    let principles = parse_tool_text(text);
    if principles.is_empty() {
        return FetchResult::EmptyFromApi;
    }

    // Update cache with a JSON-shaped representation for the existing
    // fallback_to_cache path. Uses the same envelope shape parse_or_unavailable
    // expects, so future Stale fallback works.
    if let Ok(envelope) = serde_json::to_string(&serde_json::json!({
        "data": {"principles": principles}
    })) {
        let _ = fs::write(cache_path(), envelope);
    }
    FetchResult::Fresh(principles)
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
