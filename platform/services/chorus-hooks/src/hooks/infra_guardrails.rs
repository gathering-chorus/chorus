use crate::state::chorus_log;
use crate::types::{permission_ask_json, permission_deny_json, HookInput, HookResponse, Role};
use regex::Regex;
use std::sync::LazyLock;

static DOCKER_EXEC_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bdocker\s+exec\b").unwrap()
});

static DOCKER_LOGS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bdocker\s+logs\b").unwrap()
});

static DOCKER_LOGS_FOLLOW_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\s(-f|--follow)\b").unwrap()
});

static DOCKER_LOGS_TAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"--tail[= ](\d+)").unwrap()
});

static KILL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(kill|pkill|killall)\s").unwrap()
});

static DOCKER_LIFECYCLE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bdocker\s+(stop|rm|restart|kill)\b").unwrap()
});

static DOCKER_COMPOSE_DOWN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bdocker[\s-]compose\s+down\b").unwrap()
});

static GIT_COMMIT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bgit\s+commit\b").unwrap()
});

static GIT_ADD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bgit\s+add\b").unwrap()
});

static HEREDOC_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<<['"]?EOF"#).unwrap()
});

static DOCKER_RUN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bdocker\s+run\b").unwrap()
});

static TERRAFORM_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bterraform\s+(apply|destroy)\b").unwrap()
});

const TEAM_REPO_ROOT: &str = "/Users/jeffbridwell/CascadeProjects";

/// Blocks dangerous infra commands for all roles (#1714)
pub async fn check(input: &HookInput) -> HookResponse {
    if input.tool_name_str() != "Bash" {
        return HookResponse::allow();
    }

    let cmd = input.get_tool_input_str("command");
    if cmd.is_empty() {
        return HookResponse::allow();
    }
    tracing::trace!(hook = "infra_guardrails", phase = "receive", cmd_len = cmd.len(), "checking bash command");

    // docker exec
    if DOCKER_EXEC_RE.is_match(&cmd) {
        log_guardrail("deny", "docker-exec").await;
        return HookResponse::deny(&permission_deny_json(
            "BLOCKED: docker exec is prohibited (ADR-011). Fix the code and redeploy using app-state.sh deploy. If you need read-only inspection for debugging, ask Jeff for permission first."
        ));
    }

    // docker logs
    if DOCKER_LOGS_RE.is_match(&cmd) {
        if DOCKER_LOGS_FOLLOW_RE.is_match(&cmd) {
            log_guardrail("deny", "docker-logs-follow").await;
            return HookResponse::deny(&permission_deny_json(
                "BLOCKED: docker logs --follow is prohibited. Use Loki for real-time log streaming. For crash diagnostics, use: docker logs --tail 20 <container>"
            ));
        }

        if let Some(caps) = DOCKER_LOGS_TAIL_RE.captures(&cmd) {
            if let Ok(n) = caps[1].parse::<u32>() {
                if n <= 50 {
                    log_guardrail("allow", "docker-logs-tail").await;
                    return HookResponse::allow();
                }
            }
        }

        log_guardrail("deny", "docker-logs").await;
        return HookResponse::deny(&permission_deny_json(
            "BLOCKED: docker logs without --tail is prohibited. Use Loki for log search: Grafana at http://localhost:3100 → Explore → Loki. For crash diagnostics (container died before Promtail scraped): docker logs --tail 20 <container>"
        ));
    }

    // kill/pkill/killall
    if KILL_RE.is_match(&cmd) {
        log_guardrail("deny", "kill").await;
        return HookResponse::deny(&permission_deny_json(
            "BLOCKED: Manual process killing is prohibited. Use app-state.sh stop for graceful shutdown."
        ));
    }

    // docker stop/rm/restart/kill
    if DOCKER_LIFECYCLE_RE.is_match(&cmd) {
        log_guardrail("deny", "docker-lifecycle").await;
        return HookResponse::deny(&permission_deny_json(
            "BLOCKED: Direct Docker lifecycle commands are prohibited. Use app-state.sh (start|stop|restart|status)."
        ));
    }

    // docker compose down
    if DOCKER_COMPOSE_DOWN_RE.is_match(&cmd) {
        log_guardrail("deny", "docker-compose-down").await;
        return HookResponse::deny(&permission_deny_json(
            "BLOCKED: docker compose down is prohibited. Use app-state.sh stop."
        ));
    }

    // git commit/add in team repo only
    if GIT_COMMIT_RE.is_match(&cmd) || GIT_ADD_RE.is_match(&cmd) {
        // Skip heredocs
        if HEREDOC_RE.is_match(&cmd) {
            log_guardrail("allow", "git-in-heredoc").await;
        } else {
            // Check if we're in the team repo
            // Simple heuristic: if the command doesn't cd elsewhere, assume team repo for Kade
            let in_team_repo = !cmd.contains("cd ") || cmd.contains(TEAM_REPO_ROOT);

            if in_team_repo {
                if GIT_COMMIT_RE.is_match(&cmd) {
                    log_guardrail("deny", "git-commit").await;
                    return HookResponse::deny(&permission_deny_json(
                        "BLOCKED: Direct git commit is prohibited in the team repo. Use git-queue.sh which serializes commits across roles with lockf."
                    ));
                }
                if GIT_ADD_RE.is_match(&cmd) {
                    log_guardrail("deny", "git-add").await;
                    return HookResponse::deny(&permission_deny_json(
                        "BLOCKED: Direct git add is prohibited in the team repo. Use git-queue.sh which performs atomic add+commit under lock."
                    ));
                }
            }
        }
    }

    // docker run (ask, not deny)
    if DOCKER_RUN_RE.is_match(&cmd) {
        log_guardrail("ask", "docker-run").await;
        return HookResponse::deny(&permission_ask_json(
            "docker run detected. Containers should be managed through app-state.sh and Terraform, not run directly. Is this a temporary test container? If so, Jeff must approve."
        ));
    }

    // terraform apply/destroy (ask, not deny)
    if TERRAFORM_RE.is_match(&cmd) {
        log_guardrail("ask", "terraform-direct").await;
        return HookResponse::deny(&permission_ask_json(
            "Direct terraform apply/destroy detected. These should go through app-state.sh which wraps Terraform with health checks and verification."
        ));
    }

    HookResponse::allow()
}

