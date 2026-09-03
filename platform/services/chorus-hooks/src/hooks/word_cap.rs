//! #3843 — word-cap Stop-hook leg (pairs with #3818's send-path gate).
//!
//! Jeff set a per-role word cap and made the VALUE data (#3838:
//! response.word.cap, resolved through the properties cascade at role scope).
//! Wren's leg gates role-to-role nudges at the messaging-API send path; this
//! leg gates the surface Jeff actually reads — the final reply of a turn — at
//! the Stop hook. Over-cap is rejected WITH THE COUNT, never silently
//! truncated: truncation would cut the answer mid-sentence and hide that it
//! happened.
//!
//! The two legs are two languages (TS / Rust), so the shared thing is not a
//! module — it is `config/word-cap-fixtures.json`. Both counters run every
//! case; if they ever disagree, one of them broke the fixture and Jeff would
//! be bounced on one surface and not the other. The counting rule and its
//! strip ORDER live there; this file only restates the one ordering fact that
//! matters: fences strip first, because a fence can contain backticks and
//! URLs that would otherwise strip early and leave prose behind.
//!
//! The cap is a courtesy to Jeff, not a security control. Every resolver
//! failure path — unreachable, non-2xx, malformed body, missing value, and a
//! non-positive cap (a fat-fingered "0" must not mute the team) — fails OPEN
//! to `FAIL_OPEN_CAP`. Worst case under a store outage is "we're wordy
//! today", never "the team is mute".

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Mirror of the TS leg's FAIL_OPEN_CAP — the value used whenever the graph
/// cannot be read or holds a refusable value.
pub const FAIL_OPEN_CAP: u32 = 100;

/// How long a resolved cap is trusted. Long enough that the store is not on
/// the hot path of every turn; short enough that Jeff editing the property
/// takes effect while he is still thinking about it. (TS: CAP_TTL_MS.)
pub const CAP_TTL: Duration = Duration::from_secs(60);

/// Consecutive Stop-hook blocks allowed for one over-cap reply before the
/// gate stands down (same bounded-in-code contract as nudge_drain's
/// STOP_REFUSAL_CAP — the 2026-07-23 125-fire loop is why this is a constant
/// and not a docstring).
pub const WORD_CAP_REFUSAL_CAP: u32 = 2;

/// Words in the PROSE of a message. `config/word-cap-fixtures.json` is the
/// authoritative case list — this function and the TS `countWords` both run
/// it. Strip order: fenced blocks (an unclosed fence strips to end of text —
/// accepted risk, named in the fixture), then inline code spans, then
/// http(s)/ws(s) URLs. A token counts only if it contains a letter or digit.
/// #4077 — the prose the cap counts and the english-only leg reads: header,
/// fenced blocks, inline code and URLs removed. ONE stripping rule for both.
pub fn prose_only(text: &str) -> String {
    let no_header = strip_leading_header(text);
    let no_fences = strip_fences(&no_header);
    let no_inline = strip_inline_code(&no_fences);
    strip_urls(&no_inline)
}

pub fn count_words(text: &str) -> usize {
    prose_only(text)
        .split_whitespace()
        .filter(|tok| tok.chars().any(|c| c.is_alphanumeric()))
        .count()
}

/// #3854 — the chorus prompt header is METADATA and never counts (Jeff,
/// 2026-08-13: "dont count any metadata"). LEADING only, mirroring the TS
/// leg's /^\s*---[^\n]*---\s*(\n|$)/: a dashed line mid-prose is content —
/// stripping every dashed line would let anyone hide a paragraph in dashes.
fn strip_leading_header(text: &str) -> String {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"^\s*---[^\n]*---\s*(\n|$)").unwrap());
    re.replace(text, " ").into_owned()
}

/// Paired ``` fences strip non-greedily; a trailing unpaired ``` strips to
/// the end of the text (deliberate — the alternative counts a pasted log as
/// prose and bounces a message for showing evidence).
fn strip_fences(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find("```") {
        out.push_str(&rest[..open]);
        out.push(' ');
        match rest[open + 3..].find("```") {
            Some(close) => rest = &rest[open + 3 + close + 3..],
            None => return out, // unclosed: everything after the fence is gone
        }
    }
    out.push_str(rest);
    out
}

/// Inline `code` spans on one line (TS: /`[^`\n]*`/).
fn strip_inline_code(text: &str) -> String {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"`[^`\n]*`").unwrap());
    re.replace_all(text, " ").into_owned()
}

/// URLs strip whole, not word-split — http(s) and ws(s) (Buzz relay
/// addresses already appear in nudges and must not spend Jeff's budget).
fn strip_urls(text: &str) -> String {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE
        .get_or_init(|| regex::Regex::new(r"(?i)\b(?:https?|wss?)://\S+").unwrap());
    re.replace_all(text, " ").into_owned()
}

