//! #3864 — reply-delivery correlation, emit leg.
//!
//! Jeff, 2026-08-13: "we should be able to correlate delivery to both claude
//! code and clearing." The spine already chains nudges (requested → emitted →
//! surfaced) and Jeff's inbound input (delivered/surfaced/failed); role
//! REPLIES — the surface Jeff actually reads — emit nothing. This module is
//! the missing first event: `reply.emitted`, stamped at the Stop hook the
//! moment a final reply stands (the transcript IS Claude Code delivery).
//! Clearing's tailer stamps `reply.rendered` with the same content hash;
//! pulse joins the pair and fires `reply.delivery.gap` when the render never
//! comes.
//!
//! The hash is the cross-language join key, contract in
//! `config/reply-hash-fixtures.json` (the word-cap-fixtures pattern). The two
//! sides read DIFFERENT bytes for the same reply — this extractor joins text
//! blocks with '\n' and keeps the chorus header; the tailer joins with spaces
//! and strips it — so the hash canonicalizes both to one form: strip one
//! leading `--- ... ---` header, collapse whitespace runs, trim, then
//! sha256 hex[..16].

use sha2::{Digest, Sha256};

/// Canonical form both surfaces can reach from their own bytes.
fn canonicalize(text: &str) -> String {
    // (1) strip one LEADING chorus header: optional whitespace, '---',
    // anything (incl. newlines) up to the next '---', trailing whitespace.
    let t = text.trim_start();
    let stripped: &str = if let Some(rest) = t.strip_prefix("---") {
        match rest.find("---") {
            Some(i) => rest[i + 3..].trim_start(),
            None => t,
        }
    } else {
        t
    };
    // (2) collapse every whitespace run to a single space; (3) trim.
    stripped.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// #4445 — what gemini-cli's AfterAgent sends as prompt_response when the turn
/// produced no text. Abby's 2026-10-08 06:39 "reply" in the Clearing was this.
const GEMINI_NO_TEXT: &str = "[no response text]";

/// The text of the LAST Gemini message in a gemini-cli chat file, or None when
/// that message is empty (the turn ended on tools) — never an older reply.
fn last_gemini_text(path: &str) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let last = content
        .lines()
        .rev()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .find(|v| v.get("type").and_then(|t| t.as_str()) == Some("gemini"))?;
    last.get("content")
        .and_then(|c| c.as_str())
        .filter(|t| !t.trim().is_empty())
        .map(str::to_string)
}

/// Join key for one reply across surfaces. sha256(canonicalize(text)), hex[..16].
pub fn content_hash(text: &str) -> String {
    let mut h = Sha256::new();
    h.update(canonicalize(text).as_bytes());
    let hex = format!("{:x}", h.finalize());
    hex[..16].to_string()
}

/// #4445 — the reply a turn ended with: Claude's last assistant text from its
/// transcript file; for Gemini, its last message in its own chat file. Gemini's
/// AfterAgent `prompt_response` joins every model turn of the prompt with
/// spaces (Abby's 10-07 20:41 reply went out doubled), so it is only the
/// fallback when the chat file cannot be read.
///
/// #4882 — Claude Code's Stop input carries the turn's final text as
/// `last_assistant_message`. The transcript is written asynchronously and lags
/// at Stop, so reading it re-sent the previous turn's reply (Wren 11-03 15:55)
/// or a mid-turn line (15:56). When the field is present it is the only source:
/// empty means the turn wrote no text and nothing is published. The transcript
/// is read only by runtimes that do not send the field.
pub fn reply_text(raw: &serde_json::Value) -> Option<String> {
    if let Some(v) = raw.get("last_assistant_message") {
        return v.as_str().filter(|t| !t.trim().is_empty()).map(str::to_string);
    }
    raw.get("transcript_path")
        .and_then(|v| v.as_str())
        .and_then(turn_text_from_transcript)
        .or_else(|| {
            raw.get("gemini_transcript_path")
                .and_then(|v| v.as_str())
                .and_then(last_gemini_text)
        })
        .or_else(|| {
            raw.get("prompt_response")
                .and_then(|v| v.as_str())
                // gemini-cli hands this placeholder when a turn produced no text
                .filter(|t| !t.trim().is_empty() && t.trim() != GEMINI_NO_TEXT)
                .map(str::to_string)
        })
}

/// #4882 — where `reply_text` reads from, for the `reply.source` spine event.
pub fn reply_source(raw: &serde_json::Value) -> &'static str {
    if raw.get("last_assistant_message").is_some() {
        "stop_payload"
    } else if raw.get("transcript_path").is_some() {
        "transcript"
    } else {
        "other"
    }
}

/// #4882 — a turn's own text from the transcript: the assistant text written
/// after the turn's prompt (the last user entry that is not a tool result).
/// A lagging transcript that holds no text past that prompt yields nothing,
/// never the previous turn's reply.
pub fn turn_text_from_transcript(path: &str) -> Option<String> {
    // Widen only until the turn's prompt is in the window (8 MB covers almost
    // every turn; Silas's transcript is 73 MB).
    for w in [8 * 1024 * 1024, 64 * 1024 * 1024, u64::MAX] {
        let mut f = std::fs::File::open(path).ok()?;
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        let lines = crate::session_cache::tail_lines_within(&mut f, usize::MAX, w);
        let has_prompt = lines
            .iter()
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .any(|v| is_prompt(&v));
        if has_prompt {
            return turn_text_in(&lines);
        }
        if w >= len {
            return None;
        }
    }
    None
}

fn is_prompt(v: &serde_json::Value) -> bool {
    let role = v.get("message").and_then(|m| m.get("role")).and_then(|r| r.as_str());
    if v.get("type").and_then(|t| t.as_str()) != Some("user") && role != Some("user") {
        return false;
    }
    match v.get("message").and_then(|m| m.get("content")) {
        Some(serde_json::Value::String(_)) => true,
        Some(serde_json::Value::Array(arr)) => !arr
            .iter()
            .any(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_result")),
        _ => false,
    }
}

fn turn_text_in(lines: &[String]) -> Option<String> {
    let parsed: Vec<serde_json::Value> =
        lines.iter().filter_map(|l| serde_json::from_str(l).ok()).collect();
    let start = parsed.iter().rposition(is_prompt)?;
    let mut last: Option<String> = None;
    for v in &parsed[start + 1..] {
        let msg = v.get("message").unwrap_or(v);
        if msg.get("role").and_then(|r| r.as_str()) != Some("assistant")
            && v.get("type").and_then(|t| t.as_str()) != Some("assistant")
        {
            continue;
        }
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
