//! #3203 — context-inject FORCE (HIP-001, the forcing pattern applied to the inject).
//!
//! The inject surfaces records every prompt but can't compel engagement (an IGNORE).
//! This is the Stop-hook verdict that turns it into a FORCE: did the response engage
//! what the inject surfaced this turn? Two ways to satisfy it — agent reactions:
//!   • USE one  (👍) — a distinctive token from a surfaced record shows up in the response
//!   • DISMISS  (✋) — an explicit `dismiss-inject: <reason>` marker
//! Neither → the turn is blocked (🛑). Deliberately UN-CLEVER (token-overlap + a marker);
//! the teeth are the block + the logged decision, not a clever engagement-detector
//! (perfect engagement detection is the unsolvable global-judgment — don't chase it).

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub enum EngagementVerdict {
    Pass,
    Block { reason: String },
}

/// Read the LAST assistant message's text from a Claude Code transcript JSONL.
/// Defensive over shape: each line is one JSON object; an assistant turn looks like
/// {"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":..}]}}.
/// Returns the concatenated text of the last assistant entry, or None. (AC1 input —
/// "what did I actually say this turn.")
pub fn last_assistant_text(transcript_path: &str) -> Option<String> {
    let content = std::fs::read_to_string(transcript_path).ok()?;
    let mut last: Option<String> = None;
    for line in content.lines() {
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let is_assistant = v.get("type").and_then(|t| t.as_str()) == Some("assistant")
            || v.get("message")
                .and_then(|m| m.get("role"))
                .and_then(|r| r.as_str())
                == Some("assistant");
        if !is_assistant {
            continue;
        }
        let msg = v.get("message").unwrap_or(&v);
        let text = match msg.get("content") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(serde_json::Value::Array(arr)) => arr
                .iter()
                .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("\n"),
            _ => continue,
        };
        if !text.trim().is_empty() {
            last = Some(text);
        }
    }
    last
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

/// Default per-session surfaced-record store (durable; never /tmp).
fn surfaced_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    Path::new(&home).join(".chorus").join("inject-surfaced")
}

/// Record what the inject surfaced this turn so the Stop gate can read it (AC1 input).
/// `dir`-explicit form for tests; the env-default wrapper is `record_surfaced`.
pub fn record_surfaced_in(dir: &Path, session_id: &str, records: &[String]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{}.json", sanitize(session_id)));
    let json = serde_json::to_string(records).unwrap_or_else(|_| "[]".into());
    std::fs::write(path, json)
}

pub fn read_surfaced_in(dir: &Path, session_id: &str) -> Vec<String> {
    let path = dir.join(format!("{}.json", sanitize(session_id)));
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .unwrap_or_default()
}

/// Env-default wrappers used by the live inject (write) + Stop gate (read).
pub fn record_surfaced(session_id: &str, records: &[String]) {
    let _ = record_surfaced_in(&surfaced_dir(), session_id, records);
}
pub fn read_surfaced(session_id: &str) -> Vec<String> {
    read_surfaced_in(&surfaced_dir(), session_id)
}

/// The marker an agent stamps to consciously skip surfaced context (the ✋).
pub const DISMISS_MARKER: &str = "dismiss-inject:";
/// A bare marker with no real reason doesn't count — the escape hatch needs a reason.
const MIN_REASON_ALNUM: usize = 8;
/// Tokens shorter than this are too common to count as "used" (drops "oz", "the").
const MIN_TOKEN_LEN: usize = 4;

/// Pure verdict: given the records the inject surfaced this turn and the agent's
/// final response, decide whether the turn may complete.
pub fn inject_engagement_verdict(surfaced: &[String], response: &str) -> EngagementVerdict {
    // Nothing surfaced → never block a bare turn (AC2).
    if surfaced.is_empty() {
        return EngagementVerdict::Pass;
    }
    let resp_lower = response.to_lowercase();

    // ✋ explicit dismissal WITH a reason (AC3) — marker followed by real text.
    if let Some(idx) = resp_lower.find(DISMISS_MARKER) {
        let after = &response[idx + DISMISS_MARKER.len()..];
        let alnum = after.chars().filter(|c| c.is_alphanumeric()).count();
        if alnum >= MIN_REASON_ALNUM {
            return EngagementVerdict::Pass;
        }
        // bare marker, no reason → falls through (still must use, or block).
    }

    // 👍 "used": un-clever token overlap — a distinctive token from any surfaced
    // record appears in the response.
    let used = surfaced.iter().any(|rec| {
        rec.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| t.len() >= MIN_TOKEN_LEN)
            .any(|tok| resp_lower.contains(tok))
    });
    if used {
        return EngagementVerdict::Pass;
    }

    // 🛑 neither used nor dismissed — block, naming what's unaddressed (AC1).
    let names: Vec<&str> = surfaced.iter().map(String::as_str).take(3).collect();
    EngagementVerdict::Block {
        reason: format!(
            "context-inject surfaced {} record(s) you neither used nor dismissed: [{}]. \
             Use one, or stamp 'dismiss-inject: <reason>'.",
            surfaced.len(),
            names.join(" | ")
        ),
    }
}
