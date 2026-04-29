//! Interaction Pattern Detector (#2282)
//!
//! Classifies Jeff's prompt into one of 9 interaction patterns:
//! direction, ideation, demo, triage, swat, gemba, clearing, story, reflection
//!
//! Emits interaction.pattern.detected on pattern SHIFT only — not every message.
//! All roles emit, not just Wren. Card-type mapping is fallback, not primary.

use crate::shared::state_paths::chorus_root;
use crate::state::AppState;
use crate::types::HookInput;
use tracing::info;

#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Direction,   // Jeff directing work: "pull 2282", "card it", "fix this"
    Ideation,    // Brainstorming: "what if we...", "I'm thinking about..."
    Demo,        // Reviewing work: "show me", "demo", "looks good", "acp"
    Triage,      // Sorting/prioritizing: "what's next", board scanning, seeds
    Swat,        // Crisis: "this is broken", "down", urgent fix language
    Gemba,       // Observing: "what's kade doing", "tail silas", "/gemba"
    Clearing,    // Multi-role alignment: "/clearing", group discussion
    Story,       // Jeff sharing context: "reminds me of", "at Staples we...", "my dad..."
    Reflection,  // Looking back: "what did we ship", "how's the session", wrap-up
}

impl Pattern {
    pub fn as_str(&self) -> &'static str {
        match self {
            Pattern::Direction => "direction",
            Pattern::Ideation => "ideation",
            Pattern::Demo => "demo",
            Pattern::Triage => "triage",
            Pattern::Swat => "swat",
            Pattern::Gemba => "gemba",
            Pattern::Clearing => "clearing",
            Pattern::Story => "story",
            Pattern::Reflection => "reflection",
        }
    }
}

/// Detect interaction pattern from Jeff's prompt text
pub fn detect(prompt: &str) -> Pattern {
    let lower = prompt.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();

    if words.is_empty() {
        return Pattern::Direction;
    }

    // Skill invocations are strong signals
    if lower.starts_with("/clearing") { return Pattern::Clearing; }
    if lower.starts_with("/gemba") { return Pattern::Gemba; }
    if lower.starts_with("/demo") || lower.starts_with("/acp") { return Pattern::Demo; }
    if lower.starts_with("/pull") || lower.starts_with("/jdi") { return Pattern::Direction; }
    if lower.starts_with("/cs") || lower.starts_with("/sb") || lower.starts_with("/ab") || lower.starts_with("/flow") { return Pattern::Triage; }
    if lower.starts_with("/reboot") || lower.starts_with("/werk") { return Pattern::Reflection; }

    // Swat: urgency and breakage language
    let swat_signals = ["broken", "down", "crashed", "emergency", "fire", "urgent", "fix this now", "swat"];
    if swat_signals.iter().any(|s| lower.contains(s)) {
        return Pattern::Swat;
    }

    // Demo: review and acceptance language
    let demo_signals = ["show me", "demo", "looks good", "accept", "acp", "ship it", "looks right"];
    if demo_signals.iter().any(|s| lower.contains(s)) {
        return Pattern::Demo;
    }

    // Gemba: observation language
    let gemba_signals = ["what's kade", "what's silas", "what's wren", "what is kade", "what is silas", "what is wren", "tail ", "observe", "watching"];
    if gemba_signals.iter().any(|s| lower.contains(s)) {
        return Pattern::Gemba;
    }

    // Story: narrative and personal context
    let story_signals = ["reminds me", "at staples", "my dad", "back when", "years ago", "i remember", "that time", "the story"];
    if story_signals.iter().any(|s| lower.contains(s)) {
        return Pattern::Story;
    }

    // Ideation: exploratory and speculative language
    let ideation_signals = ["what if", "i'm thinking", "what about", "could we", "imagine", "brainstorm", "idea", "explore", "spike"];
    if ideation_signals.iter().any(|s| lower.contains(s)) {
        return Pattern::Ideation;
    }

    // Reflection: session review and wrap-up
    let reflection_signals = ["what did we", "how many cards", "session", "eod", "wrapping up", "done for today", "how's it going", "status"];
    if reflection_signals.iter().any(|s| lower.contains(s)) {
        return Pattern::Reflection;
    }

    // Triage: board and prioritization language
    let triage_signals = ["what's next", "board", "backlog", "prioriti", "which card", "seeds", "triage"];
    if triage_signals.iter().any(|s| lower.contains(s)) {
        return Pattern::Triage;
    }

    // Default: directing work
    Pattern::Direction
}

