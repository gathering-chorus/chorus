//! Card-directive detector — #2964 AC: substrate-enforced attribution rule.
//!
//! UserPromptSubmit hook companion to `card_approval_responder`. Where the
//! approval responder watches Jeff's prompt for `approve` / `deny` keywords
//! and replays a pending bouncer-refused card, THIS hook watches Jeff's
//! prompt for *card-creation directive* keywords ("make a card", "file a
//! card", "create a card", "add a card") and writes a fresh marker file so
//! the bouncer can detect Jeff-initiated-context on the next `cards add`
//! call and SKIP the approval-ask entirely.
//!
//! The architecture is mirror-image:
//!
//!   Jeff says "approve"            → responder hook → replay pending payload
//!   Jeff says "make a card …"      → directive hook → write marker → bouncer skips
//!
//! Both move agent-judgment OUT of the gate. Today's #2964 audit established
//! that the rule "Jeff said make a card → attribute as Jeff, no bouncer"
//! can't live in agent CLAUDE.md as a discipline — agents disregard it the
//! moment it's tested. The substrate makes the decision instead: prompt
//! pattern matches → marker fires → bouncer reads marker → skip.
//!
//! Marker path: `~/.chorus/pending-directives/<role>.json`
//! Marker freshness: configurable; default 60s. Bouncer reads timestamp.

use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::{Duration, SystemTime};

/// Marker freshness window. Bouncer treats markers older than this as stale
/// (the directive was probably for a different/earlier intent). 60s gives
/// enough time for the agent to compose the card and invoke `cards add` but
/// not so much that a stale directive from earlier in the session leaks.
pub const DIRECTIVE_FRESHNESS_SECS: u64 = 60;

/// Standalone-word directive patterns. Each detects the imperative shape of
/// "user is telling agent to file a card." All require the word "card" near
/// the verb to disambiguate from generic "make / add / file / create"
/// statements that aren't card-related.
static DIRECTIVE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:make|file|create|add|open|draft|cut)\s+(?:a\s+|an\s+|the\s+|me\s+a\s+|a\s+new\s+)?(?:test\s+)?card\b",
    )
    .unwrap()
});

/// Detect a card-creation directive in a UserPromptSubmit prompt.
///
/// Guards (same shape as detect_approval_signal):
/// - Long prompts (>400 chars) are ignored — directives are short; long
///   prompts are usually quoted content or discussion. The 400-char cap is
///   double the approval-detector's because card directives often include
///   a few words of context ("make a card for silas to do X").
/// - Relayed nudge content (`[nudge from`, `[card-approval]`, `[feedback]`,
///   `[demo]`) is ignored — injected substrate text, not Jeff typing.
/// - Quoted/template content (`<…>`) is ignored.
pub fn detect_directive_signal(prompt: &str) -> bool {
    if prompt.is_empty() || prompt.len() > 400 {
        return false;
    }
    let trimmed = prompt.trim();
    if trimmed.starts_with("[nudge from")
        || trimmed.starts_with("[card-approval]")
        || trimmed.starts_with("[feedback]")
        || trimmed.starts_with("[demo]")
        || trimmed.starts_with('<')
    {
        return false;
    }
    DIRECTIVE_PATTERN.is_match(prompt)
}

/// Write the directive marker so the bouncer can skip on the next `cards add`.
/// The marker is JSON with timestamp + the matched prompt excerpt — the bouncer
/// reads timestamp for freshness and excerpt for spine-event attribution.
pub fn write_directive_marker(
    pending_dir: &Path,
    role: &str,
    prompt: &str,
    now: SystemTime,
) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(pending_dir)?;
    let path = pending_dir.join(format!("{}.json", role));
    let ts_ms = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    // Trim the prompt excerpt to keep the marker small.
    let excerpt: String = prompt.chars().take(200).collect();
    let body = serde_json::json!({
        "role": role,
        "ts_ms": ts_ms,
        "prompt_excerpt": excerpt,
    });
    std::fs::write(&path, serde_json::to_string_pretty(&body)?)?;
    Ok(path)
}

/// True if the marker at `path` is younger than DIRECTIVE_FRESHNESS_SECS.
/// Missing/unreadable markers count as stale (no directive context).
pub fn is_marker_fresh(path: &Path, now: SystemTime) -> bool {
    let mtime = match std::fs::metadata(path).and_then(|m| m.modified()) {
        Ok(t) => t,
        Err(_) => return false,
    };
    match now.duration_since(mtime) {
        Ok(age) => age < Duration::from_secs(DIRECTIVE_FRESHNESS_SECS),
        Err(_) => true, // mtime in future — treat as fresh
    }
}

