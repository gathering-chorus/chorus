//! TDD gate (#1814, refined per Jeff's feedback, #1915 acceptance fix)
//!
//! Two enforcement points:
//! 1. PreToolUse on Edit/Write of production code: blocks unless a test file
//!    was edited FIRST in this session. Tests before code, not tests before done.
//! 2. PreToolUse on demo: blocks unless tests were actually RUN.
//!
//! Only enforces when the role is actively building (state = "building").
//! Acceptance, retroactive closure, and idle roles are exempt — the TDD gate
//! applies to builders, not acceptors (#1915).
//!
//! Jeff's direction: "Running tests just to get past the gate is performative.
//! TDD means tests come before code, not after."

use crate::state::AppState;
use crate::types::{permission_deny_json, HookInput, HookResponse};

/// Check if the role is actively in "building" state (#1915).
/// TDD gate only applies to builders — acceptance and retroactive closure are exempt.
fn is_role_building(role: &str) -> bool {
    let state_path = format!("/tmp/claude-team-scan/{}-declared.json", role);
    match std::fs::read_to_string(&state_path) {
        Ok(content) => {
            match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(parsed) => {
                    parsed.get("state").and_then(|s| s.as_str()) == Some("building")
                }
                Err(_) => false,
            }
        }
        Err(_) => false,
    }
}

/// Test file patterns — delegated to shared module (#2076)
#[cfg(test)]
fn is_test_file(path: &str) -> bool {
    crate::shared::file_classification::is_test_file(path)
}

/// Production code — delegated to shared module (#2076)
fn is_production_code(path: &str) -> bool {
    crate::shared::file_classification::is_production_code(path)
}

/// Check if the current tool call is a demo action.
/// /acp and cards-done are NOT included — acceptance is a product decision
/// by Jeff/Wren, not a build action. The TDD gate checks the invoker's
/// session, but the acceptor isn't the builder (#1996).
/// demo_gate.rs handles done-without-demo separately.
fn is_demo_or_done(input: &HookInput) -> bool {
    let tool = input.tool_name_str();

    if tool == "Skill" {
        let skill = input.get_tool_input_str("skill");
        return skill == "demo";
    }

    false
}

/// Check if the current tool call is editing production code
fn is_code_edit(input: &HookInput) -> bool {
    let tool = input.tool_name_str();
    if tool != "Edit" && tool != "Write" {
        return false;
    }
    let file_path = input.get_tool_input_str("file_path");
    is_production_code(&file_path)
}

/// Is this path a config/tooling file (not runtime code)?
/// Writes to these have no behavioral signature — they're not runtime logic.
fn is_config_file(path: &str) -> bool {
    let base = path.rsplit('/').next().unwrap_or(path);
    // tsconfig variants, eslint configs, prettier, clippy, rustfmt, editorconfig
    base == "tsconfig.json" || base.starts_with("tsconfig.") && base.ends_with(".json")
        || base.starts_with("eslint.config.") || base == ".eslintrc.json" || base == ".eslintrc.js"
        || base == ".prettierrc" || base.starts_with(".prettierrc.")
        || base == ".clippy.toml" || base == "rustfmt.toml" || base == ".rustfmt.toml"
        || base == ".editorconfig"
}

/// Does the string contain any non-import, non-comment, non-blank line?
/// Used to classify edit strings as "purely imports/comments/whitespace" or not.
fn has_behavioral_content(s: &str) -> bool {
    for raw in s.lines() {
        let line = raw.trim();
        if line.is_empty() { continue; }
        // Comment forms: // /* * */ #
        if line.starts_with("//") || line.starts_with("/*") || line.starts_with("*")
            || line.starts_with("#") || line == "*/" {
            continue;
        }
        // Import statements (TS/JS/Rust)
        if line.starts_with("import ") || line.starts_with("export type ")
            || line.starts_with("export { ") || line.starts_with("use ")
            || line.starts_with("pub use ") {
            continue;
        }
        // Anything else = behavior
        return true;
    }
    false
}

/// #2286: edits with no behavioral signature are exempt from the TDD gate.
/// Returns true for: unused import deletion, comment-only edits,
/// config file writes (tsconfig, eslint, clippy, rustfmt, etc).
/// Returns false for: logic changes, new functions, mixed import+code edits.
pub fn is_no_signature_edit(input: &HookInput) -> bool {
    let tool = input.tool_name_str();
    let file_path = input.get_tool_input_str("file_path");

    // Config file writes are exempt — the whole file is configuration.
    if is_config_file(&file_path) {
        return true;
    }

    // For Edits: the edit is exempt only if neither side has behavioral content.
    if tool == "Edit" {
        let old_string = input.get_tool_input_str("old_string");
        let new_string = input.get_tool_input_str("new_string");
        return !has_behavioral_content(&old_string) && !has_behavioral_content(&new_string);
    }

    // Writes to non-config files are never exempt — Write replaces the entire file.
    false
}

