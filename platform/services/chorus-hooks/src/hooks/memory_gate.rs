//! Context synthesis gate (#1811, redesigned #1835)
//! PreToolUse on Edit/Write: blocks code changes without demonstrated synthesis.
//!
//! The gate doesn't check whether you searched — it checks whether you
//! produced a context synthesis that shows you understood what you found.
//! Searching and throwing away results is the same as not searching.
//!
//! What counts as synthesis:
//! - Assistant message containing "prior work:" or "current state:" or "approach:"
//!   (the context synthesis block)
//! - Assistant message referencing specific prior cards, decisions, or patterns
//!   found via Chorus/memory search
//!
//! Jeff's refinement:
//! - Defect fixes: ALWAYS enforce (you need prior context most when fixing)
//! - Own domain, enhancement: trust builder
//! - Cross-domain: enforce
//! - Sub-millisecond — same scan pattern as JDI hook

use crate::state::AppState;
use crate::types::{permission_deny_json, HookInput, HookResponse};
use tracing::info;

/// PostToolUse: log when a Chorus search or memory read completes.
/// This creates the investigation timeline Jeff needs to see in the log —
/// not just write-time decisions, but the full research-then-synthesize arc.
pub fn post_check(input: &HookInput) {
    let tool = input.tool_name_str();
    let role_name = format!("{:?}", input.role()).to_lowercase();

    match tool {
        "Bash" => {
            // Check if this was a Chorus search
            let cmd = input.get_tool_input_str("command");
            let response = input.tool_response_str();
            if cmd.contains("chorus-query.sh") || cmd.contains("chorus search") {
                let result_preview = if response.len() > 100 {
                    format!("{}...", &response[..100])
                } else {
                    response.to_string()
                };
                info!(
                    gate = "context-synthesis",
                    event = "chorus-search-completed",
                    role = %role_name,
                    query = %cmd,
                    results = %result_preview,
                );
            }
            // Check if this was a git history lookup
            if cmd.contains("git log") || cmd.contains("git show") || cmd.contains("git blame") {
                info!(
                    gate = "context-synthesis",
                    event = "research-completed",
                    role = %role_name,
                    query = %cmd,
                );
            }
        }
        "Read" => {
            let file_path = input.get_tool_input_str("file_path");
            let lower = file_path.to_lowercase();
            if lower.contains("memory.md") || lower.contains("decisions/")
                || lower.contains("/memory/") || lower.contains("briefs/")
            {
                info!(
                    gate = "context-synthesis",
                    event = "memory-read",
                    role = %role_name,
                    file = %file_path,
                );
            }
        }
        _ => {}
    }
}

/// File extensions that count as "code files" — the gate only fires for these
fn is_code_file(path: &str) -> bool {
    let code_exts = [
        ".rs", ".ts", ".tsx", ".js", ".jsx", ".py", ".sh", ".ejs", ".html",
        ".css", ".scss", ".json", ".toml", ".yaml", ".yml",
    ];
    code_exts.iter().any(|ext| path.ends_with(ext))
}

/// Check if the card context is a fix — reads type: label from board (#1909)
fn is_defect_fix() -> bool {
    crate::types::is_fix_card()
}

/// Markers that indicate the role produced a context synthesis —
/// demonstrated understanding, not just search execution.
const SYNTHESIS_MARKERS: &[&str] = &[
    "prior work:",
    "current state:",
    "approach:",
    "context synthesis",
    "from prior sessions",
    "chorus shows",
    "memory shows",
    "based on dec-",
    "based on adr-",
    "from card #",
    "shipped in #",
    "last time this",
    "known failure",
    "prior art:",
    "this connects to",
    "the pattern from",
];

/// Markers that indicate a search happened (necessary but not sufficient)
const SEARCH_MARKERS: &[&str] = &[
    "chorus-query.sh",
    "\"chorus search\"",
    "\"skill\":\"chorus",
    "skill:\"chorus",
    "memory.md",
    "decisions/",
    "/memory/",
];

