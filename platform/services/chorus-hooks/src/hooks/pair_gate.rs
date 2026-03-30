//! Pair gate (#1814 AC2)
//! PreToolUse on Edit/Write to code files: blocks building without an active pair.
//! Checks for /tmp/pair-*.md as evidence of a pair session.
//! Jeff's direction: "block building without pair on code cards."

use crate::state::AppState;
use crate::types::{permission_deny_json, HookInput, HookResponse};

/// File extensions that count as code files
fn is_code_file(path: &str) -> bool {
    let code_exts = [
        ".rs", ".ts", ".tsx", ".js", ".jsx", ".py", ".sh", ".ejs", ".html",
        ".css", ".scss",
    ];
    code_exts.iter().any(|ext| path.ends_with(ext))
}

/// Check for an active pair session
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

pub fn check(input: &HookInput, state: &AppState) -> HookResponse {
    let tool = input.tool_name_str();
    if tool != "Edit" && tool != "Write" {
        return HookResponse::allow();
    }

    let file_path = input.get_tool_input_str("file_path");
    if !is_code_file(&file_path) {
        return HookResponse::allow();
    }

    // Skip generated/build paths
    if file_path.contains("/target/") || file_path.contains("/node_modules/")
        || file_path.contains("/dist/")
    {
        return HookResponse::allow();
    }

    // Check for active pair — either a pair file on disk or pair evidence in session
    if has_active_pair() || has_pair_evidence_in_session(input, state) {
        return HookResponse::allow();
    }

    HookResponse::deny(&permission_deny_json(
        "Pair gate: no active pair session detected. \
         Start /pair before editing code files. \
         #1814: code cards require a pair."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HookInput;

    fn make_input(tool: &str, file_path: &str) -> HookInput {
        HookInput {
            tool_name: Some(tool.to_string()),
            tool_input: Some(serde_json::json!({ "file_path": file_path })),
            tool_response: None,
            session_id: None,
            cwd: Some("/Users/jeffbridwell/CascadeProjects/chorus/engineer".to_string()),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("kade".to_string()),
        }
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
    fn code_file_detection() {
        assert!(is_code_file("src/app.ts"));
        assert!(is_code_file("hooks/gate.rs"));
        assert!(!is_code_file("docs/README.md"));
        assert!(!is_code_file("data.ttl"));
    }
}