/// Scan session for test file edits BEFORE the current point
fn has_test_file_edit(input: &HookInput, state: &AppState) -> bool {
    let session_id = match &input.session_id {
        Some(id) => id.clone(),
        None => return true,
    };

    let cwd = input.cwd.as_deref().unwrap_or("");
    let lines = state.session_cache.get_tail(&session_id, cwd, 500);
    if lines.is_empty() {
        return false; // No session data = no evidence of tests = block
    }

    for line in &lines {
        let lower = line.to_lowercase();
        // Look for Edit/Write tool calls on test files
        if (lower.contains("\"edit\"") || lower.contains("\"write\""))
            && (lower.contains(".test.") || lower.contains(".spec.")
                || lower.contains("/tests/") || lower.contains("/test/")
                || lower.contains("_test.") || lower.contains(".feature"))
        {
            return true;
        }
    }

    false
}

/// Scan session JSONL for evidence of test runs
fn has_test_run(input: &HookInput, state: &AppState) -> bool {
    let session_id = match &input.session_id {
        Some(id) => id.clone(),
        None => return true,
    };

    let cwd = input.cwd.as_deref().unwrap_or("");
    let lines = state.session_cache.get_tail(&session_id, cwd, 500);
    if lines.is_empty() {
        return true;
    }

    for line in &lines {
        let lower = line.to_lowercase();
        if (lower.contains("cargo test") || lower.contains("npx jest")
            || lower.contains("npm test") || lower.contains("npm run test")
            || lower.contains("npx cucumber") || lower.contains("vitest")
            || lower.contains("bats "))
            && lower.contains("\"bash\"")
        {
            return true;
        }
    }

    false
}

/// Check if this session edited any production code files
fn has_production_code_edit(input: &HookInput, state: &AppState) -> bool {
    let session_id = match &input.session_id {
        Some(id) => id.clone(),
        None => return false,
    };

    let cwd = input.cwd.as_deref().unwrap_or("");
    let lines = state.session_cache.get_tail(&session_id, cwd, 500);

    for line in &lines {
        let lower = line.to_lowercase();
        if (lower.contains("\"edit\"") || lower.contains("\"write\"")) && lower.contains("file_path") {
            // Extract file path and check if it's production code
            if let Some(start) = lower.find("file_path") {
                let rest = &line[start..];
                // Look for the value after file_path
                if let Some(val_start) = rest.find('"').and_then(|i| rest[i+1..].find('"').map(|j| i + 1 + j + 1)) {
                    if let Some(val_end) = rest[val_start..].find('"') {
                        let path = &rest[val_start..val_start + val_end];
                        if is_production_code(path) {
                            return true;
                        }
                    }
                }
            }
        }
    }

    false
}

/// Card-type-specific test guidance — tells the role what kind of test to write
fn test_guidance(card_type: &str) -> &'static str {
    match card_type {
        "fix" => "Write a test that reproduces the bug from the AC. It should FAIL now (proving the bug exists) and PASS after your fix. Test what Jeff sees break, not internals.",
        "new" => "Write a test for the first AC item. Describe what Jeff experiences — page loads, message appears, data shows up. The test should FAIL because the feature doesn't exist yet.",
        "enhance" => "Write a test that verifies the new behavior alongside existing behavior. Both the enhancement and the original must pass. Start from the AC.",
        _ => "Write a test that covers the first AC item. It should FAIL before you write the code.",
    }
}

