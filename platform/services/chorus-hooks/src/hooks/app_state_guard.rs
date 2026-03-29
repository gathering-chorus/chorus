use crate::types::{permission_deny_json, HookInput, HookResponse};
use regex::Regex;
use std::sync::LazyLock;

static DOCKER_COMPOSE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"docker\s+compose\s+(up|down|stop|restart|rm|kill|create|build)").unwrap()
});

static DOCKER_COMPOSE_FLAGS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"docker\s+compose\s+--\S+").unwrap()
});

static LSOF_KILL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"lsof.*\|\s*(xargs\s+)?kill").unwrap()
});

static PIPED_KILL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\|\s*(xargs\s+)?(kill|pkill|killall)").unwrap()
});

static KILL9_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"kill\s+-9").unwrap()
});

static TERRAFORM_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"terraform\s+(apply|destroy)").unwrap()
});

pub fn check(input: &HookInput) -> HookResponse {
    if input.tool_name_str() != "Bash" {
        return HookResponse::allow();
    }

    let cmd = input.get_tool_input_str("command");
    if cmd.is_empty() {
        return HookResponse::allow();
    }

    let first_line = cmd.lines().next().unwrap_or("");

    // Skip heredocs and echo/printf
    if cmd.contains("<<") {
        return HookResponse::allow();
    }
    let fl_trimmed = first_line.trim_start();
    if fl_trimmed.starts_with("echo ")
        || fl_trimmed.starts_with("printf ")
        || fl_trimmed.starts_with("cat >")
        || fl_trimmed.starts_with("cat <<")
    {
        return HookResponse::allow();
    }

    // Allow service-lifecycle.sh
    if cmd.contains("service-lifecycle.sh") {
        return HookResponse::allow();
    }

    if DOCKER_COMPOSE_RE.is_match(&cmd) {
        return HookResponse::deny(&permission_deny_json(
            "BLOCKED: Use app-state.sh for container lifecycle. Run: bash ../jeff-bridwell-personal-site/app-state.sh [start|stop|restart|deploy|status]"
        ));
    }

    if DOCKER_COMPOSE_FLAGS_RE.is_match(&cmd) {
        return HookResponse::deny(&permission_deny_json(
            "BLOCKED: Use app-state.sh for container lifecycle. No direct docker compose flags."
        ));
    }

    if LSOF_KILL_RE.is_match(&cmd) {
        return HookResponse::deny(&permission_deny_json(
            "BLOCKED: No lsof-based process killing. Use app-state.sh for service lifecycle."
        ));
    }

    if PIPED_KILL_RE.is_match(&cmd) {
        return HookResponse::deny(&permission_deny_json(
            "BLOCKED: No piped process killing. Use app-state.sh for service lifecycle."
        ));
    }

    if KILL9_RE.is_match(&cmd) {
        return HookResponse::deny(&permission_deny_json(
            "BLOCKED: No kill -9. Use app-state.sh for service lifecycle."
        ));
    }

    if TERRAFORM_RE.is_match(&cmd) {
        return HookResponse::deny(&permission_deny_json(
            "BLOCKED: No direct terraform apply/destroy. Use app-state.sh deploy for infrastructure changes."
        ));
    }

    HookResponse::allow()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn bash_input(cmd: &str) -> HookInput {
        serde_json::from_value(json!({
            "tool_name": "Bash",
            "tool_input": {"command": cmd},
            "cwd": "/Users/jeffbridwell/CascadeProjects/architect"
        })).unwrap()
    }

    #[test]
    fn allows_normal_bash() {
        let r = check(&bash_input("ls -la"));
        assert!(r.stdout.is_none());
        assert_eq!(r.exit_code, 0);
    }

    #[test]
    fn blocks_docker_compose_up() {
        let r = check(&bash_input("docker compose up -d"));
        assert!(r.stdout.is_some());
        assert!(r.stdout.unwrap().contains("deny"));
    }

    #[test]
    fn blocks_kill_9() {
        let r = check(&bash_input("kill -9 12345"));
        assert!(r.stdout.is_some());
    }

    #[test]
    fn blocks_piped_kill() {
        let r = check(&bash_input("lsof -i :3000 | xargs kill"));
        assert!(r.stdout.is_some());
    }

    #[test]
    fn allows_app_state() {
        let r = check(&bash_input("bash app-state.sh deploy"));
        assert!(r.stdout.is_none());
    }

    #[test]
    fn allows_heredoc() {
        let r = check(&bash_input("cat <<EOF\ndocker compose up\nEOF"));
        assert!(r.stdout.is_none());
    }

    #[test]
    fn ignores_non_bash() {
        let input: HookInput = serde_json::from_value(json!({
            "tool_name": "Read",
            "tool_input": {"file_path": "/tmp/test"},
            "cwd": "/tmp"
        })).unwrap();
        assert_eq!(check(&input).exit_code, 0);
    }
}
