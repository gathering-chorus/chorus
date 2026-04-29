//! #2477 — Sync MCP client for chorus-hook-shim.
//!
//! Replaces the shim's direct HTTP call to `/api/loom/principles` with the
//! typed MCP surface defined by chorus-api at `/mcp`. Streamable HTTP
//! (POST + SSE response) over ureq — sync to match the shim's existing
//! HTTP idiom, no async creep.
//!
//! Public surface:
//!   - `init_session(base_url, role) -> McpSession` — initialize handshake,
//!     captures Mcp-Session-Id, sends notifications/initialized.
//!   - `list_tools(&session) -> Vec<Tool>` — tools/list call.
//!   - `call_tool(&session, name, args) -> Value` — tools/call invocation.
//!
//! Per-role context flows through the X-Chorus-Role header on every request.
//! Spine event `mcp.tool.invoked` emitted from call_tool with source=shim.

use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, Clone)]
pub struct McpSession {
    pub base_url: String,
    pub role: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Tool {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

/// Initialize a session with the MCP server. Returns the session token to
/// use on subsequent calls.
pub fn init_session(base_url: &str, role: &str) -> Result<McpSession, String> {
    let url = mcp_url(base_url);
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "chorus-hook-shim", "version": "1.0.0"}
        }
    });

    let resp = ureq::post(&url)
        .timeout(DEFAULT_TIMEOUT)
        .set("Content-Type", "application/json")
        .set("Accept", "application/json, text/event-stream")
        .set("X-Chorus-Role", role)
        .send_json(body);

    let resp = match resp {
        Ok(r) => r,
        Err(e) => return Err(format!("mcp initialize failed: {}", e)),
    };

    let session_id = resp
        .header("mcp-session-id")
        .or_else(|| resp.header("Mcp-Session-Id"))
        .ok_or_else(|| "mcp initialize: no session id header".to_string())?
        .to_string();

    let body_str = resp
        .into_string()
        .map_err(|e| format!("mcp initialize: read body: {}", e))?;
    // Parse initialize result to surface protocol errors early; then ack.
    let _ = parse_jsonrpc_payload(&body_str)?;

    let session = McpSession {
        base_url: base_url.to_string(),
        role: role.to_string(),
        session_id,
    };

    // Notifications/initialized: server expects this before tool calls.
    let ack = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
    let _ = ureq::post(&url)
        .timeout(DEFAULT_TIMEOUT)
        .set("Content-Type", "application/json")
        .set("Accept", "application/json, text/event-stream")
        .set("X-Chorus-Role", role)
        .set("Mcp-Session-Id", &session.session_id)
        .send_json(ack);

    Ok(session)
}

#[allow(dead_code)]
pub fn list_tools(session: &McpSession) -> Result<Vec<Tool>, String> {
    let payload = call_jsonrpc(session, "tools/list", json!({}), 2)?;
    let tools: Vec<Tool> = serde_json::from_value(payload["tools"].clone())
        .map_err(|e| format!("tools/list parse: {}", e))?;
    Ok(tools)
}

pub fn call_tool(session: &McpSession, name: &str, args: Value) -> Result<Value, String> {
    // Note: spine emit `mcp.tool.invoked tool=X source=shim` is the caller's
    // responsibility — keeps mcp_client free of crate::chorus_log dependency
    // so it can be exposed from lib.rs without dragging shared module in.
    call_jsonrpc(
        session,
        "tools/call",
        json!({"name": name, "arguments": args}),
        2,
    )
}

fn mcp_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    format!("{}/mcp", trimmed)
}

fn call_jsonrpc(
    session: &McpSession,
    method: &str,
    params: Value,
    id: u64,
) -> Result<Value, String> {
    let url = mcp_url(&session.base_url);
    let body = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    });

    let resp = ureq::post(&url)
        .timeout(DEFAULT_TIMEOUT)
        .set("Content-Type", "application/json")
        .set("Accept", "application/json, text/event-stream")
        .set("X-Chorus-Role", &session.role)
        .set("Mcp-Session-Id", &session.session_id)
        .send_json(body);

    let resp = match resp {
        Ok(r) => r,
        Err(e) => return Err(format!("mcp {} failed: {}", method, e)),
    };

    let body_str = resp
        .into_string()
        .map_err(|e| format!("mcp {} read body: {}", method, e))?;
    parse_jsonrpc_payload(&body_str)
}

/// Parse a Streamable HTTP response. Body is either plain JSON or SSE
/// framed `event: message\ndata: {...}`. Returns the inner `result` field
/// on success or an Err with the JSON-RPC error message on protocol error.
fn parse_jsonrpc_payload(body: &str) -> Result<Value, String> {
    let json_text = if body.contains("data:") {
        // SSE: find the data line; concat is unlikely for our payload sizes.
        body.lines()
            .find_map(|l| l.strip_prefix("data:"))
            .map(|s| s.trim().to_string())
            .ok_or_else(|| "sse: no data line".to_string())?
    } else {
        body.to_string()
    };

    let parsed: Value = serde_json::from_str(&json_text)
        .map_err(|e| format!("jsonrpc parse: {}: {}", e, json_text.chars().take(120).collect::<String>()))?;

    if let Some(err) = parsed.get("error") {
        let code = err.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
        let msg = err.get("message").and_then(|m| m.as_str()).unwrap_or("");
        return Err(format!("jsonrpc error {}: {}", code, msg));
    }

    Ok(parsed.get("result").cloned().unwrap_or(Value::Null))
}
