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
#[derive(Debug, Clone, Default, Deserialize)]
pub struct HookInput {
    pub tool_name: Option<String>,
    pub tool_input: Option<serde_json::Value>,
    /// #3334 — Claude Code sends this field as `tool_output` on failure events
    /// (PostToolUseFailure) and historically as `tool_response`. One field, both
    /// names, so a failure payload deserializes identically.
    #[serde(alias = "tool_output")]
    pub tool_response: Option<serde_json::Value>,
    /// #3334 — top-level failure flag on PostToolUseFailure payloads. The most
    /// reliable error signal: present even when no numeric exit code exists
    /// (e.g. killed by signal).
    #[serde(default)]
    pub tool_output_is_error: Option<bool>,
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
    /// #3252: the SHARED werk trace_id. Shim injects this from CHORUS_TRACE_ID
    /// (the demo/build/deploy lifecycle trace minted in #2897) — same socket-
    /// crossing pattern as `deploy_role` / `chorus_worktree_override`, because
    /// env vars don't reach the daemon. NOT a hook-only id: carrying the
    /// existing werk trace means `grep <trace_id> chorus.log` returns the
    /// demo/build/deploy verbs (#3023/#3270) AND every hook.decision as ONE
    /// correlated flow. None on ad-hoc tool calls outside a werk run.
    #[serde(default)]
    pub trace_id: Option<String>,
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

    /// #3252: resolve the trace_id for this tool call. Returns the shim-
    /// propagated shared werk trace when present (so hook.decision events join
    /// the same flow as demo/build/deploy verbs), otherwise mints a fresh
    /// time-ordered uuid v7 so the field is never empty. The minted fallback
    /// is per-dispatch: a Pre and a Post for the SAME ad-hoc tool call get
    /// different ids (Claude Code gives no per-call key), so cross-dispatch
    /// correlation outside a werk run relies on session_id, not trace_id.
    pub fn trace_id_or_mint(&self) -> String {
        match self.trace_id.as_deref() {
            Some(t) if !t.is_empty() => t.to_string(),
            _ => uuid::Uuid::now_v7().to_string(),
        }
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

/// Detect the caller's role from env for spine attribution. Prefers
/// `CHORUS_ROLE` (set by shim.rs:124 inside hook execution), then
/// `DEPLOY_ROLE` (set by /role-state, cards CLI, agent context). Falls back
/// to `"unknown"` when neither is set — never panic, never hardcode (#2899).
pub(crate) fn caller_role_for_event() -> String {
    std::env::var("CHORUS_ROLE")
        .ok()
        .or_else(|| std::env::var("DEPLOY_ROLE").ok())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Check if ANY building role is working a fix card.
///
/// Test/smoke override: `CHORUS_TEST_FORCE_FIX_CARD=1` forces true,
/// `CHORUS_TEST_FORCE_FIX_CARD=0` forces false. Used by gate smoke checks
/// so the smoke is deterministic (does not depend on live board WIP).
/// Name is intentionally `CHORUS_TEST_*`-scoped to mark intent and discourage
/// shell-discoverable use as a runtime gate-bypass (silas review #2644).
/// Every override-fire emits a `gate.test_override.checked` spine event for
/// audit (ADR-028 hook-bypass discipline; same family as CHORUS_MCP_BYPASS).
/// Event is named for what actually happened — the test override env var was
/// checked — not "gate bypassed in production." Role attribution comes from
/// `caller_role_for_event()`, never hardcoded (#2899).
pub fn is_fix_card() -> bool {
    if let Ok(v) = std::env::var("CHORUS_TEST_FORCE_FIX_CARD") {
        let forced = v == "1" || v.eq_ignore_ascii_case("true");
        let role = caller_role_for_event();
        // Best-effort spine emit; failures must not affect gate behavior.
        let _ = std::process::Command::new("/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log")
            .args(["gate.test_override.checked", &role,
                   &format!("forced={} value={}", forced, v)])
            .output();
        return forced;
    }
    for role in &["kade", "silas", "wren"] {
        if card_type_for_role(role) == "fix" {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod trace_id_tests {
    use super::*;

    #[test]
    fn propagated_shared_trace_is_returned_verbatim() {
        // #3252: when the shim propagates CHORUS_TRACE_ID, the hook.decision
        // event MUST carry that exact id so it joins the demo/build/deploy flow.
        let input = HookInput {
            trace_id: Some("demo-3252-abc123".to_string()),
            ..Default::default()
        };
        assert_eq!(input.trace_id_or_mint(), "demo-3252-abc123");
    }

    #[test]
    fn absent_trace_mints_a_nonempty_id() {
        // Ad-hoc tool call outside a werk run: field must never be empty.
        let input = HookInput::default();
        assert!(!input.trace_id_or_mint().is_empty());
    }

    #[test]
    fn empty_trace_string_is_treated_as_absent_and_minted() {
        // A propagated-but-empty CHORUS_TRACE_ID must not become an empty join key.
        let input = HookInput {
            trace_id: Some(String::new()),
            ..Default::default()
        };
        assert!(!input.trace_id_or_mint().is_empty());
    }

    #[test]
    fn minted_ids_are_unique_per_dispatch() {
        let a = HookInput::default().trace_id_or_mint();
        let b = HookInput::default().trace_id_or_mint();
        assert_ne!(a, b, "minted fallback ids must be unique per dispatch");
    }
}
