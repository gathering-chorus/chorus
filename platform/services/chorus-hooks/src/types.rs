use serde::{Deserialize, Serialize};
use crate::shared::state_paths::chorus_root;

/// Role detection from working directory
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Wren,
    Silas,
    Kade,
    Unknown,
}

impl Role {
    pub fn from_cwd(cwd: &str) -> Self {
        if cwd.contains("product-manager") || cwd.contains("roles/wren") {
            Role::Wren
        } else if cwd.contains("architect") || cwd.contains("roles/silas") {
            Role::Silas
        } else if cwd.contains("engineer") || cwd.contains("roles/kade") {
            Role::Kade
        } else {
            Role::Unknown
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Wren => "wren",
            Role::Silas => "silas",
            Role::Kade => "kade",
            Role::Unknown => "unknown",
        }
    }
}

/// Input from Claude Code hooks — union of all possible fields
#[derive(Debug, Clone, Deserialize)]
pub struct HookInput {
    pub tool_name: Option<String>,
    pub tool_input: Option<serde_json::Value>,
    pub tool_response: Option<serde_json::Value>,
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    pub prompt: Option<String>,
    pub stop_hook_active: Option<bool>,
    #[serde(rename = "type")]
    pub hook_type: Option<String>,
    /// Injected by shim from DEPLOY_ROLE env var (#1714)
    pub deploy_role: Option<String>,
}

impl HookInput {
    pub fn role(&self) -> Role {
        // Check deploy_role field injected by shim (#1714) — CWD detection fails from app dir
        if let Some(ref dr) = self.deploy_role {
            match dr.as_str() {
                "silas" => return Role::Silas,
                "wren" => return Role::Wren,
                "kade" => return Role::Kade,
                _ => {}
            }
        }
        Role::from_cwd(self.cwd.as_deref().unwrap_or(""))
    }

    pub fn tool_name_str(&self) -> &str {
        self.tool_name.as_deref().unwrap_or("")
    }

    pub fn get_tool_input_str(&self, key: &str) -> String {
        self.tool_input
            .as_ref()
            .and_then(|v| v.get(key))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    }

    /// Get tool_response as a string — handles both string and object values
    pub fn tool_response_str(&self) -> String {
        match &self.tool_response {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
            None => String::new(),
        }
    }
}

/// Response back to Claude Code
#[derive(Debug, Serialize)]
pub struct HookResponse {
    /// What to write to stdout (JSON or empty)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    /// What to write to stderr (feedback messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    /// Exit code (0 = allow, 2 = block)
    pub exit_code: i32,
}

impl HookResponse {
    pub fn allow() -> Self {
        Self {
            stdout: None,
            stderr: None,
            exit_code: 0,
        }
    }

    pub fn allow_with_message(msg: &str) -> Self {
        Self {
            stdout: Some(msg.to_string()),
            stderr: None,
            exit_code: 0,
        }
    }

    pub fn deny(stdout_json: &str) -> Self {
        Self {
            stdout: Some(stdout_json.to_string()),
            stderr: None,
            exit_code: 0,
        }
    }

    pub fn block_with_stderr(msg: &str) -> Self {
        Self {
            stdout: None,
            stderr: Some(msg.to_string()),
            exit_code: 2,
        }
    }

    pub fn warn_stderr(msg: &str) -> Self {
        Self {
            stdout: None,
            stderr: Some(msg.to_string()),
            exit_code: 0,
        }
    }
}

/// Permission decision JSON that Claude Code expects
pub fn permission_deny_json(reason: &str) -> String {
    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": reason
        }
    })
    .to_string()
}

pub fn permission_ask_json(reason: &str) -> String {
    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "ask",
            "permissionDecisionReason": reason
        }
    })
    .to_string()
}

pub fn decision_allow_json(message: &str) -> String {
    serde_json::json!({
        "decision": "allow",
        "message": message
    })
    .to_string()
}


/// Card type from board labels — replaces is_defect_fix() keyword matching (#1909).
/// Reads card ID from role-state JSON, then checks board labels for type: prefix.
/// Returns "fix", "new", "enhance", "chore", "swat", or "unknown".
pub fn card_type_for_role(role: &str) -> String {
    let state_path = format!("/tmp/claude-team-scan/{}-declared.json", role);
    let card_id = match std::fs::read_to_string(&state_path) {
        Ok(content) => {
            match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(parsed) => {
                    if parsed.get("state").and_then(|s| s.as_str()) != Some("building") {
                        return "unknown".to_string();
                    }
                    parsed.get("card")
                        .and_then(|c| c.as_u64())
                        .map(|n| n.to_string())
                        .unwrap_or_default()
                }
                Err(_) => return "unknown".to_string(),
            }
        }
        Err(_) => return "unknown".to_string(),
    };

    if card_id.is_empty() {
        return "unknown".to_string();
    }

    // Check state file for card_type (written by /pull via role-state.sh)
    if let Ok(content) = std::fs::read_to_string(&state_path) {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(ct) = parsed.get("card_type").and_then(|v| v.as_str()) {
                if !ct.is_empty() && ct != "unknown" {
                    return ct.to_string();
                }
            }
        }
    }

    // Fallback: query board for card labels via cards CLI
    let output = std::process::Command::new("bash")
        .args(["-lc", &format!("{}/platform/scripts/cards view {}", chorus_root(), card_id)])
        .output();

    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if line.contains("type:") {
                if let Some(t) = line.split("type:").nth(1) {
                    let card_type = t.split(|c: char| !c.is_alphanumeric()).next().unwrap_or("unknown");
                    return card_type.to_string();
                }
            }
        }
    }

    "unknown".to_string()
}

/// Check if ANY building role is working a fix card.
pub fn is_fix_card() -> bool {
    for role in &["kade", "silas", "wren"] {
        if card_type_for_role(role) == "fix" {
            return true;
        }
    }
    false
}