/// Remove the marker after the bouncer consumes it. Best-effort.
pub fn consume_marker(path: &Path) {
    let _ = std::fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn directive_make_a_card() {
        assert!(detect_directive_signal("make a card for silas to do X"));
        assert!(detect_directive_signal("make a card"));
        assert!(detect_directive_signal("Make a Card for me"));
    }

    #[test]
    fn directive_file_create_add() {
        assert!(detect_directive_signal("file a card for the cleanup"));
        assert!(detect_directive_signal("create a card to track this"));
        assert!(detect_directive_signal("add a card for the missing fix"));
    }

    #[test]
    fn directive_with_qualifiers() {
        assert!(detect_directive_signal("make me a card for silas"));
        assert!(detect_directive_signal("make a new card for the cleanup"));
        assert!(detect_directive_signal("draft a card for X"));
        assert!(detect_directive_signal("cut a card for the bug"));
    }

    #[test]
    fn directive_with_test_qualifier() {
        // "make a test card" — explicitly the case Jeff tested earlier today
        assert!(detect_directive_signal("make a test card for me wren"));
        assert!(detect_directive_signal("create a test card"));
    }

    #[test]
    fn no_directive_on_unrelated_make_file_create() {
        // No "card" word → no match
        assert!(!detect_directive_signal("make a sandwich"));
        assert!(!detect_directive_signal("file the report"));
        assert!(!detect_directive_signal("create a new directory"));
        assert!(!detect_directive_signal("add a comment to this"));
    }

    #[test]
    fn no_directive_on_card_without_verb() {
        // "card" appears but not as a directive
        assert!(!detect_directive_signal("the card system is broken"));
        assert!(!detect_directive_signal("what's on the card"));
        assert!(!detect_directive_signal("this card has no AC"));
    }

    #[test]
    fn no_directive_on_long_prompts() {
        let long = format!("{} make a card {}", "x".repeat(250), "y".repeat(250));
        assert!(long.len() > 400);
        assert!(!detect_directive_signal(&long));
    }

    #[test]
    fn no_directive_on_relayed_nudges() {
        assert!(!detect_directive_signal("[nudge from kade] make a card for me"));
        assert!(!detect_directive_signal("[card-approval] please make a card"));
        assert!(!detect_directive_signal("[feedback] should we make a card?"));
        assert!(!detect_directive_signal("[demo] make a card showing the flow"));
    }

    #[test]
    fn no_directive_on_quoted_content() {
        assert!(!detect_directive_signal("<system-reminder>make a card</system-reminder>"));
    }

    #[test]
    fn no_directive_on_empty() {
        assert!(!detect_directive_signal(""));
        assert!(!detect_directive_signal("   "));
    }

    #[test]
    fn write_marker_creates_json_file() {
        let tmp = TempDir::new().unwrap();
        let path = write_directive_marker(tmp.path(), "wren", "make a card for X", SystemTime::now())
            .unwrap();
        assert!(path.exists());
        assert_eq!(path.file_name().unwrap().to_str().unwrap(), "wren.json");
        let body: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(body["role"].as_str(), Some("wren"));
        assert!(body["ts_ms"].as_u64().unwrap() > 0);
        assert_eq!(body["prompt_excerpt"].as_str(), Some("make a card for X"));
    }

    #[test]
    fn write_marker_overwrites_per_role() {
        let tmp = TempDir::new().unwrap();
        write_directive_marker(tmp.path(), "wren", "first directive", SystemTime::now()).unwrap();
        write_directive_marker(tmp.path(), "wren", "second directive", SystemTime::now()).unwrap();
        // Only one wren.json file should exist; second overwrites first
        let entries: Vec<_> = std::fs::read_dir(tmp.path()).unwrap().collect();
        assert_eq!(entries.len(), 1);
        let body: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(tmp.path().join("wren.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(body["prompt_excerpt"].as_str(), Some("second directive"));
    }

    #[test]
    fn marker_fresh_within_window() {
        let tmp = TempDir::new().unwrap();
        let path = write_directive_marker(tmp.path(), "wren", "make a card", SystemTime::now())
            .unwrap();
        assert!(is_marker_fresh(&path, SystemTime::now()));
    }

    #[test]
    fn marker_stale_past_window() {
        let tmp = TempDir::new().unwrap();
        let path = write_directive_marker(tmp.path(), "wren", "make a card", SystemTime::now())
            .unwrap();
        // Set mtime back past the freshness window
        let target = SystemTime::now() - Duration::from_secs(DIRECTIVE_FRESHNESS_SECS + 5);
        let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_modified(target).unwrap();
        assert!(!is_marker_fresh(&path, SystemTime::now()));
    }

    #[test]
    fn marker_missing_is_not_fresh() {
        let tmp = TempDir::new().unwrap();
        let missing = tmp.path().join("nope.json");
        assert!(!is_marker_fresh(&missing, SystemTime::now()));
    }

    #[test]
    fn consume_marker_removes_file() {
        let tmp = TempDir::new().unwrap();
        let path = write_directive_marker(tmp.path(), "wren", "make a card", SystemTime::now())
            .unwrap();
        assert!(path.exists());
        consume_marker(&path);
        assert!(!path.exists());
    }
}
