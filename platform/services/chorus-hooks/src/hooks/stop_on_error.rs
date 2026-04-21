//! Stop-on-error gate (#1841)
//!
//! PostToolUse hook that blocks the next tool call when the previous one errored.
//! Roles must stop, diagnose, and fix — not barrel through errors silently.
//!
//! Exemptions for known-benign exit patterns (grep no-match, diff with differences, etc.)

use crate::state::{chorus_log, AppState};
use crate::types::{HookInput, HookResponse};
use regex::Regex;
use std::sync::LazyLock;
use tracing::info;

/// Matches Claude Code's error output format: "Exit code N" or "Error: Exit code N"
static EXIT_CODE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)exit code ([1-9]\d*)").unwrap()
});

/// Commands where non-zero exit is expected/normal behavior
static BENIGN_COMMANDS: &[&str] = &[
    "grep",       // exit 1 = no matches
    "rg",         // exit 1 = no matches
    "diff",       // exit 1 = differences found (not an error)
    "test ",      // exit 1 = condition false
    "[ ",         // test bracket form
    "which ",     // exit 1 = not found (informational)
    "command -v", // exit 1 = not found (informational)
    "git status", // sometimes exits non-zero for edge cases
    "git diff",   // exit 1 = differences found
];

/// Commands that are always exempt (output is noisy/expected to contain "error")
static EXEMPT_COMMANDS: &[&str] = &[
    "cargo test",    // test runners report failures in output, role handles them
    "cargo build",   // compiler warnings contain "error" keyword
    "npm test",
    "npx jest",
    "npx vitest",
    "pytest",
    "smoke-check",   // smoke check manages its own error reporting
    "board-ts",      // board CLI has its own error handling
    "/cards ",       // board CLI alias
    "chorus-log",    // meta — don't block on logging failures
    "role-state",    // state management
    "git commit",    // pre-commit hooks may fail, handled by git flow
    "git push",      // push failures handled by acp flow
    "git-queue",     // commit flow has own error handling
];

/// Extract the error-relevant portion of tool_response.
/// Claude Code sends Bash results as either a string or an object with stdout/stderr fields.
/// We only want to match "Exit code N" in the stderr portion or at the top level,
/// NOT inside stdout data that happens to contain that text.
fn extract_error_text(input: &HookInput) -> String {
    match &input.tool_response {
        Some(serde_json::Value::Object(obj)) => {
            // Structured response — check stderr field first, then top-level string representation
            let stderr = obj.get("stderr")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            // Also check if there's an "error" field
            let error = obj.get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            format!("{}\n{}", stderr, error)
        }
        Some(serde_json::Value::String(s)) => {
            // Plain string response — check the first few lines only
            // to avoid matching error text deep inside command output
            s.lines().take(5).collect::<Vec<_>>().join("\n")
        }
        _ => String::new(),
    }
}

