// @domain: spine — placed by chorus-crawl's domain rules (#4538)
//! #2450 — Live principles injection into SessionStart envelope.
//!
//! Boot fetches the canonical principle set (athena-make /v1/principles/principles, #4353), renders
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

const DEFAULT_API_URL: &str = "http://localhost:3360/v1/principles/principles";
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
/// #4515 — the door answers `data: [ {name, label, …} ]`; the last-known-good
/// cache (and the test fixtures) keep `data: {principles: [ {id, …} ]}`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ApiData {
    Door(Vec<DoorPrinciple>),
    Cached { principles: Vec<Principle> },
}
#[derive(Debug, Deserialize)]
struct DoorPrinciple {
    name: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    comment: String,
    #[serde(default, rename = "techReading")]
    tech_reading: String,
    #[serde(default, rename = "jeffReading")]
    jeff_reading: String,
    #[serde(default)]
    order: serde_json::Value,
}
impl ApiData {
    fn principles(self) -> Vec<Principle> {
        match self {
            ApiData::Cached { principles } => principles,
            ApiData::Door(rows) => rows
                .into_iter()
                .filter(|r| !r.name.is_empty())
                .map(|r| Principle { id: r.name, label: r.label, comment: r.comment, tech_reading: r.tech_reading, jeff_reading: r.jeff_reading, order: r.order })
                .collect(),
        }
    }
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
    crate::shared::state_paths::session_start_file(role, "-principles.hash")
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

    // #4515 — the door, read directly: the old MCP server and its
    // principles list tool are gone. Retries as before (#2964).
    let role = std::env::var("CHORUS_ROLE").unwrap_or_else(|_| "shim".to_string());
    let url = api_url();

    let mut last_error: Option<String> = None;
    let mut backoff_ms = INITIAL_BACKOFF_MS;

    for attempt in 0..MAX_FETCH_RETRIES {
        match try_fetch_once(&url) {
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

/// One GET of the door's principles. Ok = a terminal answer (Fresh or
/// EmptyFromApi); Err = a failure worth a retry. #2964 retry-with-backoff.
fn try_fetch_once(url: &str) -> Result<FetchResult, String> {
    let resp = ureq::get(&format!("{url}?limit=500"))
        .timeout(std::time::Duration::from_secs(3))
        .call()
        .map_err(|e| format!("door GET: {e}"))?;
    let body = resp.into_string().map_err(|e| format!("door body: {e}"))?;
    let principles = serde_json::from_str::<ApiEnvelope>(&body)
        .map_err(|e| format!("door parse: {e}"))?
        .data
        .principles();
    if principles.is_empty() {
        return Ok(FetchResult::EmptyFromApi);
    }
    // The cache keeps the {principles: [...]} shape fallback_to_cache reads.
    if let Ok(envelope) = serde_json::to_string(&serde_json::json!({ "data": { "principles": principles } })) {
        let _ = fs::write(cache_path(), envelope);
    }
    Ok(FetchResult::Fresh(principles))
}

fn parse_or_unavailable(body: &str) -> FetchResult {
    match serde_json::from_str::<ApiEnvelope>(body) {
        Ok(env) => {
            let principles = env.data.principles();
            if principles.is_empty() {
                FetchResult::EmptyFromApi
            } else {
                FetchResult::Fresh(principles)
            }
        }
        Err(e) => FetchResult::Unavailable(format!("parse: {}", e)),
    }
}

fn fallback_to_cache(reason: String) -> FetchResult {
    match fs::read_to_string(cache_path()) {
        Ok(body) => match serde_json::from_str::<ApiEnvelope>(&body) {
            Ok(env) => {
                let principles = env.data.principles();
                if principles.is_empty() { FetchResult::Unavailable(reason) } else { FetchResult::Stale(principles) }
            }
            Err(_) => FetchResult::Unavailable(reason),
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
     ⚠ ALARM — the principles API returned an empty set. Graph state is broken; \
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

    // #4515 — the door's own answer: `data` is the list of rows, id is `name`.
    // The Hemenway comment holds nested parens: the old MCP text parse took a
    // comment fragment for the id (#3010); reading rows has no parse to break.
    const DOOR: &str = r#"{"apiVersion":"v1","kind":"Principle","data":[
      {"name":"hemenway-catch-and-store","label":"Catch and store energy and materials","status":"",
       "comment":"every gradient (in slope, charge, temperature, or otherwise) is an opportunity","techReading":"t","jeffReading":"j","order":"3"},
      {"name":"","label":"a row with no name is not a principle"}]}"#;

    #[test]
    fn the_doors_rows_become_principles_with_their_names_as_ids() {
        let FetchResult::Fresh(ps) = parse_or_unavailable(DOOR) else { panic!("door rows did not parse") };
        assert_eq!(ps.len(), 1, "the nameless row is dropped");
        assert_eq!(ps[0].id, "hemenway-catch-and-store");
        assert_eq!(ps[0].tech_reading, "t");
    }

    #[test]
    fn the_cache_shape_still_reads() {
        let cached = r#"{"data":{"principles":[{"id":"hemenway-observe","label":"Observe"}]}}"#;
        let FetchResult::Fresh(ps) = parse_or_unavailable(cached) else { panic!("cache shape did not parse") };
        assert_eq!(ps[0].id, "hemenway-observe");
    }

    // NEGATIVE PROOF: a door with no rows is the empty alarm, never Fresh.
    #[test]
    fn an_empty_door_is_empty_not_fresh() {
        assert!(matches!(parse_or_unavailable(r#"{"data":[]}"#), FetchResult::EmptyFromApi));
    }

    #[test]
    fn the_default_source_is_the_door_not_an_mcp_server() {
        assert_eq!(DEFAULT_API_URL, "http://localhost:3360/v1/principles/principles");
    }
}
