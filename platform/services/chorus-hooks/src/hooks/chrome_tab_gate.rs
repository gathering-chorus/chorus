//! Chrome tab gate (#1775, DEC-090)
//!
//! PreToolUse on Bash: blocks role-initiated `open http` commands that would
//! open tabs in Jeff's Chrome window. Redirects to chrome-window.sh which
//! opens in the role's own Chrome window.
//!
//! Exception: if Jeff's message in the current turn contains URL-open intent
//! (e.g., "open this", "show me in browser"), the command is allowed —
//! Jeff asked for it.

use crate::types::{HookInput, HookResponse};

pub async fn check(input: &HookInput) -> HookResponse {
    if input.tool_name_str() != "Bash" {
        return HookResponse::allow();
    }

    let cmd = input.get_tool_input_str("command");
    if cmd.is_empty() {
        return HookResponse::allow();
    }

    // Check if the command opens a URL via `open`
    let has_open_http = cmd.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.starts_with("open http")
            || trimmed.starts_with("open \"http")
            || trimmed.starts_with("open 'http")
    });

    if !has_open_http {
        return HookResponse::allow();
    }

    // Exception: Jeff asked to open something
    let prompt = input.prompt.as_deref().unwrap_or("");
    let jeff_requested = prompt.contains("open")
        && (prompt.contains("http") || prompt.contains("browser") || prompt.contains("show me")
            || prompt.contains("in chrome") || prompt.contains("in tab"));

    if jeff_requested {
        return HookResponse::allow();
    }

    HookResponse::block_with_stderr(
        "BLOCKED: DEC-090 — don't open URLs in Jeff's browser. Use chrome-window.sh instead:\n  \
         bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chrome-window.sh <url>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HookInput;
    use serde_json::json;

    fn make_input(cmd: &str, prompt: &str) -> HookInput {
        HookInput {
            tool_name: Some("Bash".into()),
            tool_input: Some(json!({"command": cmd})),
            tool_response: None,
            session_id: Some("t".into()),
            cwd: Some("/tmp".into()),
            prompt: Some(prompt.into()),
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("silas".into()),
        }
    }

    #[tokio::test]
    async fn blocks_open_http() {
        let input = make_input("open http://localhost:3000/test", "check the page");
        let r = check(&input).await;
        assert!(r.stderr.is_some(), "should block open http");
    }

    #[tokio::test]
    async fn allows_jeff_requested_open() {
        let input = make_input("open http://localhost:3000/test", "open this in browser for me");
        let r = check(&input).await;
        assert!(r.stderr.is_none(), "should allow Jeff-requested open");
    }

    #[tokio::test]
    async fn allows_non_open_commands() {
        let input = make_input("curl http://localhost:3000/test", "check health");
        let r = check(&input).await;
        assert!(r.stderr.is_none(), "should allow curl");
    }

    #[tokio::test]
    async fn allows_chrome_window_sh() {
        let input = make_input("bash chrome-window.sh http://localhost:3000", "open page");
        let r = check(&input).await;
        assert!(r.stderr.is_none(), "should allow chrome-window.sh");
    }
}