/// Check if a Bash tool call resulted in an error that should stop the role
pub async fn check(input: &HookInput, _state: &AppState) -> HookResponse {
    if input.tool_name_str() != "Bash" {
        return HookResponse::allow();
    }

    let response = extract_error_text(input);
    if response.is_empty() {
        return HookResponse::allow();
    }

    let command = input.get_tool_input_str("command");
    let cmd_first_line = command.lines().next().unwrap_or("");

    // Check exempt commands first
    for pat in EXEMPT_COMMANDS {
        if cmd_first_line.contains(pat) {
            return HookResponse::allow();
        }
    }

    // Look for non-zero exit code in the response
    let exit_code = match EXIT_CODE_RE.captures(&response) {
        Some(caps) => caps.get(1).and_then(|m| m.as_str().parse::<i32>().ok()).unwrap_or(0),
        None => return HookResponse::allow(),
    };

    if exit_code == 0 {
        return HookResponse::allow();
    }

    // Check benign commands — non-zero exit is expected
    for pat in BENIGN_COMMANDS {
        if cmd_first_line.contains(pat) {
            return HookResponse::allow();
        }
    }

    // Also exempt if the command is in a conditional chain (|| or &&)
    // where the exit code is handled
    if cmd_first_line.contains("|| true")
        || cmd_first_line.contains("|| echo")
        || cmd_first_line.contains("|| :")
        || cmd_first_line.contains("; true")
        || cmd_first_line.ends_with("2>/dev/null")
        || cmd_first_line.ends_with("2>&1 || true")
    {
        return HookResponse::allow();
    }

    // This is a real error. Extract the error line for the message.
    // Use full tool_response for the error context message
    let full_response = input.tool_response_str();
    let error_line = full_response
        .lines()
        .find(|l| {
            l.contains("Error") || l.contains("error") || l.contains("FAILED")
                || l.contains("fatal") || l.contains("No such file")
                || l.contains("not found") || l.contains("Permission denied")
        })
        .unwrap_or("(see output above)");

    let error_short: String = error_line.chars().take(200).collect();
    let cmd_short: String = cmd_first_line.chars().take(120).collect();
    let role = input.role();

    info!(
        role = role.as_str(),
        exit_code,
        command = cmd_short.as_str(),
        "stop-on-error: blocking next action"
    );

    // Emit spine event
    chorus_log(
        "tool.error.blocked",
        role.as_str(),
        &[
            ("exit_code", &exit_code.to_string()),
            ("command", &cmd_short),
            ("error", &error_short),
        ],
    )
    .await;

    HookResponse::block_with_stderr(&format!(
        "Stop-on-error: previous command exited {exit_code}.\n  cmd: {cmd_short}\n  error: {error_short}\nDiagnose and fix before continuing. Do not proceed past errors silently."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    use crate::shared::state_paths::chorus_root;

    /// Helper: make a Bash input with structured tool_response (how Claude Code sends it)
    fn make_bash_input(command: &str, stderr: &str, stdout: &str) -> HookInput {
        HookInput {
            tool_name: Some("Bash".to_string()),
            tool_input: Some(json!({"command": command})),
            tool_response: Some(json!({
                "stdout": stdout,
                "stderr": stderr,
                "interrupted": false,
                "isImage": false,
            })),
            session_id: Some("test".to_string()),
            cwd: Some(format!("{}/architect", chorus_root())),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("silas".to_string()),
        }
    }

    /// Helper: make a Bash input with plain string response (some hooks send this way)
    fn make_bash_input_str(command: &str, response: &str) -> HookInput {
        HookInput {
            tool_name: Some("Bash".to_string()),
            tool_input: Some(json!({"command": command})),
            tool_response: Some(json!(response)),
            session_id: Some("test".to_string()),
            cwd: Some(format!("{}/architect", chorus_root())),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("silas".to_string()),
        }
    }

    #[tokio::test]
    async fn test_blocks_on_exit_code_in_stderr() {
        let state = AppState::new();
        let input = make_bash_input(
            "ls /nonexistent",
            "Exit code 1\nls: /nonexistent: No such file or directory",
            "",
        );
        let r = check(&input, &state).await;
        assert_eq!(r.exit_code, 2, "should block");
        assert!(r.stderr.is_some());
        assert!(r.stderr.unwrap().contains("Stop-on-error"));
    }

    #[tokio::test]
    async fn test_blocks_on_exit_code_126() {
        let state = AppState::new();
        let input = make_bash_input(
            "bash /some/script.sh",
            "Exit code 126\n/some/script.sh: Permission denied",
            "",
        );
        let r = check(&input, &state).await;
        assert_eq!(r.exit_code, 2, "should block on exit 126");
    }

    #[tokio::test]
    async fn test_allows_success() {
        let state = AppState::new();
        let input = make_bash_input("echo hello", "", "hello");
        let r = check(&input, &state).await;
        assert_eq!(r.exit_code, 0);
        assert!(r.stderr.is_none());
    }

    #[tokio::test]
    async fn test_allows_grep_no_match() {
        let state = AppState::new();
        let input = make_bash_input(
            "grep -r 'nonexistent' .",
            "Exit code 1",
            "",
        );
        let r = check(&input, &state).await;
        assert_eq!(r.exit_code, 0, "grep exit 1 is benign");
    }

    #[tokio::test]
    async fn test_allows_diff_with_differences() {
        let state = AppState::new();
        let input = make_bash_input(
            "diff file1 file2",
            "Exit code 1",
            "< line1\n> line2",
        );
        let r = check(&input, &state).await;
        assert_eq!(r.exit_code, 0, "diff exit 1 is benign");
    }

    #[tokio::test]
    async fn test_allows_test_command() {
        let state = AppState::new();
        let input = make_bash_input(
            "test -f /nonexistent",
            "Exit code 1",
            "",
        );
        let r = check(&input, &state).await;
        assert_eq!(r.exit_code, 0, "test exit 1 is benign");
    }

    #[tokio::test]
    async fn test_allows_cargo_test() {
        let state = AppState::new();
        let input = make_bash_input(
            "cargo test 2>&1",
            "Exit code 1",
            "test result: FAILED. 1 passed; 1 failed",
        );
        let r = check(&input, &state).await;
        assert_eq!(r.exit_code, 0, "cargo test is exempt");
    }

    #[tokio::test]
    async fn test_allows_or_true_pattern() {
        let state = AppState::new();
        let input = make_bash_input(
            "some-cmd || true",
            "Exit code 1",
            "some error",
        );
        let r = check(&input, &state).await;
        assert_eq!(r.exit_code, 0, "|| true pattern is exempt");
    }

    #[tokio::test]
    async fn test_allows_non_bash_tools() {
        let state = AppState::new();
        let input = HookInput {
            tool_name: Some("Read".to_string()),
            tool_input: Some(json!({"file_path": "/x"})),
            tool_response: Some(json!({"stderr": "Exit code 1", "stdout": ""})),
            session_id: Some("test".to_string()),
            cwd: Some(format!("{}/architect", chorus_root())),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("silas".to_string()),
        };
        let r = check(&input, &state).await;
        assert_eq!(r.exit_code, 0, "only Bash triggers the gate");
    }

    #[tokio::test]
    async fn test_blocks_script_not_found() {
        let state = AppState::new();
        let input = make_bash_input(
            &format!("bash {}/platform/scripts/missing.sh", chorus_root()),
            "Exit code 127\nbash: No such file or directory",
            "",
        );
        let r = check(&input, &state).await;
        assert_eq!(r.exit_code, 2, "should block on missing script");
    }

    #[tokio::test]
    async fn test_allows_board_ts() {
        let state = AppState::new();
        let input = make_bash_input(
            &format!("bash {}/platform/scripts/cards view 1808", chorus_root()),
            "Exit code 1",
            "Card not found",
        );
        let r = check(&input, &state).await;
        assert_eq!(r.exit_code, 0, "board-ts/cards is exempt");
    }

    #[tokio::test]
    async fn test_no_false_positive_on_stdout_containing_exit_code() {
        let state = AppState::new();
        // Command succeeds but stdout contains "Exit code 127" as data
        let input = make_bash_input(
            "curl -s --unix-socket /tmp/test.sock http://localhost/test",
            "",
            "{\"stderr\": \"Stop-on-error: previous command exited 127.\\n  cmd: bash /broken/script.sh\\n  error: Error: Exit code 127\"}",
        );
        let r = check(&input, &state).await;
        assert_eq!(r.exit_code, 0, "should NOT trigger on error text in stdout data");
    }

    #[tokio::test]
    async fn test_string_response_only_checks_first_lines() {
        let state = AppState::new();
        // Plain string response where "Exit code" appears deep in output
        let input = make_bash_input_str(
            "some-command",
            "line1\nline2\nline3\nline4\nline5\nline6\nExit code 1\nthe error",
        );
        let r = check(&input, &state).await;
        assert_eq!(r.exit_code, 0, "should not match Exit code deep in string output");
    }

    #[tokio::test]
    async fn allows_npm_test_failures() {
        let state = AppState::new();
        let input = make_bash_input("npm test", "Exit code 1\nTests failed", "");
        let r = check(&input, &state).await;
        assert_eq!(r.exit_code, 0, "npm test is exempt");
    }

    #[tokio::test]
    async fn allows_smoke_check_failures() {
        let state = AppState::new();
        let input = make_bash_input("smoke-check.sh --all", "Exit code 1\n2 failures", "");
        let r = check(&input, &state).await;
        assert_eq!(r.exit_code, 0, "smoke-check is exempt");
    }

    #[tokio::test]
    async fn allows_which_not_found() {
        let state = AppState::new();
        let input = make_bash_input("which nonexistent", "Exit code 1", "");
        let r = check(&input, &state).await;
        assert_eq!(r.exit_code, 0, "which is benign");
    }

    #[tokio::test]
    async fn allows_stderr_redirect() {
        let state = AppState::new();
        let input = make_bash_input("cmd 2>/dev/null", "Exit code 1\nsome error", "");
        let r = check(&input, &state).await;
        assert_eq!(r.exit_code, 0, "2>/dev/null pattern is exempt");
    }

    #[tokio::test]
    async fn allows_rg_no_match() {
        let state = AppState::new();
        let input = make_bash_input("rg 'pattern' .", "Exit code 1", "");
        let r = check(&input, &state).await;
        assert_eq!(r.exit_code, 0, "rg exit 1 is benign");
    }

    #[tokio::test]
    async fn allows_git_status() {
        let state = AppState::new();
        let input = make_bash_input("git status --porcelain", "Exit code 1", "");
        let r = check(&input, &state).await;
        assert_eq!(r.exit_code, 0, "git status is benign");
    }

    #[tokio::test]
    async fn allows_git_diff() {
        let state = AppState::new();
        let input = make_bash_input("git diff HEAD", "Exit code 1\ndifferences found", "");
        let r = check(&input, &state).await;
        assert_eq!(r.exit_code, 0, "git diff is benign");
    }
}
