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
    /// #2625: injected by shim from CHORUS_WORKTREE_OVERRIDE env. Env vars
    /// don't cross the unix-socket boundary to the daemon; shim writes them
    /// into the JSON for hooks to read.
    #[serde(default)]
    pub chorus_worktree_override: Option<bool>,
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


/// Card type from board for a role's WIP card (#1909, #2467).
///
/// #2467 (2026-04-30): role-state no longer carries card/card_type. Card
/// belongs to the board. This function makes two HTTP calls to chorus-api:
///   1. GET /api/chorus/context/board/wip?role=<Role>  → list of WIP cards
///   2. GET /api/athena/card/<id>                       → parses `domains`
///      array for `type:fix` / `type:new` / etc.
///
/// Returns "fix", "new", "enhance", "chore", "swat", or "unknown".
///
/// Hot-path: tdd_gate / test_quality_gate call this on every relevant edit.
/// 1-second timeout per call; fails open to "unknown" (conservative gating)
/// rather than block. No cache today — chorus-api is local + fast (~50ms).
/// Returns "unknown" when role has 0 or >1 WIP cards (ambiguous → conservative).
pub fn card_type_for_role(role: &str) -> String {
    // chorus-api expects role with first letter capitalized: Silas/Kade/Wren
    let role_cap = {
        let mut chars = role.chars();
        match chars.next() {
            Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            None => return "unknown".to_string(),
        }
    };
    let wip_url = format!(
        "http://localhost:3340/api/chorus/context/board/wip?role={}",
        role_cap
    );
    let wip_out = std::process::Command::new("curl")
        .args(["-s", "--max-time", "1", &wip_url])
        .output();
    let Ok(out) = wip_out else { return "unknown".to_string(); };
    let body = String::from_utf8_lossy(&out.stdout);
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&body) else {
        return "unknown".to_string();
    };

    let cards = parsed.pointer("/data/cards")
        .and_then(|v| v.as_array());
    let Some(cards) = cards else { return "unknown".to_string(); };

    // Only confident when role has exactly one WIP card.
    if cards.len() != 1 {
        return "unknown".to_string();
    }
    let card_id = match cards[0].get("id").and_then(|v| v.as_u64()) {
        Some(id) => id,
        None => return "unknown".to_string(),
    };

    // Fetch full card detail (Athena surfaces the labels-as-domains array)
    let card_url = format!("http://localhost:3340/api/athena/card/{}", card_id);
    let card_out = std::process::Command::new("curl")
        .args(["-s", "--max-time", "1", &card_url])
        .output();
    let Ok(out) = card_out else { return "unknown".to_string(); };
    let body = String::from_utf8_lossy(&out.stdout);
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&body) else {
        return "unknown".to_string();
    };

    // domains is a flat array of label strings: ["chunk:ops","type:fix",...]
    let Some(domains) = parsed.pointer("/data/domains").and_then(|v| v.as_array()) else {
        return "unknown".to_string();
    };
    for label in domains {
        let Some(s) = label.as_str() else { continue; };
        if let Some(stripped) = s.strip_prefix("type:") {
            let kind = stripped.split(|c: char| !c.is_alphanumeric())
                .next()
                .unwrap_or("");
            if !kind.is_empty() {
                return kind.to_string();
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
