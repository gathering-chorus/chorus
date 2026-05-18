//! #2998 — MCP health gate. PreToolUse hook that refuses any
//! `mcp__chorus-api__*` tool call when chorus-mcp daemon at :3341 is
//! unreachable. Hard-error semantics: when MCP is down, every role's next
//! MCP tool call gets a loud typed refusal in their session — no silent
//! retry, no quiet timeout, no missed signal.
//!
//! Jeff 2026-05-18: "i just need to have mcp outages heard loud and
//! blocking." This is the smallest substrate change that does that.
//!
//! Behavior:
//!   - Tool name does not start with `mcp__chorus-api__` → allow.
//!   - chorus-mcp /api/chorus/health returns 2xx within 1s → allow.
//!   - Anything else (timeout, 5xx, connection refused) → block with stderr
//!     message naming chorus-mcp as down, exit-2.

use crate::types::{HookInput, HookResponse};
use std::time::Duration;

const HEALTH_URL: &str = "http://localhost:3341/api/chorus/health";
const HEALTH_TIMEOUT: Duration = Duration::from_millis(1000);
const MCP_PREFIX: &str = "mcp__chorus-api__";

pub fn check(input: &HookInput) -> HookResponse {
    let tool = input.tool_name_str();
    if !tool.starts_with(MCP_PREFIX) {
        return HookResponse::allow();
    }

    let agent = ureq::AgentBuilder::new()
        .timeout(HEALTH_TIMEOUT)
        .build();

    match agent.get(HEALTH_URL).call() {
        Ok(resp) if (200..300).contains(&resp.status()) => HookResponse::allow(),
        Ok(resp) => HookResponse::block_with_stderr(&format!(
            "HARD ERROR — chorus-mcp at :3341 returned HTTP {} on /api/chorus/health.\n\
             All `mcp__chorus-api__*` tool calls are BLOCKED until chorus-mcp recovers.\n\
             Substrate is degraded. Halt current work and surface to Jeff.\n\
             Recovery: `launchctl kickstart -k gui/$(id -u)/com.chorus.mcp`",
            resp.status()
        )),
        Err(e) => HookResponse::block_with_stderr(&format!(
            "HARD ERROR — chorus-mcp at :3341 unreachable: {}.\n\
             All `mcp__chorus-api__*` tool calls are BLOCKED until chorus-mcp recovers.\n\
             Substrate is degraded. Halt current work and surface to Jeff.\n\
             Recovery: `launchctl kickstart -k gui/$(id -u)/com.chorus.mcp`",
            e
        )),
    }
}
