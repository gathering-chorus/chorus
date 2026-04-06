//! Input Classifier (#1659)
//!
//! UserPromptSubmit hook that classifies Jeff's input as:
//! - command: imperative verb, action requested → execute
//! - question: interrogative → answer
//! - continuation: short affirmative/directive → continue current thread
//! - statement: observation, thinking out loud → acknowledge, don't act
//!
//! Classification is emitted via stderr so the role sees it in context.
//! Reduces "Jeff said 5 words and I launched 12 tool calls" pattern.

use crate::types::{HookInput, HookResponse};
use tracing::info;

#[derive(Debug, PartialEq)]
enum InputType {
    Command,
    Question,
    Continuation,
    Statement,
}

impl InputType {
    fn as_str(&self) -> &'static str {
        match self {
            InputType::Command => "command",
            InputType::Question => "question",
            InputType::Continuation => "continuation",
            InputType::Statement => "statement",
        }
    }
}

pub async fn check(input: &HookInput) -> HookResponse {
    let prompt = input.prompt.as_deref().unwrap_or("").trim();

    // Skip empty or very long prompts (likely pasted content)
    if prompt.is_empty() || prompt.len() > 500 {
        return HookResponse::allow();
    }

    // Skip skill invocations — those are always commands
    if prompt.starts_with('/') {
        return HookResponse::allow();
    }

    // Extract inner message from nudge prefix (#2255)
    // "[nudge from jeff | ...] actual message [REPLY EXPECTED...]" → "actual message"
    let classify_text = if prompt.starts_with("[nudge from") {
        extract_nudge_body(prompt)
    } else {
        prompt.to_string()
    };

    let classification = classify(&classify_text);

    info!(
        input_type = classification.as_str(),
        prompt_len = prompt.len(),
        "input-classifier"
    );

    match classification {
        InputType::Statement => {
            HookResponse::warn_stderr(&format!(
                "input.classified | type=statement | \
                 This looks like an observation, not a request. \
                 Acknowledge briefly (1-2 sentences), don't launch tool calls or start work."
            ))
        }
        InputType::Question => {
            HookResponse::warn_stderr(&format!(
                "input.classified | type=question | \
                 Answer the question. Don't start building unless asked."
            ))
        }
        // Commands and continuations flow through normally
        _ => HookResponse::allow(),
    }
}

/// Extract the actual message body from a nudge-prefixed prompt.
/// "[nudge from jeff | 2026-04-06 16:11 Boston] did we do 2246? [REPLY EXPECTED...]"
/// → "did we do 2246?"
fn extract_nudge_body(prompt: &str) -> String {
    // Find first ']' — end of nudge prefix
    let after_prefix = match prompt.find(']') {
        Some(i) => prompt[i + 1..].trim(),
        None => return prompt.to_string(),
    };
    // Strip trailing "[REPLY EXPECTED..." suffix if present
    let body = if let Some(i) = after_prefix.find("[REPLY EXPECTED") {
        after_prefix[..i].trim()
    } else {
        after_prefix
    };
    body.to_string()
}

