//! Pair gate (#1814 AC2)
//! PreToolUse on Edit/Write to code files: blocks building without an active pair.
//! Checks for /tmp/pair-*.md as evidence of a pair session.
//! Jeff's direction: "block building without pair on code cards."

use crate::state::AppState;
use crate::types::{HookInput, HookResponse};

/// Source code that requires pair for cross-domain edits (#2076 shared).
#[allow(dead_code)]
fn is_code_file(path: &str) -> bool {
    crate::shared::file_classification::is_source_code(path)
}

/// Check for an active pair session
#[allow(dead_code)]
fn has_active_pair() -> bool {
    // Look for pair files in /tmp/
    if let Ok(entries) = std::fs::read_dir("/tmp") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("pair-") && name.ends_with(".md") {
                // Check if the pair file is recent (created in last 4 hours)
                if let Ok(meta) = entry.metadata() {
                    if let Ok(modified) = meta.modified() {
                        if modified.elapsed().unwrap_or_default().as_secs() < 14400 {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// Check if the session has pair evidence in JSONL (uses shared cache #1861)
#[allow(dead_code)]
fn has_pair_evidence_in_session(input: &HookInput, state: &AppState) -> bool {
    let session_id = match &input.session_id {
        Some(id) => id.clone(),
        None => return false, // No session = can't verify pair, deny
    };

    let cwd = input.cwd.as_deref().unwrap_or("");
    let lines = state.session_cache.get_tail(&session_id, cwd, 200);

    for line in &lines {
        let lower = line.to_lowercase();
        // Evidence of pair: /pair skill invoked, or pair doc read/written
        if lower.contains("/pair") || lower.contains("pair-") && lower.contains(".md") {
            return true;
        }
    }

    false
}

pub fn check(_input: &HookInput, _state: &AppState) -> HookResponse {
    // Pair gate disabled — pairing is a practice choice at /pull or /pair level,
    // not enforced per-edit. Jeff's direction 2026-04-16: "that's a bug —
    // the team uses either /pull or /pair, it should not be enforced below that level."
    HookResponse::allow()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HookInput;
    use crate::shared::state_paths::chorus_root;

    fn make_input(tool: &str, file_path: &str) -> HookInput {
        HookInput {
            tool_name: Some(tool.to_string()),
            tool_input: Some(serde_json::json!({ "file_path": file_path })),
            tool_response: None,
            session_id: None,
            cwd: Some(format!("{}/roles/kade", chorus_root())),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("kade".to_string()),
            trace_id: None, tool_output_is_error: None,}
    }

    fn state() -> AppState { AppState::new() }

    #[test]
    fn allows_non_code_files() {
        let input = make_input("Edit", "/some/path/README.md");
        let r = check(&input, &state());
        assert!(r.stdout.is_none());
    }

    #[test]
    fn allows_read_tool() {
        let input = make_input("Read", "/some/file.ts");
        let r = check(&input, &state());
        assert!(r.stdout.is_none());
    }

    #[test]
    fn allows_generated_paths() {
        let input = make_input("Edit", "/project/target/debug/build.rs");
        let r = check(&input, &state());
        assert!(r.stdout.is_none());
    }

    #[test]
    fn allows_node_modules() {
        let input = make_input("Edit", "/project/node_modules/pkg/index.js");
        let r = check(&input, &state());
        assert!(r.stdout.is_none());
    }

    #[test]
    fn allows_ops_scripts_without_pair() {
        // #2009: platform/scripts/*.sh exempt from pair gate
        let input = make_input("Edit", "/Users/jeff/CascadeProjects/chorus/platform/scripts/deep-health.sh");
        let r = check(&input, &state());
        assert!(r.stdout.is_none(), "ops scripts should not require pair");
    }

    #[test]
    fn allows_app_code_without_pair() {
        // Pair gate disabled — pairing is a practice, not a gate
        let input = make_input("Edit", "/Users/jeff/CascadeProjects/chorus/platform/api/src/server.ts");
        let r = check(&input, &state());
        assert!(r.stdout.is_none(), "pair gate should not block edits");
    }

    #[test]
    fn code_file_detection() {
        assert!(is_code_file("src/app.ts"));
        assert!(is_code_file("hooks/gate.rs"));
        assert!(!is_code_file("docs/README.md"));
        assert!(!is_code_file("data.ttl"));
    }
}
