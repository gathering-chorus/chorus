//! #2477 — Rust MCP client for chorus-hooks shim.
//!
//! Hermetic tests for the sync MCP client that replaces the shim's direct
//! HTTP call to /api/loom/principles. Spawns a TcpListener-backed fake MCP
//! server in-process; exercises the session-init handshake, tools/list,
//! tools/call (success path), and tools/call rejection (unknown tool).

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

use chorus_hooks::mcp_client;

fn spawn_fake_mcp(handler: impl Fn(&str) -> String + Send + 'static) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = match stream { Ok(s) => s, Err(_) => continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            if reader.read_line(&mut request_line).is_err() { continue }
            let mut content_length = 0usize;
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).is_err() { break }
                if h == "\r\n" || h.is_empty() { break }
                if let Some(rest) = h.to_lowercase().strip_prefix("content-length:") {
                    if let Ok(n) = rest.trim().parse::<usize>() { content_length = n }
                }
            }
            let mut body = vec![0u8; content_length];
            use std::io::Read;
            let _ = reader.read_exact(&mut body);
            let body_str = String::from_utf8_lossy(&body).to_string();
            let response_payload = handler(&body_str);
            let session_header = if body_str.contains("\"method\":\"initialize\"") {
                "mcp-session-id: test-session-1\r\n"
            } else {
                ""
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n{}Content-Length: {}\r\n\r\n{}",
                session_header,
                response_payload.len(),
                response_payload,
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    thread::sleep(Duration::from_millis(50));
    format!("http://127.0.0.1:{}", port)
}

fn sse_data(json: &str) -> String {
    format!("event: message\ndata: {}\n\n", json)
}

#[test]
fn init_session_captures_session_id_and_acks() {
    let base = spawn_fake_mcp(move |body| {
        if body.contains("\"method\":\"initialize\"") {
            sse_data(r#"{"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}}},"jsonrpc":"2.0","id":1}"#)
        } else {
            String::from("event: message\ndata: {}\n\n")
        }
    });
    let session = mcp_client::init_session(&base, "silas").expect("init ok");
    assert_eq!(session.session_id, "test-session-1");
    assert_eq!(session.role, "silas");
}

#[test]
fn list_tools_returns_tool_names() {
    let base = spawn_fake_mcp(move |body| {
        if body.contains("\"method\":\"initialize\"") {
            sse_data(r#"{"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}}},"jsonrpc":"2.0","id":1}"#)
        } else if body.contains("\"method\":\"tools/list\"") {
            sse_data(r#"{"result":{"tools":[{"name":"chorus_principles_list","description":"d"},{"name":"chorus_principles_get","description":"d"}]},"jsonrpc":"2.0","id":2}"#)
        } else {
            sse_data(r#"{"result":{},"jsonrpc":"2.0","id":3}"#)
        }
    });
    let session = mcp_client::init_session(&base, "silas").expect("init ok");
    let tools = mcp_client::list_tools(&session).expect("list ok");
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"chorus_principles_list"));
    assert!(names.contains(&"chorus_principles_get"));
}

#[test]
fn call_tool_returns_principles_payload() {
    let base = spawn_fake_mcp(move |body| {
        if body.contains("\"method\":\"initialize\"") {
            sse_data(r#"{"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}}},"jsonrpc":"2.0","id":1}"#)
        } else if body.contains("\"method\":\"tools/call\"") {
            sse_data(r#"{"result":{"content":[{"type":"text","text":"3 principles:\n- Observe (hemenway-observe) - watch first\n- Connect (hemenway-connect) - relative location\n- Catch and store"}]},"jsonrpc":"2.0","id":2}"#)
        } else {
            sse_data(r#"{"result":{},"jsonrpc":"2.0","id":3}"#)
        }
    });
    let session = mcp_client::init_session(&base, "silas").expect("init ok");
    let result = mcp_client::call_tool(&session, "chorus_principles_list", serde_json::json!({})).expect("call ok");
    let text = result["content"][0]["text"].as_str().unwrap_or("");
    assert!(text.contains("hemenway-observe"));
    assert!(text.contains("Observe"));
}

#[test]
fn call_tool_propagates_jsonrpc_error() {
    let base = spawn_fake_mcp(move |body| {
        if body.contains("\"method\":\"initialize\"") {
            sse_data(r#"{"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}}},"jsonrpc":"2.0","id":1}"#)
        } else if body.contains("\"method\":\"tools/call\"") {
            sse_data(r#"{"error":{"code":-32601,"message":"Unknown tool: bogus"},"jsonrpc":"2.0","id":2}"#)
        } else {
            sse_data(r#"{"result":{},"jsonrpc":"2.0","id":3}"#)
        }
    });
    let session = mcp_client::init_session(&base, "silas").expect("init ok");
    let err = mcp_client::call_tool(&session, "bogus", serde_json::json!({})).expect_err("should error");
    let msg = err.to_string();
    assert!(msg.to_lowercase().contains("unknown") || msg.contains("-32601"),
        "error should propagate JSON-RPC error message; got: {}", msg);
}