fn classify(prompt: &str) -> InputType {
    let lower = prompt.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();

    if words.is_empty() {
        return InputType::Statement;
    }

    let first = words[0];
    let word_count = words.len();

    // Continuations: very short affirmatives/directives (1-3 words)
    if word_count <= 3 {
        let continuations = [
            "yes", "no", "ok", "okay", "sure", "go", "do it", "ship it",
            "yep", "nope", "agreed", "correct", "exactly", "right",
            "proceed", "continue", "next", "done", "stop", "wait",
            "looks good", "lgtm", "perfect", "nice", "good",
        ];
        let joined = words.join(" ");
        if continuations.iter().any(|c| joined == *c || joined.starts_with(c)) {
            return InputType::Continuation;
        }
    }

    // Commands: imperative verbs (check BEFORE questions — "can you X" is a command)
    let command_verbs = [
        "fix", "build", "add", "create", "run", "deploy", "show", "move",
        "pull", "push", "demo", "check", "test", "update", "delete", "remove",
        "install", "start", "restart", "stop", "kill", "open", "close",
        "read", "write", "edit", "search", "find", "grep", "look", "see",
        "make", "set", "get", "put", "send", "nudge", "brief", "card",
        "commit", "merge", "rebase", "revert", "reset", "tag", "release",
        "scan", "audit", "review", "verify", "validate", "lint", "format",
        "refactor", "rename", "extract", "inline", "wrap", "unwrap",
        "enable", "disable", "configure", "wire", "connect", "disconnect",
        "monitor", "observe", "tail", "watch", "log", "dump", "export",
        "import", "load", "save", "backup", "restore", "migrate", "sync",
        "let's", "let", "go", "please", "just", "jdi", "jfdi",
        "accept", "reject", "approve", "deny", "block", "allow",
    ];
    if command_verbs.contains(&first) {
        return InputType::Command;
    }
    // "can you X" / "could you X" = command, not question
    if (first == "can" || first == "could") && word_count > 2 && words[1] == "you" {
        return InputType::Command;
    }

    // Questions: interrogative words or ends with ?
    if prompt.ends_with('?') {
        return InputType::Question;
    }
    let question_starters = [
        "what", "where", "when", "why", "how", "can", "could",
        "do", "does", "did", "is", "are", "was", "were", "will",
        "would", "should", "which", "who", "whom",
    ];
    if word_count <= 12 && question_starters.contains(&first) {
        return InputType::Question;
    }

    // Statements with explicit thinking-out-loud markers
    let statement_markers = [
        "i think", "i wonder", "i notice", "i'm thinking",
        "i don't know", "i'm not sure", "i feel like",
        "that's", "this is", "it's", "it seems", "it looks like",
        "interesting", "hmm", "huh", "weird", "strange",
        "reminds me", "makes me think", "feels like",
    ];
    if statement_markers.iter().any(|m| lower.starts_with(m)) {
        return InputType::Statement;
    }

    // Short inputs (4-6 words) without imperative verbs are likely statements
    if word_count <= 6 && !command_verbs.contains(&first) {
        return InputType::Statement;
    }

    // Default: longer inputs without clear markers → treat as command
    // (safer to act than to ignore a real request)
    InputType::Command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commands() {
        assert_eq!(classify("fix the bug in photos page"), InputType::Command);
        assert_eq!(classify("add a test for the nudge hook"), InputType::Command);
        assert_eq!(classify("show me the board"), InputType::Command);
        assert_eq!(classify("can you check the logs"), InputType::Command);
        assert_eq!(classify("let's move on"), InputType::Command);
        assert_eq!(classify("just do it"), InputType::Command);
    }

    #[test]
    fn test_questions() {
        assert_eq!(classify("what cards are with you?"), InputType::Question);
        assert_eq!(classify("where is the config file"), InputType::Question);
        assert_eq!(classify("how does the hook work"), InputType::Question);
        assert_eq!(classify("is kade still building"), InputType::Question);
    }

    #[test]
    fn test_continuations() {
        assert_eq!(classify("yes"), InputType::Continuation);
        assert_eq!(classify("ok"), InputType::Continuation);
        assert_eq!(classify("do it"), InputType::Continuation);
        assert_eq!(classify("ship it"), InputType::Continuation);
        assert_eq!(classify("looks good"), InputType::Continuation);
    }

    #[test]
    fn test_nudge_body_extraction() {
        assert_eq!(
            extract_nudge_body("[nudge from jeff | 2026-04-06 16:11 Boston] did we do 2246? [REPLY EXPECTED — nudge jeff back]"),
            "did we do 2246?"
        );
        assert_eq!(
            extract_nudge_body("[nudge from jeff | 2026-04-06 16:11 Boston] show me the board"),
            "show me the board"
        );
    }

    #[test]
    fn test_nudge_wrapped_question_classified_correctly() {
        // Root cause of #2255: "did we do 2246?" via Bridge arrived as command
        assert_eq!(classify("did we do 2246?"), InputType::Question);
    }

    #[test]
    fn test_statements() {
        assert_eq!(classify("I don't know where my music is"), InputType::Statement);
        assert_eq!(classify("I think the photos are wrong"), InputType::Statement);
        assert_eq!(classify("interesting pattern"), InputType::Statement);
        assert_eq!(classify("hmm"), InputType::Statement);
        assert_eq!(classify("that's weird"), InputType::Statement);
    }
}