pub fn check(input: &HookInput, state: &AppState) -> HookResponse {
    // #1915: TDD gate only applies when the role is actively building.
    // Acceptance, retroactive closure, and idle roles are exempt.
    if !is_role_building(input.role().as_str()) {
        return HookResponse::allow();
    }

    // Skip TDD for card types that are not code-with-tests:
    //   - chore / swat: existing skips
    //   - doc / design: documentation, service-design rewrites, ADRs, page-graph populates,
    //     ontology curation. Not behavioral code; demos validate by reading the doc / viewing
    //     the page, not by running tests. (#2683 evidence: 100+ prior misfires of the same shape.)
    let card_type = crate::types::card_type_for_role(input.role().as_str());
    if card_type == "chore" || card_type == "swat" || card_type == "doc" || card_type == "design" {
        return HookResponse::allow();
    }

    // #2286: edits with no behavioral signature (unused imports, config flags,
    // comment-only) bypass the TDD gate — there's no behavior to test.
    if is_no_signature_edit(input) {
        return HookResponse::allow();
    }

    // Gate 1: Production code edit — require test file edit first
    if is_code_edit(input)
        && !has_test_file_edit(input, state) {
            let file_path = input.get_tool_input_str("file_path");
            let fname = file_path.rsplit('/').next().unwrap_or(&file_path);
            let guidance = test_guidance(&card_type);
            return HookResponse::deny(&permission_deny_json(&format!(
                "TDD gate: you're editing production code ({}) but haven't written a test yet in this session. \
                 {} \
                 DEC-1674: AC → tests (red) → code → tests (green) → demo.",
                fname, guidance
            )));
        }

    // Gate 2 retired (2026-05-03): the demo-time "did THIS session run tests" check
    // misfired ~100+ times across 03/04 (chorus-index search receipts) on doc work,
    // bats tests, cross-session work, page-graph populates. The discipline lives at
    // Gate 1 (code-edit time), not at demo time. Validation surface for non-code
    // cards is the demo itself, not a synthetic test-run check. is_demo_or_done /
    // has_production_code_edit / has_test_run helpers are kept for now; they may be
    // removed in a follow-up if no consumer surfaces.

    HookResponse::allow()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HookInput;
    use crate::shared::state_paths::chorus_root;

    fn make_input(tool: &str, key: &str, val: &str) -> HookInput {
        HookInput {
            tool_name: Some(tool.to_string()),
            tool_input: Some(serde_json::json!({ key: val })),
            tool_response: None,
            session_id: None,
            cwd: Some(format!("{}/roles/kade", chorus_root())),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("kade".to_string()),
            chorus_worktree_override: None, trace_id: None,}
    }

    fn state() -> AppState { AppState::new() }

    #[test]
    fn allows_non_code_tools() {
        let input = make_input("Read", "file_path", "/some/file.ts");
        let r = check(&input, &state());
        assert!(r.stdout.is_none());
    }

    #[test]
    fn test_file_detection() {
        assert!(is_test_file("src/hooks/tdd_gate_test.rs"));
        assert!(is_test_file("src/app.test.ts"));
        assert!(is_test_file("tests/integration.rs"));
        assert!(is_test_file("features/gates/tdd.feature"));
        assert!(!is_test_file("src/main.rs"));
        assert!(!is_test_file("src/app.ts"));
    }

    #[test]
    fn production_code_detection() {
        assert!(is_production_code("src/main.rs"));
        assert!(is_production_code("src/app.ts"));
        assert!(!is_production_code("src/app.test.ts"));
        assert!(!is_production_code("tests/integration.rs"));
        assert!(!is_production_code("README.md"));
        assert!(!is_production_code("target/debug/build.rs"));
    }

    #[test]
    fn detects_demo_skill() {
        let input = make_input("Skill", "skill", "demo");
        assert!(is_demo_or_done(&input));
    }

    #[test]
    fn acp_not_demo_or_done() {
        // #1996: /acp is acceptance, not a build action — TDD gate shouldn't block it
        let input = make_input("Skill", "skill", "acp");
        assert!(!is_demo_or_done(&input));
    }

    #[test]
    fn board_done_not_demo_or_done() {
        // #1996: cards done handled by demo_gate.rs, not TDD gate
        let input = make_input("Bash", "command", "bash board-ts done 1811");
        assert!(!is_demo_or_done(&input));
    }

    #[test]
    fn ignores_other_bash() {
        let input = make_input("Bash", "command", "ls -la");
        assert!(!is_demo_or_done(&input));
    }

    #[test]
    fn code_edit_detected() {
        let input = make_input("Edit", "file_path", "/project/src/main.rs");
        assert!(is_code_edit(&input));
    }

    #[test]
    fn test_edit_not_code_edit() {
        let input = make_input("Edit", "file_path", "/project/tests/gate.test.ts");
        assert!(!is_code_edit(&input));
    }

    #[test]
    fn allows_demo_without_session() {
        let input = make_input("Skill", "skill", "demo");
        let r = check(&input, &state());
        assert!(r.stdout.is_none());
    }

    #[test]
    fn demo_with_session_no_longer_denies() {
        // Gate 2 retirement (2026-05-03): demo invocations with a session_id present
        // and prior production_code edits no longer trigger the "no test runs" deny.
        // The discipline lives at Gate 1 (code-edit time), not at demo time.
        let input = HookInput {
            tool_name: Some("Skill".into()),
            tool_input: Some(serde_json::json!({"skill": "demo", "args": "1234"})),
            tool_response: None,
            session_id: Some("test-session".into()),
            cwd: Some(format!("{}/roles/kade", chorus_root())),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("kade".into()),
            chorus_worktree_override: None, trace_id: None,
        };
        let r = check(&input, &state());
        assert!(r.stdout.is_none(), "Gate 2 should be retired — demo must allow regardless of test-run history");
    }

    // --- #2297: bats detection ---

    #[test]
    fn markdown_edit_not_code_edit() {
        let input = make_input("Edit", "file_path", "/project/SKILL.md");
        assert!(!is_code_edit(&input));
    }

    #[test]
    fn yaml_edit_not_code_edit() {
        let input = make_input("Edit", "file_path", "/project/alerting/synthetic-test.yml");
        assert!(!is_code_edit(&input));
    }

    #[test]
    fn plist_edit_not_code_edit() {
        let input = make_input("Write", "file_path", "/Users/jeff/Library/LaunchAgents/com.chorus.test.plist");
        assert!(!is_code_edit(&input));
    }

    #[test]
    fn non_code_files_detected_correctly() {
        assert!(!is_production_code("SKILL.md"));
        assert!(!is_production_code("alerting/synthetic-test.yml"));
        assert!(!is_production_code("config/profiles/base.json"));
        assert!(!is_production_code("docs/README.md"));
    }

    // ── #2286: is_no_signature_edit ──────────────────────────────────────────

    fn edit(file: &str, old: &str, new: &str) -> HookInput {
        HookInput {
            tool_name: Some("Edit".into()),
            tool_input: Some(serde_json::json!({
                "file_path": file, "old_string": old, "new_string": new,
            })),
            tool_response: None, session_id: None, cwd: None, prompt: None,
            stop_hook_active: None, hook_type: None, deploy_role: Some("kade".into()),
            chorus_worktree_override: None, trace_id: None,}
    }
    fn write(file: &str, content: &str) -> HookInput {
        HookInput {
            tool_name: Some("Write".into()),
            tool_input: Some(serde_json::json!({"file_path": file, "content": content})),
            tool_response: None, session_id: None, cwd: None, prompt: None,
            stop_hook_active: None, hook_type: None, deploy_role: Some("kade".into()),
            chorus_worktree_override: None, trace_id: None,}
    }

    #[test]
    fn unused_import_delete_is_exempt() {
        let i = edit("src/app.ts", "import { foo } from './foo';", "");
        assert!(is_no_signature_edit(&i));
    }

    #[test]
    fn rust_use_delete_is_exempt() {
        let i = edit("src/lib.rs", "use std::fs;", "");
        assert!(is_no_signature_edit(&i));
    }

    #[test]
    fn tsconfig_flag_flip_is_exempt() {
        let i = edit("tsconfig.json", "\"noUnusedLocals\": false", "\"noUnusedLocals\": true");
        assert!(is_no_signature_edit(&i));
    }

    #[test]
    fn eslint_config_edit_is_exempt() {
        let i = edit("eslint.config.js", "'warn'", "'error'");
        assert!(is_no_signature_edit(&i));
    }

    #[test]
    fn clippy_toml_write_is_exempt() {
        let i = write(".clippy.toml", "msrv = \"1.70\"\n");
        assert!(is_no_signature_edit(&i));
    }

    #[test]
    fn comment_only_edit_is_exempt() {
        let i = edit("src/app.ts", "// old", "// new explaining why");
        assert!(is_no_signature_edit(&i));
    }

    #[test]
    fn new_function_is_not_exempt() {
        let i = edit("src/app.ts", "const x = 1;", "const x = 1;\nfunction bar() { return 2; }");
        assert!(!is_no_signature_edit(&i));
    }

    #[test]
    fn logic_change_is_not_exempt() {
        let i = edit("src/app.ts", "return x + 1;", "return x + 2;");
        assert!(!is_no_signature_edit(&i));
    }

    #[test]
    fn mixed_import_and_logic_is_not_exempt() {
        let i = edit("src/app.ts",
            "import { foo } from './foo';\nconst x = 1;",
            "import { bar } from './bar';\nconst x = foo(2);");
        assert!(!is_no_signature_edit(&i));
    }

    #[test]
    fn write_to_ts_source_is_not_exempt() {
        let i = write("src/app.ts", "export function handler() { return 1; }");
        assert!(!is_no_signature_edit(&i));
    }
}
