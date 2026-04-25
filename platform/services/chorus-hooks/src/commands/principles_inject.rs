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

    let url = api_url();
    let resp = ureq::get(&url)
        .timeout(Duration::from_secs(3))
        .call();

    match resp {
        Ok(r) => match r.into_string() {
            Ok(body) => {
                let parsed = parse_or_unavailable(&body);
                if matches!(parsed, FetchResult::Fresh(_)) {
                    let _ = fs::write(cache_path(), &body);
                }
                parsed
            }
            Err(e) => fallback_to_cache(format!("read body: {}", e)),
        },
        Err(e) => fallback_to_cache(format!("http: {}", e)),
    }
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
