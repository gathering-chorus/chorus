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

#[cfg(test)]
mod tests_4882 {
    use super::reply_text;
    use serde_json::json;

    /// A transcript as it stands at Stop: the previous turn's reply is there,
    /// this turn's final text is not yet written (it lags), only its interim line.
    fn lagging_transcript(dir: &std::path::Path) -> String {
        let p = dir.join("t.jsonl");
        let lines = [
            json!({"type":"user","message":{"role":"user","content":"earlier prompt"}}),
            json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"previous turn's reply"}]}}),
            json!({"type":"user","message":{"role":"user","content":"whats next"}}),
            json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"I'm checking what's still open."}]}}),
        ];
        std::fs::write(&p, lines.iter().map(|l| l.to_string() + "\n").collect::<String>()).unwrap();
        p.to_string_lossy().to_string()
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("rd-4882-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_turn_s_final_text_is_published_not_the_lagging_transcript_4882() {
        let d = tmp("final");
        let tp = lagging_transcript(&d);
        let raw = json!({"transcript_path": tp, "last_assistant_message": "#4882 is next."});
        assert_eq!(reply_text(&raw).as_deref(), Some("#4882 is next."));
        // negative proof (#3734): the old lookup, transcript only, publishes the interim line
        assert_eq!(crate::hooks::inject_force::last_assistant_text(&tp).as_deref(), Some("I'm checking what's still open."));
    }

    #[test]
    fn a_turn_with_no_text_publishes_nothing_never_the_previous_reply_4882() {
        let d = tmp("empty");
        let p = d.join("t.jsonl");
        std::fs::write(&p, json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"previous turn's reply"}]}}).to_string() + "\n").unwrap();
        let tp = p.to_string_lossy().to_string();
        for empty in [json!(""), json!("   "), json!(null)] {
            let raw = json!({"transcript_path": tp, "last_assistant_message": empty});
            assert_eq!(reply_text(&raw), None, "empty final text must not fall back to the transcript");
        }
        // negative proof: the old whole-transcript lookup re-sends the previous reply
        assert_eq!(crate::hooks::inject_force::last_assistant_text(&tp).as_deref(), Some("previous turn's reply"));
    }

    #[test]
    fn a_rewrite_after_a_stop_gate_publishes_the_rewrite_only_4882() {
        let d = tmp("rewrite");
        let tp = lagging_transcript(&d);
        // the continuation's Stop carries the rewrite, while the transcript still ends on the rejected draft
        let raw = json!({"transcript_path": tp, "stop_hook_active": true, "last_assistant_message": "short rewrite"});
        assert_eq!(reply_text(&raw).as_deref(), Some("short rewrite"));
    }

    #[test]
    fn without_the_field_a_lagging_transcript_yields_nothing_past_the_prompt_4882() {
        let d = tmp("fallback");
        let p = d.join("t.jsonl");
        let lines = [
            json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"previous turn's reply"}]}}),
            json!({"type":"user","message":{"role":"user","content":"whats next"}}),
            json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","name":"Bash"}]}}),
            json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"ok"}]}}),
        ];
        std::fs::write(&p, lines.iter().map(|l| l.to_string() + "\n").collect::<String>()).unwrap();
        let tp = p.to_string_lossy().to_string();
        assert_eq!(reply_text(&json!({"transcript_path": tp})), None, "no text past the turn's prompt: publish nothing");
        assert_eq!(crate::hooks::inject_force::last_assistant_text(&tp).as_deref(), Some("previous turn's reply"), "negative proof: old lookup");
        // and text past the prompt (after a tool result) is the turn's own
        let mut more = std::fs::read_to_string(&p).unwrap();
        more.push_str(&(json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"answer"}]}}).to_string() + "\n"));
        std::fs::write(&p, more).unwrap();
        assert_eq!(reply_text(&json!({"transcript_path": tp})).as_deref(), Some("answer"));
    }

    #[test]
    fn gemini_path_is_unchanged_4882() {
        let raw = json!({"prompt_response": "gemini reply"});
        assert_eq!(reply_text(&raw).as_deref(), Some("gemini reply"));
    }
}
