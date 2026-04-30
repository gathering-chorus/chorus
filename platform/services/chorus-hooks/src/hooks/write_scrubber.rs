use crate::state::chorus_log;
use crate::types::{permission_deny_json, HookInput, HookResponse};
use regex::Regex;
use std::sync::LazyLock;

static SECRET_VALUE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(password|token|secret|api_key|apikey|auth_token)\s*[=:]\s*\S+").unwrap()
});

static AWS_KEY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"AKIA[A-Z0-9]{16}").unwrap()
});

static SLACK_TOKEN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"xox[bpras]-[A-Za-z0-9\-]{10,}").unwrap()
});

static GITHUB_TOKEN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"ghp_[A-Za-z0-9]{36}").unwrap()
});

static API_KEY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"sk-[A-Za-z0-9]{20,}").unwrap()
});

static IPV4_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\b").unwrap()
});

static MAC_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)([0-9A-Fa-f]{2}[:-]){5}[0-9A-Fa-f]{2}").unwrap()
});

static LOCALHOST_PORT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"localhost:[0-9]{4,5}").unwrap()
});

fn is_shared_file(path: &str) -> bool {
    path.ends_with("/activity.md")
        || path.ends_with("/MEMORY.md")
        || path.contains("/memory/") && path.ends_with(".md")
        || path.ends_with("/current-work.md")
        || path.ends_with("/tech-debt.md")
        || path.ends_with("/decisions.md")
        || path.ends_with("/system-architecture.md")
        || path.ends_with("/TEAM_PROTOCOL.md")
        || path.ends_with("/team-architecture.md")
        || path.contains("/.claude/settings") && path.ends_with(".json")
        || path.ends_with("/CLAUDE.md")
}

pub async fn check(input: &HookInput) -> HookResponse {
    let tool = input.tool_name_str();
    if tool != "Write" && tool != "Edit" {
        return HookResponse::allow();
    }
    tracing::trace!(hook = "write_scrubber", phase = "receive", tool, "checking write/edit");

    let file_path = input.get_tool_input_str("file_path");
    if file_path.is_empty() || !is_shared_file(&file_path) {
        return HookResponse::allow();
    }

    let content = if tool == "Write" {
        input.get_tool_input_str("content")
    } else {
        input.get_tool_input_str("new_string")
    };

    if content.is_empty() {
        return HookResponse::allow();
    }

    // PRIVATE PATTERNS — hard deny
    if SECRET_VALUE_RE.is_match(&content) {
        log_scrub("deny", "private", "secret-value", &file_path).await;
        return HookResponse::deny(&permission_deny_json(
            "BLOCKED: Content contains a credential pattern (password=, token=, secret=, api_key=). Remove the sensitive value before writing to shared files."
        ));
    }

    if AWS_KEY_RE.is_match(&content) {
        log_scrub("deny", "private", "aws-key", &file_path).await;
        return HookResponse::deny(&permission_deny_json(
            "BLOCKED: Content contains an AWS access key (AKIA...). Never write credentials to shared files."
        ));
    }

    if SLACK_TOKEN_RE.is_match(&content) {
        log_scrub("deny", "private", "slack-token", &file_path).await;
        return HookResponse::deny(&permission_deny_json(
            "BLOCKED: Content contains a Slack token (xoxb/xoxp/...). Never write tokens to shared files."
        ));
    }

    if GITHUB_TOKEN_RE.is_match(&content) {
        log_scrub("deny", "private", "github-token", &file_path).await;
        return HookResponse::deny(&permission_deny_json(
            "BLOCKED: Content contains a GitHub token (ghp_...). Never write tokens to shared files."
        ));
    }

    if API_KEY_RE.is_match(&content) {
        log_scrub("deny", "private", "api-key", &file_path).await;
        return HookResponse::deny(&permission_deny_json(
            "BLOCKED: Content contains an API key (sk-...). Never write API keys to shared files."
        ));
    }

    // INTERNAL PATTERNS — warn and log (don't block)
    if IPV4_RE.is_match(&content) {
        log_scrub("warn", "internal", "ipv4", &file_path).await;
    }
    if MAC_RE.is_match(&content) {
        log_scrub("warn", "internal", "mac", &file_path).await;
    }
    if LOCALHOST_PORT_RE.is_match(&content) {
        log_scrub("warn", "internal", "localhost-port", &file_path).await;
    }

    HookResponse::allow()
}