/// Scan session JSONL for context synthesis in assistant output (uses shared cache #1861).
/// Check if session contains git log/blame on the specific file being edited (#1903).
/// Only called for fix/swat cards — ensures the role read commit history before modifying.
fn scan_for_git_history(input: &HookInput, state: &AppState, target_file: &str) -> bool {
    let session_id = match &input.session_id {
        Some(id) => id.clone(),
        None => return true, // No session = can't check, allow
    };

    let cwd = input.cwd.as_deref().unwrap_or("");
    let lines = state.session_cache.get_tail(&session_id, cwd, 300);
    if lines.is_empty() {
        return true; // Can't read = allow
    }

    // Extract filename for matching (git log shows relative paths)
    let fname = target_file.rsplit('/').next().unwrap_or(target_file);

    for line in &lines {
        let lower = line.to_lowercase();
        // Check for git log/blame/show on this file or its parent directory
        if (lower.contains("git log") || lower.contains("git blame") || lower.contains("git show"))
            && lower.contains(&fname.to_lowercase())
        {
            return true;
        }
    }

    false
}

/// Returns (has_search, has_synthesis)
///
/// has_search: the role ran a search (Chorus, memory files, git)
/// has_synthesis: the role produced output demonstrating understanding of what they found
fn scan_session_for_synthesis(input: &HookInput, state: &AppState) -> (bool, bool) {
    let session_id = match &input.session_id {
        Some(id) => id.clone(),
        None => return (true, true), // No session ID = can't check, allow
    };

    let cwd = input.cwd.as_deref().unwrap_or("");
    let lines = state.session_cache.get_tail(&session_id, cwd, 300);
    if lines.is_empty() {
        return (true, true); // Can't read JSONL = allow
    }

    let mut has_search = false;
    let mut has_synthesis = false;

    for line in &lines {
        if line.is_empty() {
            continue;
        }

        let lower = line.to_lowercase();

        // Check for search execution (tool calls)
        if !has_search {
            for marker in SEARCH_MARKERS {
                if lower.contains(marker) {
                    has_search = true;
                    break;
                }
            }
            // git history checks also count
            if !has_search && (lower.contains("git log") || lower.contains("git show")
                || lower.contains("git blame")) {
                has_search = true;
            }
        }

        // Check for synthesis in assistant output —
        // assistant messages contain reasoning that references what was found
        if !has_synthesis && lower.contains("assistant") {
            for marker in SYNTHESIS_MARKERS {
                if lower.contains(marker) {
                    has_synthesis = true;
                    break;
                }
            }
        }

        if has_search && has_synthesis {
            break; // Early exit
        }
    }

    (has_search, has_synthesis)
}