async fn log_guardrail(decision: &str, pattern: &str) {
    tracing::trace!(
        hook = "infra_guardrails",
        phase = "execute",
        decision,
        pattern,
        "guardrail decision"
    );
    chorus_log(
        "guard.rule.decided",
        "system",
        &[("decision", decision), ("pattern", pattern)],
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HookInput;
    use serde_json::json;

    fn kade_bash(cmd: &str) -> HookInput {
        HookInput {
            tool_name: Some("Bash".to_string()),
            tool_input: Some(json!({"command": cmd})),
            tool_response: None,
            session_id: None,
            cwd: Some("/Users/jeffbridwell/CascadeProjects/engineer".to_string()),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: None,
        }
    }

    fn silas_bash(cmd: &str) -> HookInput {
        HookInput {
            tool_name: Some("Bash".to_string()),
            tool_input: Some(json!({"command": cmd})),
            tool_response: None,
            session_id: None,
            cwd: Some("/Users/jeffbridwell/CascadeProjects/architect".to_string()),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: None,
        }
    }

    // === Role gating: all roles blocked (#1714) ===

    #[tokio::test]
    async fn test_silas_blocked_by_docker_exec() {
        let input = silas_bash("docker exec -it mycontainer bash");
        let r = check(&input).await;
        // All roles are blocked from docker exec — use app-state.sh
        assert!(r.stdout.is_some(), "docker exec should be denied for silas");
        assert!(r.stdout.unwrap().contains("deny"));
    }

    // === docker exec ===

    #[tokio::test]
    async fn test_deny_docker_exec() {
        let input = kade_bash("docker exec -it gathering-app bash");
        let r = check(&input).await;
        assert!(r.stdout.is_some());
        assert!(r.stdout.unwrap().contains("docker exec is prohibited"));
    }

    // === docker logs ===

    #[tokio::test]
    async fn test_deny_docker_logs_follow() {
        let input = kade_bash("docker logs -f gathering-app");
        let r = check(&input).await;
        assert!(r.stdout.is_some());
        assert!(r.stdout.unwrap().contains("docker logs --follow is prohibited"));
    }

    #[tokio::test]
    async fn test_deny_docker_logs_no_tail() {
        let input = kade_bash("docker logs gathering-app");
        let r = check(&input).await;
        assert!(r.stdout.is_some());
        assert!(r.stdout.unwrap().contains("docker logs without --tail"));
    }

    #[tokio::test]
    async fn test_allow_docker_logs_tail_20() {
        let input = kade_bash("docker logs --tail 20 gathering-app");
        let r = check(&input).await;
        assert!(r.stdout.is_none());
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_deny_docker_logs_tail_100() {
        let input = kade_bash("docker logs --tail 100 gathering-app");
        let r = check(&input).await;
        assert!(r.stdout.is_some());
        assert!(r.stdout.unwrap().contains("docker logs without --tail"));
    }

    // === kill/pkill/killall ===

    #[tokio::test]
    async fn test_deny_kill() {
        let input = kade_bash("kill -9 12345");
        let r = check(&input).await;
        assert!(r.stdout.is_some());
        assert!(r.stdout.unwrap().contains("Manual process killing"));
    }

    #[tokio::test]
    async fn test_deny_pkill() {
        let input = kade_bash("pkill node");
        let r = check(&input).await;
        assert!(r.stdout.is_some());
        assert!(r.stdout.unwrap().contains("Manual process killing"));
    }

    #[tokio::test]
    async fn test_deny_killall() {
        let input = kade_bash("killall node");
        let r = check(&input).await;
        assert!(r.stdout.is_some());
        assert!(r.stdout.unwrap().contains("Manual process killing"));
    }

    // === docker lifecycle ===

    #[tokio::test]
    async fn test_deny_docker_stop() {
        let input = kade_bash("docker stop gathering-app");
        let r = check(&input).await;
        assert!(r.stdout.is_some());
        assert!(r.stdout.unwrap().contains("Docker lifecycle"));
    }

    #[tokio::test]
    async fn test_deny_docker_rm() {
        let input = kade_bash("docker rm gathering-app");
        let r = check(&input).await;
        assert!(r.stdout.is_some());
        assert!(r.stdout.unwrap().contains("Docker lifecycle"));
    }

    #[tokio::test]
    async fn test_deny_docker_restart() {
        let input = kade_bash("docker restart gathering-app");
        let r = check(&input).await;
        assert!(r.stdout.is_some());
        assert!(r.stdout.unwrap().contains("Docker lifecycle"));
    }

    // === docker compose down ===

    #[tokio::test]
    async fn test_deny_docker_compose_down() {
        let input = kade_bash("docker compose down");
        let r = check(&input).await;
        assert!(r.stdout.is_some());
        assert!(r.stdout.unwrap().contains("docker compose down"));
    }

    #[tokio::test]
    async fn test_deny_docker_dash_compose_down() {
        let input = kade_bash("docker-compose down");
        let r = check(&input).await;
        assert!(r.stdout.is_some());
        assert!(r.stdout.unwrap().contains("docker compose down"));
    }

    // === git commit/add in team repo ===

    #[tokio::test]
    async fn test_deny_git_commit() {
        let input = kade_bash("git commit -m 'test'");
        let r = check(&input).await;
        assert!(r.stdout.is_some());
        assert!(r.stdout.unwrap().contains("git-queue.sh"));
    }

    #[tokio::test]
    async fn test_deny_git_add() {
        let input = kade_bash("git add .");
        let r = check(&input).await;
        assert!(r.stdout.is_some());
        assert!(r.stdout.unwrap().contains("git-queue.sh"));
    }

    #[tokio::test]
    async fn test_allow_git_in_heredoc() {
        let input = kade_bash("cat <<'EOF'\ngit commit -m 'example'\nEOF");
        let r = check(&input).await;
        assert!(r.stdout.is_none());
        assert_eq!(r.exit_code, 0);
    }

    // === docker run (ask, not deny) ===

    #[tokio::test]
    async fn test_ask_docker_run() {
        let input = kade_bash("docker run -d nginx");
        let r = check(&input).await;
        assert!(r.stdout.is_some());
        assert!(r.stdout.unwrap().contains("docker run detected"));
    }

    // === terraform (ask, not deny) ===

    #[tokio::test]
    async fn test_ask_terraform_apply() {
        let input = kade_bash("terraform apply");
        let r = check(&input).await;
        assert!(r.stdout.is_some());
        assert!(r.stdout.unwrap().contains("terraform"));
    }

    #[tokio::test]
    async fn test_ask_terraform_destroy() {
        let input = kade_bash("terraform destroy");
        let r = check(&input).await;
        assert!(r.stdout.is_some());
        assert!(r.stdout.unwrap().contains("terraform"));
    }

    // === Safe commands pass ===

    #[tokio::test]
    async fn test_allow_ls() {
        let input = kade_bash("ls -la");
        let r = check(&input).await;
        assert!(r.stdout.is_none());
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_allow_app_state_sh() {
        let input = kade_bash("bash ../chorus/platform/scripts/app-state.sh status");
        let r = check(&input).await;
        assert!(r.stdout.is_none());
        assert_eq!(r.exit_code, 0);
    }

    // === Non-Bash tool bypass ===

    #[tokio::test]
    async fn test_allow_write_tool() {
        let input = HookInput {
            tool_name: Some("Write".to_string()),
            tool_input: Some(json!({"file_path": "/tmp/test"})),
            tool_response: None,
            session_id: None,
            cwd: Some("/Users/jeffbridwell/CascadeProjects/engineer".to_string()),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: None,
        };
        let r = check(&input).await;
        assert!(r.stdout.is_none());
        assert_eq!(r.exit_code, 0);
    }
}