struct CacheEntry {
    role: String,
    cap: u32,
    at: Instant,
}

static CAP_CACHE: Mutex<Vec<CacheEntry>> = Mutex::new(Vec::new());

/// Resolve the cap for a role from the Properties domain — same endpoint and
/// key as the TS leg, so there is one source of truth, not two:
/// `GET {base}/api/chorus/properties/resolve?key=response.word.cap&scope=role&name={role}`.
///
/// FAIL-OPEN on every failure path; only a successful, positive value is
/// cached (an outage is retried next turn, never remembered as a cap).
pub fn resolve_cap(role: &str, api_base: &str) -> u32 {
    if let Ok(cache) = CAP_CACHE.lock() {
        if let Some(hit) = cache.iter().find(|e| e.role == role) {
            if hit.at.elapsed() < CAP_TTL {
                return hit.cap;
            }
        }
    }
    let url = format!(
        "{api_base}/api/chorus/properties/resolve?key=response.word.cap&scope=role&name={role}"
    );
    let cap = match ureq::get(&url).timeout(Duration::from_millis(1500)).call() {
        Ok(resp) => match resp.into_json::<serde_json::Value>() {
            Ok(body) => {
                let n = body
                    .get("value")
                    .and_then(|v| {
                        v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                    })
                    .unwrap_or(0.0);
                // A non-positive or absent cap is a misconfiguration, not an
                // instruction — refusing it means a fat-fingered "0" cannot
                // mute the team.
                if n.is_finite() && n > 0.0 {
                    Some(n as u32)
                } else {
                    None
                }
            }
            Err(_) => None,
        },
        Err(_) => None,
    };
    match cap {
        Some(cap) => {
            if let Ok(mut cache) = CAP_CACHE.lock() {
                cache.retain(|e| e.role != role);
                cache.push(CacheEntry { role: role.to_string(), cap, at: Instant::now() });
            }
            cap
        }
        None => FAIL_OPEN_CAP,
    }
}

/// Test seam only — the cache is process-global by design.
pub fn clear_cap_cache() {
    if let Ok(mut cache) = CAP_CACHE.lock() {
        cache.clear();
    }
}

/// Over-cap verdict, mirroring the TS `checkCap` message contract.
pub struct OverCap {
    pub words: usize,
    pub cap: u32,
}

pub fn check_cap(text: &str, cap: u32) -> Result<(), OverCap> {
    let words = count_words(text);
    if words <= cap as usize {
        Ok(())
    } else {
        Err(OverCap { words, cap })
    }
}

/// Per-role bounded refusal counter — word-cap's OWN, deliberately not
/// nudge_drain's map: the stop hook calls nudge_drain::reset_refusals(role)
/// on every clean stop, which would wipe a shared counter before the degrade
/// path could ever fire and re-create the #3672 fire loop. A new debt key
/// (i.e. a rewritten reply) resets the count.
static REFUSALS: Mutex<Vec<(String, String, u32)>> = Mutex::new(Vec::new());

/// Returns the number of refusals BEFORE this one (0 = first).
pub fn note_refusal(role: &str, debt_key: &str) -> u32 {
    let mut map = match REFUSALS.lock() {
        Ok(m) => m,
        Err(_) => return u32::MAX, // poisoned lock: degrade, never trap on our own state
    };
    match map.iter_mut().find(|(r, _, _)| r == role) {
        Some(entry) => {
            if entry.1 != debt_key {
                entry.1 = debt_key.to_string();
                entry.2 = 0;
            }
            let before = entry.2;
            entry.2 += 1;
            before
        }
        None => {
            map.push((role.to_string(), debt_key.to_string(), 1));
            0
        }
    }
}

pub fn reset_refusals(role: &str) {
    if let Ok(mut map) = REFUSALS.lock() {
        map.retain(|(r, _, _)| r != role);
    }
}

/// #3854 — the one line injected into every turn's context so the cap is
/// known at WRITE time. The Stop gate stays as backstop; this line exists
/// because the gate cannot unprint an already-streamed draft.
pub fn cap_context_line(cap: u32) -> String {
    format!(
        "Reply cap: {} words of prose (fenced blocks, inline code, URLs, and the header line don't count). Write short the first time — the Stop gate bounces over-cap replies but cannot unprint the draft.",
        cap
    )
}

pub fn block_message(over: &OverCap) -> String {
    format!(
        "word-cap: {} words, cap is {} (response.word.cap for this role). Rewrite the \
         reply shorter and end the turn again — nothing was truncated. Fenced code \
         blocks, inline code, and URLs do not count toward the total, so receipts \
         belong in a fence.",
        over.words, over.cap
    )
}