pub fn check(input: &HookInput, state: &AppState) -> HookResponse {
    let tool = input.tool_name_str();
    if tool != "Edit" && tool != "Write" {
        return HookResponse::allow();
    }

    let file_path = input.get_tool_input_str("file_path");

    // Only gate code files
    if !is_code_file(&file_path) {
        return HookResponse::allow();
    }

    // Skip generated/build paths
    if file_path.contains("/target/") || file_path.contains("/node_modules/")
        || file_path.contains(".gitignore") || file_path.contains("package-lock")
    {
        return HookResponse::allow();
    }

    // Own domain, enhancement = trust builder
    let role = input.role();
    let is_own_domain = match role {
        crate::types::Role::Kade => file_path.contains("/engineer/") || file_path.contains("/src/"),
        crate::types::Role::Silas => file_path.contains("/architect/") || file_path.contains("/platform/"),
        crate::types::Role::Wren => file_path.contains("/product-manager/") || file_path.contains("/directing/"),
        crate::types::Role::Unknown => false,
    };

    if is_own_domain && !is_defect_fix() {
        info!(
            gate = "context-synthesis",
            decision = "skip",
            reason = "own domain enhancement",
            role = %format!("{:?}", role).to_lowercase(),
            file = %file_path,
        );
        return HookResponse::allow();
    }

    // Fix cards: require git log on the target file (#1903)
    if is_defect_fix() {
        let has_git_history = scan_for_git_history(input, state, &file_path);
        if !has_git_history {
            let fname = file_path.rsplit('/').next().unwrap_or(&file_path);
            info!(
                gate = "context-synthesis",
                decision = "deny",
                reason = "fix card without git history on target file",
                role = %format!("{:?}", role).to_lowercase(),
                file = %file_path,
            );
            return HookResponse::deny(&permission_deny_json(
                &format!(
                    "Context synthesis gate: fix card but no git history on {}. \
                     Run `git log {}` or `git blame {}` first — this file has prior commits \
                     that explain what was tried before. Don't repeat the same fix.",
                    fname, fname, fname
                )
            ));
        }
    }

    // The real gate: check for synthesis, not just search
    let (has_search, has_synthesis) = scan_session_for_synthesis(input, state);

    let role_name = format!("{:?}", role).to_lowercase();

    if !has_search && !has_synthesis {
        info!(
            gate = "context-synthesis",
            decision = "deny",
            reason = "no search, no synthesis",
            role = %role_name,
            file = %file_path,
        );
        return HookResponse::deny(&permission_deny_json(
            "Context synthesis gate: no search AND no synthesis detected. \
             Before writing code: 1) search Chorus + memory for prior work on this problem, \
             2) produce a context synthesis showing what you found and how it shapes your approach. \
             Searching without synthesizing is the same as not searching."
        ));
    }

    if has_search && !has_synthesis {
        info!(
            gate = "context-synthesis",
            decision = "deny",
            reason = "searched but no synthesis",
            role = %role_name,
            file = %file_path,
        );
        return HookResponse::deny(&permission_deny_json(
            "Context synthesis gate: you searched but didn't synthesize. \
             You ran searches — now demonstrate understanding: what did prior work tell you? \
             What's your approach given that context? Show your reasoning before writing code. \
             (Use markers like 'Prior work:', 'Current state:', 'Approach:' in your response.)"
        ));
    }

    if !has_search && has_synthesis {
        info!(
            gate = "context-synthesis",
            decision = "warn",
            reason = "synthesis without search",
            role = %role_name,
            file = %file_path,
        );
        return HookResponse::warn_stderr(
            "Context synthesis gate: synthesis found but no search detected. \
             If you're working from session context, that's fine. But if you're \
             guessing, run chorus-query.sh first."
        );
    }

    info!(
        gate = "context-synthesis",
        decision = "allow",
        reason = "search + synthesis present",
        role = %role_name,
        file = %file_path,
    );
    HookResponse::allow()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HookInput;

    fn make_input(tool: &str, file_path: &str) -> HookInput {
        HookInput {
            tool_name: Some(tool.to_string()),
            tool_input: Some(serde_json::json!({
                "file_path": file_path
            })),
            tool_response: None,
            session_id: None,
            cwd: Some("/Users/jeffbridwell/CascadeProjects/chorus/engineer".to_string()),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("kade".to_string()),
        }
    }

    fn make_input_with_session(tool: &str, file_path: &str, session_lines: &[&str]) -> (HookInput, tempfile::TempDir) {
        let tmp = tempfile::TempDir::new().unwrap();
        let cwd = tmp.path().join("test-project");
        std::fs::create_dir_all(&cwd).unwrap();

        let cwd_str = cwd.to_string_lossy().to_string();
        let project_key = cwd_str.replace('/', "-");
        let project_key = project_key.strip_prefix('-').unwrap_or(&project_key);

        let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/jeffbridwell".to_string());
        let session_id = format!("test-{}", std::process::id());
        let jsonl_dir = format!("{}/.claude/projects/-{}", home, project_key);
        std::fs::create_dir_all(&jsonl_dir).unwrap();
        let jsonl_path = format!("{}/{}.jsonl", jsonl_dir, session_id);
        let content = session_lines.join("\n");
        std::fs::write(&jsonl_path, &content).unwrap();

        let input = HookInput {
            tool_name: Some(tool.to_string()),
            tool_input: Some(serde_json::json!({ "file_path": file_path })),
            tool_response: None,
            session_id: Some(session_id.clone()),
            cwd: Some(cwd_str),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("silas".to_string()),
        };
        (input, tmp)
    }

    fn state() -> AppState { AppState::new() }

    // --- Basic gate behavior (unchanged) ---

    #[test]
    fn allows_non_code_files() {
        let r = check(&make_input("Edit", "/some/path/README.md"), &state());
        assert!(r.exit_code == 0);
    }

    #[test]
    fn allows_non_edit_tools() {
        let r = check(&make_input("Read", "/some/file.ts"), &state());
        assert!(r.exit_code == 0);
    }

    #[test]
    fn allows_own_domain() {
        let r = check(&make_input("Edit", "/Users/jeffbridwell/CascadeProjects/chorus/engineer/src/app.ts"), &state());
        assert!(r.exit_code == 0);
    }

    #[test]
    fn allows_target_dir() {
        let r = check(&make_input("Edit", "/project/target/debug/build.rs"), &state());
        assert!(r.exit_code == 0);
    }

    #[test]
    fn gate_applies_to_code_files() {
        assert!(is_code_file("src/handlers/people.handler.ts"));
        assert!(is_code_file("src/hooks/memory_gate.rs"));
        assert!(!is_code_file("docs/README.md"));
        assert!(!is_code_file("notes.txt"));
    }

    // --- Synthesis detection (the real gate) ---

    // Search happened but no synthesis — BLOCKED
    #[test]
    fn blocks_search_without_synthesis() {
        let s = state();
        let lines = vec![
            r#"{"type":"tool_use","name":"Bash","input":{"command":"bash chorus-query.sh search seeds"}}"#,
            r#"{"type":"tool_result","content":"Found 20 results..."}"#,
            r#"{"type":"assistant","content":"What are you seeing with seeds?"}"#,
        ];
        let (input, _tmp) = make_input_with_session("Edit", "/cross/domain/file.ts", &lines);
        let (has_search, has_synthesis) = scan_session_for_synthesis(&input, &s);
        assert!(has_search, "search should be detected");
        assert!(!has_synthesis, "no synthesis — role asked Jeff instead of reasoning");
    }

    // Search + synthesis — PASS
    #[test]
    fn passes_search_with_synthesis() {
        let s = state();
        let lines = vec![
            r#"{"type":"tool_use","name":"Bash","input":{"command":"bash chorus-query.sh search seed pipeline"}}"#,
            r#"{"type":"tool_result","content":"Found 20 results: Kade shipped #1794 dual write..."}"#,
            r#"{"type":"assistant","content":"Prior work: Kade shipped #1794 (kill dual write) and #1798 (routing defaults) last week. Current state: the Twilio webhook routes through SMS adapter to Fuseki. Approach: check the webhook endpoint first since the route may have shifted in restructure."}"#,
        ];
        let (input, _tmp) = make_input_with_session("Edit", "/cross/domain/file.ts", &lines);
        let (has_search, has_synthesis) = scan_session_for_synthesis(&input, &s);
        assert!(has_search);
        assert!(has_synthesis, "synthesis with prior work reference should pass");
    }

    // No search, no synthesis — BLOCKED
    #[test]
    fn blocks_nothing() {
        let s = state();
        let lines = vec![
            r#"{"type":"assistant","content":"Let me fix this file."}"#,
        ];
        let (input, _tmp) = make_input_with_session("Edit", "/cross/domain/file.ts", &lines);
        let (has_search, has_synthesis) = scan_session_for_synthesis(&input, &s);
        assert!(!has_search);
        assert!(!has_synthesis);
    }

    // Synthesis referencing a decision — PASS
    #[test]
    fn detects_decision_reference() {
        let s = state();
        let lines = vec![
            r#"{"type":"tool_use","name":"Read","input":{"file_path":"/project/memory/MEMORY.md"}}"#,
            r#"{"type":"assistant","content":"Based on DEC-094, harvest is paused. This change aligns with the tightening operations phase."}"#,
        ];
        let (input, _tmp) = make_input_with_session("Edit", "/cross/domain/file.ts", &lines);
        let (has_search, has_synthesis) = scan_session_for_synthesis(&input, &s);
        assert!(has_search, "MEMORY.md read is a search");
        assert!(has_synthesis, "DEC reference is synthesis");
    }

    // Synthesis referencing prior card — PASS
    #[test]
    fn detects_card_reference() {
        let s = state();
        let lines = vec![
            r#"{"type":"tool_use","name":"Bash","input":{"command":"bash chorus-query.sh search nudge delivery"}}"#,
            r#"{"type":"assistant","content":"Shipped in #1793 — nudge delivery was rewritten. The current path uses osascript inject, not TTY polling."}"#,
        ];
        let (input, _tmp) = make_input_with_session("Edit", "/cross/domain/file.ts", &lines);
        let (_, has_synthesis) = scan_session_for_synthesis(&input, &s);
        assert!(has_synthesis, "card reference shows understanding");
    }

    // Chorus shows pattern — PASS
    #[test]
    fn detects_chorus_shows() {
        let s = state();
        let lines = vec![
            r#"{"type":"tool_use","name":"Bash","input":{"command":"bash chorus-query.sh search clearing path"}}"#,
            r#"{"type":"assistant","content":"Chorus shows this was flagged Feb 26 — same stale path pattern after restructure."}"#,
        ];
        let (input, _tmp) = make_input_with_session("Edit", "/cross/domain/file.ts", &lines);
        let (_, has_synthesis) = scan_session_for_synthesis(&input, &s);
        assert!(has_synthesis);
    }

    // No session ID — allow (can't check)
    #[test]
    fn allows_no_session() {
        let r = check(&make_input("Edit", "/some/cross-domain/file.ts"), &state());
        assert!(r.exit_code == 0);
    }
}
