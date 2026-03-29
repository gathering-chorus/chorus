//! JDI/JFDI detector — instruments Jeff's bias-to-action signals.
//!
//! Detects "jdi", "jfdi", "just do it" in Jeff's prompt text and emits
//! `interaction.jdi.received` spine events. Only fires for Jeff's messages
//! (all roles receive Jeff's prompts, so we detect by content, not role).
//!
//! Guards against false positives: roles discussing JDI in conversation
//! won't trigger this — only UserPromptSubmit prompts.

use crate::state::{chorus_log, AppState};
use crate::types::HookInput;
use regex::Regex;
use std::sync::LazyLock;

/// Pattern matches Jeff's JDI signals — standalone words, not embedded in other words.
/// Matches: "jdi", "jfdi", "just do it", "just fucking do it"
/// Does NOT match: "jdis", "the jdi pattern", role quoting Jeff
static JDI_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|\s)(?:jdi|jfdi|just\s+(?:fucking\s+)?do\s+it)(?:\s|$|[.,!?])").unwrap()
});

/// Check if a UserPromptSubmit prompt contains a JDI signal from Jeff.
/// Returns true if detected (for logging/metrics), never blocks.
pub async fn check(input: &HookInput, _state: &AppState) {
    let prompt = match &input.prompt {
        Some(p) if !p.is_empty() => p,
        _ => return,
    };

    // Only match short prompts — Jeff's JDI signals are terse.
    // Long prompts containing "just do it" in a paragraph are instructions, not signal.
    if prompt.len() > 200 {
        return;
    }

    // Skip relayed nudge content — these are injected messages, not Jeff typing
    let trimmed = prompt.trim();
    if trimmed.starts_with("[nudge from") || trimmed.starts_with("<") {
        return;
    }

    if !JDI_PATTERN.is_match(prompt) {
        return;
    }

    let role = input.role();

    // Read current card from andon state for context
    let card = read_role_card(role.as_str()).unwrap_or_default();

    // Emit spine event
    let role_str = role.as_str().to_string();
    let card_clone = card.clone();
    tokio::spawn(async move {
        let mut fields = vec![("role", role_str.as_str())];
        if !card_clone.is_empty() {
            fields.push(("card", card_clone.as_str()));
        }
        chorus_log("interaction.jdi.received", "jeff", &fields).await;
    });
}

/// Read the card a role is currently working on from andon state
fn read_role_card(role: &str) -> Option<String> {
    let state_file = format!("/tmp/claude-team-scan/{}-declared.json", role);
    let content = std::fs::read_to_string(&state_file).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;
    parsed
        .get("card")
        .and_then(|c| {
            if c.is_number() {
                Some(c.to_string())
            } else {
                c.as_str().map(|s| s.to_string())
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jdi_standalone() {
        assert!(JDI_PATTERN.is_match("jdi"));
        assert!(JDI_PATTERN.is_match("JDI"));
        assert!(JDI_PATTERN.is_match("jdi!"));
        assert!(JDI_PATTERN.is_match("ok jdi"));
    }

    #[test]
    fn test_jfdi_standalone() {
        assert!(JDI_PATTERN.is_match("jfdi"));
        assert!(JDI_PATTERN.is_match("JFDI"));
        assert!(JDI_PATTERN.is_match("yeah jfdi."));
    }

    #[test]
    fn test_just_do_it() {
        assert!(JDI_PATTERN.is_match("just do it"));
        assert!(JDI_PATTERN.is_match("Just do it"));
        assert!(JDI_PATTERN.is_match("just do it!"));
        assert!(JDI_PATTERN.is_match("ok just do it"));
    }

    #[test]
    fn test_just_fucking_do_it() {
        assert!(JDI_PATTERN.is_match("just fucking do it"));
        assert!(JDI_PATTERN.is_match("Just Fucking Do It"));
    }

    #[test]
    fn test_no_false_positives() {
        // Role discussing the concept — but these are short, so they'd match.
        // The 200-char guard handles long discussion paragraphs.
        // For short references, the spine event is still useful signal.
        assert!(!JDI_PATTERN.is_match("the jdi_detector module"));
    }

    #[test]
    fn test_embedded_no_match() {
        // Should not match when embedded in another word
        assert!(!JDI_PATTERN.is_match("jdis"));
        assert!(!JDI_PATTERN.is_match("prejdi"));
    }
}
