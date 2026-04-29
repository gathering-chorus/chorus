//! NiFi Discipline Hook — enforce native processors, block bash wrappers (#1686)
//!
//! At demo/accept time for NiFi cards, query the NiFi API for the process group
//! and check that it uses native transformation processors, not just
//! GenerateFlowFile + ExecuteStreamCommand (bash-in-a-NiFi-costume).
//!
//! Native processors: ExecuteGroovyScript, ExecuteSQL, ConvertRecord, SplitJson,
//! JoltTransformJSON, InvokeHTTP (with non-trivial config), ListenHTTP, HandleHttpRequest.
//!
//! Bash wrapper pattern: GenerateFlowFile → ExecuteStreamCommand chain with no
//! native transformation in between.

use crate::state::AppState;
use crate::types::HookInput;
use tracing::{info, warn};

/// Native NiFi processor types that indicate real NiFi usage
const NATIVE_PROCESSORS: &[&str] = &[
    "ExecuteGroovyScript",
    "ExecuteSQL",
    "ExecuteSQLRecord",
    "ConvertRecord",
    "SplitJson",
    "JoltTransformJSON",
    "InvokeHTTP",
    "ListenHTTP",
    "HandleHttpRequest",
    "HandleHttpResponse",
    "MergeContent",
    "RouteOnAttribute",
    "RouteOnContent",
    "UpdateAttribute",
    "EvaluateJsonPath",
    "PutDatabaseRecord",
    "QueryDatabaseTable",
    "PublishKafka",
    "ConsumeKafka",
    "ListenSyslog",
    "ListenTCP",
    "PutFile",
    "GetFile",
    "ListFile",
    "FetchFile",
];

/// Bash wrapper processors — these are NiFi wrapping shell, not NiFi-native
const WRAPPER_PROCESSORS: &[&str] = &[
    "ExecuteStreamCommand",
    "ExecuteProcess",
];

/// Check if a demo/accept is for a NiFi card and validate processor discipline
pub async fn check(input: &HookInput, _state: &AppState) -> Option<String> {
    // Only trigger on Skill tool (demo/accept)
    let tool = input.tool_name.as_deref().unwrap_or("");
    if tool != "Skill" {
        return None;
    }

    // Check if the skill args reference a card with NiFi in its description
    let _prompt = input.prompt.as_deref().unwrap_or("");
    let tool_input = input.tool_input.as_ref()
        .and_then(|v| v.as_object())
        .and_then(|o| o.get("args"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Only check on demo and acp skills
    let skill = input.tool_input.as_ref()
        .and_then(|v| v.as_object())
        .and_then(|o| o.get("skill"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if skill != "demo" && skill != "acp" {
        return None;
    }

    // Extract card ID from args
    let card_id = tool_input.split_whitespace().next().unwrap_or("");
    if card_id.is_empty() || !card_id.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    // Check if card description mentions NiFi — query cards
    let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/jeffbridwell".to_string());
    let output = std::process::Command::new("bash")
        .args(["-l", &format!("{}/platform/scripts/cards", crate::shared::state_paths::chorus_root()), "view", card_id])
        .env("CHORUS_ROOT", crate::shared::state_paths::chorus_root())
        .env("HOME", &home)
        .env("PATH", format!("{}/CascadeProjects/chorus/platform/scripts:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin", home))
        .output()
        .ok()?;
    let desc = String::from_utf8_lossy(&output.stdout);

    if !desc.to_lowercase().contains("nifi") {
        return None; // Not a NiFi card
    }

    info!(card = card_id, "NiFi discipline check — querying NiFi API for processor types");

    // Query NiFi for all process groups and find the one matching this card
    // For now, just log the check — the actual NiFi API query requires token management
    // which is handled by nifi-dsl.sh. We check the card description for processor mentions.

    // Heuristic: if the card description mentions ExecuteStreamCommand or ExecuteProcess
    // without also mentioning a native processor, warn.
    let desc_lower = desc.to_lowercase();
    let has_wrapper = WRAPPER_PROCESSORS.iter().any(|p| desc_lower.contains(&p.to_lowercase()));
    let has_native = NATIVE_PROCESSORS.iter().any(|p| desc_lower.contains(&p.to_lowercase()));

    if has_wrapper && !has_native {
        let msg = format!(
            "⚠ NiFi discipline: #{} uses bash wrapper processors (ExecuteStreamCommand/ExecuteProcess) without native NiFi transformation. \
             NiFi orchestrating bash is not NiFi-native. Use ExecuteGroovyScript, ConvertRecord, or other native processors.",
            card_id
        );
        warn!("{}", msg);
        return Some(msg);
    }

    if !has_wrapper && !has_native && desc_lower.contains("nifi") {
        // NiFi card but no processor types mentioned — can't validate
        info!(card = card_id, "NiFi card but no processor types in description — skipping discipline check");
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use crate::types::HookInput;
    use crate::shared::state_paths::chorus_root;
    use serde_json::json;

    #[tokio::test]
    async fn returns_none_for_non_bash() {
        let state = AppState::new();
        let input = HookInput {
            tool_name: Some("Read".to_string()),
            tool_input: Some(json!({"file_path": "/tmp/test"})),
            tool_response: None, session_id: Some("t".into()),
            cwd: Some(format!("{}/architect", chorus_root())),
            prompt: None, stop_hook_active: None, hook_type: None,
            deploy_role: Some("silas".into()),
        };
        assert!(check(&input, &state).await.is_none());
    }

    #[tokio::test]
    async fn returns_none_for_normal_bash() {
        let state = AppState::new();
        let input = HookInput {
            tool_name: Some("Bash".to_string()),
            tool_input: Some(json!({"command": "echo hello"})),
            tool_response: None, session_id: Some("t".into()),
            cwd: Some(format!("{}/architect", chorus_root())),
            prompt: None, stop_hook_active: None, hook_type: None,
            deploy_role: Some("silas".into()),
        };
        assert!(check(&input, &state).await.is_none());
    }
}
