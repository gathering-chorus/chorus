use crate::shared::state_paths::chorus_root;
use crate::state::chorus_log;
use crate::types::{permission_ask_json, permission_deny_json, HookInput, HookResponse};
use regex::Regex;
use std::path::Path;

fn repo_root() -> &'static str { chorus_root() }

fn manifest_paths() -> Vec<String> {
    let root = chorus_root();
    vec![
        format!("{}/roles/wren/.sensitive-paths", root),
        format!("{}/roles/silas/.sensitive-paths", root),
        format!("{}/platform/roles/kade/.sensitive-paths", root),
    ]
}

pub async fn check(input: &HookInput) -> HookResponse {
    let tool = input.tool_name_str();
    if tool != "Read" && tool != "Write" && tool != "Edit" {
        return HookResponse::allow();
    }

    let file_path = input.get_tool_input_str("file_path");
    if file_path.is_empty() {
        return HookResponse::allow();
    }

    // Common patterns — always Private (block read AND write)
    if file_path.ends_with("/.env")
        || file_path.ends_with(".env")
        || file_path.contains("/.env.")
        || file_path.contains("/terraform.tfstate")
        || file_path.contains("/.ssh/")
        || file_path.ends_with("/credentials.json")
    {
        log_access("deny", "private", &file_path).await;
        let action = if tool == "Read" { "read" } else { "write to" };
        let reason = if file_path.contains(".env") {
            format!("BLOCKED: Cannot {} .env files — they contain credentials.", action)
        } else if file_path.contains("terraform.tfstate") {
            format!("BLOCKED: Cannot {} Terraform state — contains infrastructure secrets.", action)
        } else if file_path.contains("credentials") {
            format!("BLOCKED: Cannot {} credentials files.", action)
        } else {
            format!("BLOCKED: Cannot {} SSH key files.", action)
        };
        return HookResponse::deny(&permission_deny_json(&reason));
    }

    // Check manifests
    for manifest_path in &manifest_paths() {
        if !Path::new(manifest_path.as_str()).exists() {
            continue;
        }

        let content = match std::fs::read_to_string(manifest_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let mut current_tier = "";
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            if trimmed.starts_with("private:") {
                current_tier = "private";
                continue;
            } else if trimmed.starts_with("internal:") {
                current_tier = "internal";
                continue;
            }

            // Parse entry (strip "  - " prefix and quotes)
            let entry = trimmed
                .trim_start_matches("- ")
                .trim_start_matches("  - ")
                .trim_matches('"');

            if entry.is_empty() || entry == "[]" {
                continue;
            }

            if path_matches(&file_path, entry) {
                if current_tier == "private" {
                    log_access("deny", "private", &file_path).await;
                    return HookResponse::deny(&permission_deny_json(
                        "BLOCKED: File classified as PRIVATE. This file contains personal/sensitive data that must never be sent to external APIs. See data-classification-policy.md."
                    ));
                } else if current_tier == "internal" {
                    log_access("ask", "internal", &file_path).await;
                    return HookResponse::deny(&permission_ask_json(
                        "File classified as INTERNAL (infrastructure/operational data). Reading this file will send its contents to Anthropic's API. Confirm this is intentional. See data-classification-policy.md."
                    ));
                }
            }
        }
    }

    HookResponse::allow()
}

fn path_matches(path: &str, pattern: &str) -> bool {
    let full_pattern = if pattern.starts_with('/') {
        pattern.to_string()
    } else {
        format!("{}/{}", repo_root(), pattern)
    };

    // Convert glob to regex
    let regex_str = full_pattern
        .replace("**", "\x00DOUBLESTAR\x00")
        .replace('*', "[^/]+")
        .replace("\x00DOUBLESTAR\x00", ".+");

    let regex_str = format!("^{}$", regex_str);
    Regex::new(&regex_str)
        .map(|re| re.is_match(path))
        .unwrap_or(false)
}

async fn log_access(decision: &str, tier: &str, path: &str) {
    chorus_log(
        "guard.classify.decided",
        "system",
        &[("decision", decision), ("tier", tier), ("path", path)],
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HookInput;
    use serde_json::json;

    fn make_read(path: &str) -> HookInput {
        HookInput {
            tool_name: Some("Read".to_string()),
            tool_input: Some(json!({"file_path": path})),
            tool_response: None,
            session_id: Some("test".to_string()),
            cwd: Some(format!("{}/architect", chorus_root())),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("silas".to_string()),
        }
    }

    #[tokio::test]
    async fn allows_non_read_tools() {
        let input = HookInput {
            tool_name: Some("Bash".to_string()),
            tool_input: Some(json!({"command": "ls"})),
            tool_response: None,
            session_id: Some("test".to_string()),
            cwd: Some(format!("{}/architect", chorus_root())),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("silas".to_string()),
        };
        let r = check(&input).await;
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn allows_normal_file_read() {
        let input = make_read(&format!("{}/platform/scripts/cards", chorus_root()));
        let r = check(&input).await;
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn handles_env_file_read() {
        let input = make_read(&format!("{}/jeff-bridwell-personal-site/.env", chorus_root()));
        let r = check(&input).await;
        assert!(r.exit_code == 0 || r.stdout.is_some());
    }
}
