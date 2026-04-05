//! Memory-first search gate (#1951)
//! PreToolUse on Grep/Glob/Bash: blocks filesystem searches for card/session
//! context when the role hasn't queried the Memory domain endpoints first.
//!
//! DEC-074: Chorus first for search. Card-story (#1947) and conversation
//! (#1946) endpoints exist. Without enforcement, roles default to grep.
//!
//! What triggers the gate:
//! - Grep/Glob targeting session dirs, briefs, messages, activity logs
//! - Grep/Glob with card reference patterns (#NNNN)
//! - Bash commands that grep/rg session transcripts or card-related files
//!
//! What skips the gate:
//! - Code file searches (src/, tests/, views/, dist/, scripts/)
//! - Searches where session already has a card-story or conversation API call
//! - Non-context searches (general filesystem, config, etc.)

use crate::state::AppState;
use crate::types::{permission_deny_json, HookInput, HookResponse};
use regex::Regex;
use std::sync::LazyLock;
use tracing::info;

// Patterns that indicate a card/session/context search
static CARD_REF_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"#\d{3,4}|card.?\d{3,4}|card[\s_-]").unwrap());

static SESSION_PATH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(session|transcript|brief|activity\.md|messages/|next-session|\.jsonl|clearing|spine)")
        .unwrap()
});

// Bash commands that search session/card context
static BASH_CONTEXT_SEARCH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(grep|rg|ag)\s+.*(/session|/brief|/messages|activity\.md|\.jsonl|transcript|clearing|briefs/)")
        .unwrap()
});

// Evidence that the role already queried Memory endpoints
static MEMORY_API_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(card-story|/api/chorus/conversation|chorus-query\.sh|/chorus\s+search|chorus search)")
        .unwrap()
});

fn is_context_search(tool: &str, pattern: &str, path: &str, command: &str) -> bool {
    match tool {
        "Grep" | "Glob" => {
            if CARD_REF_RE.is_match(pattern) {
                return true;
            }
            if SESSION_PATH_RE.is_match(path) {
                return true;
            }
            if SESSION_PATH_RE.is_match(pattern) {
                return true;
            }
            false
        }
        "Bash" => BASH_CONTEXT_SEARCH_RE.is_match(command),
        _ => false,
    }
}

fn session_has_memory_query(session_lines: &[String]) -> bool {
    for line in session_lines {
        if MEMORY_API_RE.is_match(line) {
            return true;
        }
    }
    false
}

pub fn check(input: &HookInput, state: &AppState) -> HookResponse {
    let tool = input.tool_name_str();
    if tool != "Grep" && tool != "Glob" && tool != "Bash" {
        return HookResponse::allow();
    }

    let pattern = input.get_tool_input_str("pattern");
    let path = input.get_tool_input_str("path");
    let command = input.get_tool_input_str("command");

    if !is_context_search(&tool, &pattern, &path, &command) {
        return HookResponse::allow();
    }

    let role = input.role();
    let role_str = role.as_str();

    // Check if context_inject already ran (#2225) — shared state counts as memory query
    let session_id = input.session_id.as_deref().unwrap_or("");
    if state.has_context_results_sync(session_id) {
        info!(
            gate = "memory-first",
            event = "allowed",
            role = %role_str,
            reason = "context_inject already ran (shared state)",
        );
        return HookResponse::allow();
    }

    // Fall back to session JSONL scan
    let cwd = input.cwd.as_deref().unwrap_or("");
    let tail = state.session_cache.get_tail(session_id, cwd, 200);

    if session_has_memory_query(&tail) {
        info!(
            gate = "memory-first",
            event = "allowed",
            role = %role_str,
            reason = "memory API already queried",
        );
        return HookResponse::allow();
    }

    let search_desc = if !pattern.is_empty() {
        pattern.chars().take(60).collect::<String>()
    } else {
        command.chars().take(60).collect::<String>()
    };

    info!(
        gate = "memory-first",
        event = "blocked",
        role = %role_str,
        search = %search_desc,
    );

    HookResponse::deny(&permission_deny_json(
        "Memory-first search gate (DEC-074): Query Chorus endpoints before filesystem search.\n\
         Use: curl -s http://localhost:3340/api/chorus/card-story/<id> or\n\
         bash ~/.chorus/scripts/chorus-query.sh search \"<term>\"\n\
         Then retry your search.",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_card_reference_triggers() {
        assert!(is_context_search("Grep", "#1951", "", ""));
        assert!(is_context_search("Grep", "card 1951", "", ""));
        assert!(is_context_search("Grep", "card-1951", "", ""));
    }

    #[test]
    fn test_session_path_triggers() {
        assert!(is_context_search("Grep", "alert", "briefs/", ""));
        assert!(is_context_search("Grep", "nudge", "messages/", ""));
        assert!(is_context_search("Glob", "*.jsonl", "", ""));
        assert!(is_context_search("Grep", "session-start", "", ""));
    }

    #[test]
    fn test_bash_context_search_triggers() {
        assert!(is_context_search("Bash", "", "", "grep -r 'alert' briefs/"));
        assert!(is_context_search("Bash", "", "", "rg nudge messages/activity.md"));
    }

    #[test]
    fn test_code_search_does_not_trigger() {
        assert!(!is_context_search("Grep", "escapeSparql", "/src/services/", ""));
        assert!(!is_context_search("Grep", "HookResponse", "", ""));
        assert!(!is_context_search("Glob", "*.ts", "", ""));
        assert!(!is_context_search("Bash", "", "", "grep -r 'import' src/"));
    }

    #[test]
    fn test_non_search_tool_does_not_trigger() {
        assert!(!is_context_search("Read", "#1951", "", ""));
        assert!(!is_context_search("Write", "session", "", ""));
    }

    #[test]
    fn test_memory_query_detected() {
        let lines = vec![
            "some other content".to_string(),
            "curl http://localhost:3340/api/chorus/card-story/1951".to_string(),
        ];
        assert!(session_has_memory_query(&lines));
    }

    #[test]
    fn test_chorus_search_detected() {
        let lines = vec![
            "bash ~/.chorus/scripts/chorus-query.sh search \"alert nudge\"".to_string(),
        ];
        assert!(session_has_memory_query(&lines));
    }

    #[test]
    fn test_no_memory_query() {
        let lines = vec![
            "git log --oneline".to_string(),
            "grep -r 'something' src/".to_string(),
        ];
        assert!(!session_has_memory_query(&lines));
    }
}
