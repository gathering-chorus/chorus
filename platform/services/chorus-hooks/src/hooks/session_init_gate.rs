use crate::state::AppState;
use crate::types::{permission_deny_json, HookInput, HookResponse};
use std::path::Path;

const INIT_DIR: &str = "/tmp/claude-session-init";

pub async fn check(input: &HookInput, state: &AppState) -> HookResponse {
    let role = input.role();
    let role_str = role.as_str();

    if role_str == "unknown" {
        return HookResponse::allow();
    }

    let tool = input.tool_name_str();
    let pending = format!("{}/{}.pending", INIT_DIR, role_str);
    let done = format!("{}/{}.done", INIT_DIR, role_str);

    // Read tool: check if this is the session-start file
    if tool == "Read" {
        let file_path = input.get_tool_input_str("file_path");
        let expected = format!("/tmp/session-start-{}.md", role_str);
        if file_path == expected && Path::new(&pending).exists() {
            // Create done marker
            let _ = tokio::fs::create_dir_all(INIT_DIR).await;
            let _ = tokio::fs::write(&done, "").await;
            state.mark_session_init_done(role_str).await;
        }
        // Read always allowed
        return HookResponse::allow();
    }

    // Write/Edit/Bash: check gate
    if tool == "Write" || tool == "Edit" || tool == "Bash" {
        // No pending marker = no gate
        if !Path::new(&pending).exists() {
            return HookResponse::allow();
        }

        // Done marker exists or in-memory flag set
        if Path::new(&done).exists() || state.is_session_init_done(role_str).await {
            return HookResponse::allow();
        }

        // Bash exemptions
        if tool == "Bash" {
            let cmd = input.get_tool_input_str("command");
            if cmd.contains("session-start.sh")
                || cmd.contains("chorus-prompt.sh")
                || cmd.contains("werk-init.sh")
                || cmd.contains("wall-clock.sh")
                || cmd.contains("role-state.sh")
                || cmd.starts_with("TZ=")
            {
                return HookResponse::allow();
            }
        }

        // Gate active — deny
        return HookResponse::deny(&permission_deny_json(&format!(
            "Session init gate: Read /tmp/session-start-{}.md first. The framework requires you to load session context before doing work. Run session-start.sh or read the context file.",
            role_str
        )));
    }

    HookResponse::allow()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HookInput;
    use serde_json::json;

    fn make_input(tool: &str, role_dir: &str) -> HookInput {
        HookInput {
            tool_name: Some(tool.to_string()),
            tool_input: Some(json!({"command": "echo test", "file_path": "/tmp/test"})),
            tool_response: None,
            session_id: Some("test".to_string()),
            cwd: Some(format!("/Users/jeffbridwell/CascadeProjects/{}", role_dir)),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: None,
        }
    }

    #[tokio::test]
    async fn allows_read_always() {
        let state = AppState::new();
        let input = make_input("Read", "architect");
        let r = check(&input, &state).await;
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn allows_bash_when_no_pending_marker() {
        // No pending marker = no gate
        let state = AppState::new();
        let input = make_input("Bash", "architect");
        let r = check(&input, &state).await;
        assert_eq!(r.exit_code, 0);
    }
}

    #[tokio::test]
    async fn allows_unknown_role() {
        let state = AppState::new();
        let input = HookInput {
            tool_name: Some("Bash".to_string()),
            tool_input: Some(serde_json::json!({"command": "echo test"})),
            tool_response: None,
            session_id: Some("test".to_string()),
            cwd: Some("/Users/jeffbridwell/some/unknown/path".to_string()),
            prompt: None, stop_hook_active: None, hook_type: None,
            deploy_role: None,
        };
        let r = check(&input, &state).await;
        assert_eq!(r.exit_code, 0);
    }