async fn log_scrub(decision: &str, tier: &str, pattern: &str, path: &str) {
    tracing::trace!(
        hook = "write_scrubber",
        phase = "execute",
        decision,
        tier,
        pattern,
        path,
        "scrub decision"
    );
    chorus_log(
        "guard.scrub.blocked",
        "system",
        &[
            ("decision", decision),
            ("tier", tier),
            ("pattern", pattern),
            ("path", path),
        ],
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HookInput;
    use crate::shared::state_paths::chorus_root;
    use serde_json::json;

    fn make_write_input(file_path: &str, content: &str) -> HookInput {
        HookInput {
            tool_name: Some("Write".to_string()),
            tool_input: Some(json!({
                "file_path": file_path,
                "content": content,
            })),
            tool_response: None,
            session_id: None,
            cwd: Some(format!("{}/architect", chorus_root())),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: None,
            chorus_worktree_override: None,}
    }

    fn make_edit_input(file_path: &str, new_string: &str) -> HookInput {
        HookInput {
            tool_name: Some("Edit".to_string()),
            tool_input: Some(json!({
                "file_path": file_path,
                "old_string": "old",
                "new_string": new_string,
            })),
            tool_response: None,
            session_id: None,
            cwd: Some(format!("{}/architect", chorus_root())),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: None,
            chorus_worktree_override: None,}
    }

    // === Shared file detection ===

    #[test]
    fn test_is_shared_file_activity() {
        assert!(is_shared_file("/some/path/activity.md"));
    }

    #[test]
    fn test_is_shared_file_memory() {
        assert!(is_shared_file("/some/path/memory/user_role.md"));
        assert!(is_shared_file("/some/path/MEMORY.md"));
    }

    #[test]
    fn test_is_shared_file_claude_md() {
        assert!(is_shared_file("/some/path/CLAUDE.md"));
    }

    #[test]
    fn test_is_shared_file_settings() {
        assert!(is_shared_file("/some/.claude/settings.json"));
        assert!(is_shared_file("/some/.claude/settings.local.json"));
    }

    #[test]
    fn test_not_shared_file() {
        assert!(!is_shared_file("/some/path/random.rs"));
        assert!(!is_shared_file("/some/path/handler.ts"));
    }

    // === Private patterns (hard deny) ===

    #[tokio::test]
    async fn test_deny_secret_value_password() {
        let input = make_write_input("/x/activity.md", "config password=hunter2 here");
        let r = check(&input).await;
        assert!(r.stdout.is_some());
        assert!(r.stdout.unwrap().contains("BLOCKED"));
    }

    #[tokio::test]
    async fn test_deny_secret_value_token() {
        let input = make_write_input("/x/activity.md", "auth_token = abc123secret");
        let r = check(&input).await;
        assert!(r.stdout.is_some());
        assert!(r.stdout.unwrap().contains("BLOCKED"));
    }

    #[tokio::test]
    async fn test_deny_secret_value_api_key() {
        let input = make_write_input("/x/activity.md", "api_key: my-secret-key");
        let r = check(&input).await;
        assert!(r.stdout.is_some());
        assert!(r.stdout.unwrap().contains("BLOCKED"));
    }

    #[tokio::test]
    async fn test_deny_aws_key() {
        let input = make_write_input("/x/CLAUDE.md", "key = AKIAIOSFODNN7EXAMPLE");
        let r = check(&input).await;
        assert!(r.stdout.is_some());
        assert!(r.stdout.unwrap().contains("AWS"));
    }

    #[tokio::test]
    async fn test_deny_slack_token_xoxb() {
        // Use content without "token:" prefix so it doesn't hit SECRET_VALUE_RE first
        let input = make_write_input("/x/activity.md", "use xoxb-abc123-def456-ghijkl here");
        let r = check(&input).await;
        assert!(r.stdout.is_some());
        assert!(r.stdout.unwrap().contains("Slack"));
    }

    #[tokio::test]
    async fn test_deny_slack_token_xoxp() {
        let input = make_write_input("/x/activity.md", "SLACK_TOKEN=xoxp-1234567890-abcdef");
        let r = check(&input).await;
        // This hits secret_value first (SLACK_TOKEN=...)
        assert!(r.stdout.is_some());
        assert!(r.stdout.unwrap().contains("BLOCKED"));
    }

    #[tokio::test]
    async fn test_deny_github_token() {
        let input = make_write_input("/x/MEMORY.md", "ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij");
        let r = check(&input).await;
        assert!(r.stdout.is_some());
        assert!(r.stdout.unwrap().contains("GitHub"));
    }

    #[tokio::test]
    async fn test_deny_api_key_sk() {
        let input = make_write_input("/x/activity.md", "api: sk-abcdefghijklmnopqrstuvwxyz");
        let r = check(&input).await;
        assert!(r.stdout.is_some());
        assert!(r.stdout.unwrap().contains("API key"));
    }

    // === Internal patterns (warn, don't block) ===

    #[tokio::test]
    async fn test_warn_ipv4_allows() {
        let input = make_write_input("/x/activity.md", "server at 192.168.86.36 is up");
        let r = check(&input).await;
        // Should allow — warn is non-blocking
        assert!(r.stdout.is_none());
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_warn_mac_allows() {
        let input = make_write_input("/x/activity.md", "device AA:BB:CC:DD:EE:FF");
        let r = check(&input).await;
        assert!(r.stdout.is_none());
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_warn_localhost_port_allows() {
        let input = make_write_input("/x/activity.md", "running on localhost:3456");
        let r = check(&input).await;
        assert!(r.stdout.is_none());
        assert_eq!(r.exit_code, 0);
    }

    // === Non-shared files bypass all checks ===

    #[tokio::test]
    async fn test_allow_non_shared_file_with_secrets() {
        let input = make_write_input("/some/random.rs", "password=hunter2");
        let r = check(&input).await;
        assert!(r.stdout.is_none());
        assert_eq!(r.exit_code, 0);
    }

    // === Non-write tools bypass ===

    #[tokio::test]
    async fn test_allow_read_tool() {
        let input = HookInput {
            tool_name: Some("Read".to_string()),
            tool_input: Some(json!({"file_path": "/x/activity.md"})),
            tool_response: None,
            session_id: None,
            cwd: None,
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: None,
            chorus_worktree_override: None,};
        let r = check(&input).await;
        assert!(r.stdout.is_none());
        assert_eq!(r.exit_code, 0);
    }

    // === Edit tool also checked ===

    #[tokio::test]
    async fn test_deny_edit_with_aws_key() {
        let input = make_edit_input("/x/activity.md", "AKIAIOSFODNN7EXAMPLE");
        let r = check(&input).await;
        assert!(r.stdout.is_some());
        assert!(r.stdout.unwrap().contains("AWS"));
    }

    // === Clean content allowed ===

    #[tokio::test]
    async fn test_allow_clean_content() {
        let input = make_write_input("/x/activity.md", "Updated the board with new card #1594");
        let r = check(&input).await;
        assert!(r.stdout.is_none());
        assert_eq!(r.exit_code, 0);
    }
}