/// Check prompt and emit pattern shift event
pub async fn check(input: &HookInput, state: &AppState) -> Option<String> {
    let prompt = input.prompt.as_deref().unwrap_or("").trim();
    if prompt.is_empty() || prompt.len() > 500 {
        return None;
    }

    let role_str = input.role().as_str().to_string();
    let detected = detect(prompt);
    let detected_str = detected.as_str().to_string();

    let previous = state.get_interaction_pattern(&role_str).await;

    // Only emit on shift
    if previous == detected_str {
        return None;
    }

    // Update state
    state.set_interaction_pattern_direct(&role_str, &detected_str).await;

    info!(
        role = role_str.as_str(),
        from = previous.as_str(),
        to = detected_str.as_str(),
        "interaction-pattern-shift"
    );

    // Emit spine event via chorus-log (fire-and-forget)
    let role_clone = role_str.clone();
    let detected_clone = detected_str.clone();
    let previous_clone = previous.clone();
    tokio::task::spawn_blocking(move || {
        let chorus_log_script = format!("{}/platform/scripts/chorus-log", chorus_root());
        let _ = std::process::Command::new(&chorus_log_script)
            .args([
                "interaction.pattern.detected",
                &role_clone,
                &format!("pattern={}", detected_clone),
                &format!("from={}", previous_clone),
            ])
            .output();
    });

    Some(format!("interaction.pattern | {} → {}", previous, detected_str))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_invocations() {
        assert_eq!(detect("/clearing start"), Pattern::Clearing);
        assert_eq!(detect("/gemba kade"), Pattern::Gemba);
        assert_eq!(detect("/demo 2277"), Pattern::Demo);
        assert_eq!(detect("/pull 2282"), Pattern::Direction);
        assert_eq!(detect("/reboot"), Pattern::Reflection);
        assert_eq!(detect("/sb"), Pattern::Triage);
    }

    #[test]
    fn swat_urgency() {
        assert_eq!(detect("this is broken"), Pattern::Swat);
        assert_eq!(detect("the app is down"), Pattern::Swat);
        assert_eq!(detect("emergency fix needed"), Pattern::Swat);
    }

    #[test]
    fn demo_review() {
        assert_eq!(detect("show me the page"), Pattern::Demo);
        assert_eq!(detect("looks good ship it"), Pattern::Demo);
        assert_eq!(detect("acp 2277"), Pattern::Demo);
    }

    #[test]
    fn gemba_observation() {
        assert_eq!(detect("what's kade doing"), Pattern::Gemba);
        assert_eq!(detect("what's silas building"), Pattern::Gemba);
    }

    #[test]
    fn story_narrative() {
        assert_eq!(detect("reminds me of the Dallas Systems days"), Pattern::Story);
        assert_eq!(detect("at Staples we had the same problem"), Pattern::Story);
        assert_eq!(detect("my dad drew the blueprint"), Pattern::Story);
    }

    #[test]
    fn ideation_exploring() {
        assert_eq!(detect("what if we made the hooks emit prometheus metrics"), Pattern::Ideation);
        assert_eq!(detect("I'm thinking about a different approach"), Pattern::Ideation);
        assert_eq!(detect("spike on local embeddings"), Pattern::Ideation);
    }

    #[test]
    fn reflection_wrapup() {
        assert_eq!(detect("what did we ship today"), Pattern::Reflection);
        assert_eq!(detect("how many cards this session"), Pattern::Reflection);
        assert_eq!(detect("eod"), Pattern::Reflection);
        assert_eq!(detect("wrapping up"), Pattern::Reflection);
    }

    #[test]
    fn triage_board() {
        assert_eq!(detect("what's next"), Pattern::Triage);
        assert_eq!(detect("check the board"), Pattern::Triage);
        assert_eq!(detect("triage the seeds"), Pattern::Triage);
    }

    #[test]
    fn default_direction() {
        assert_eq!(detect("fix the perf baseline disk metric"), Pattern::Direction);
        assert_eq!(detect("pull 2282"), Pattern::Direction);
        assert_eq!(detect("make a card for this"), Pattern::Direction);
    }
}
