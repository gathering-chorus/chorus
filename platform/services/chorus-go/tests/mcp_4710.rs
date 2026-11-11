//! #4710 AC3 — an approved card goes through the MCP door as the lander, against a stand-in MCP on
//! loopback: initialize → session → initialized → tools/call clearing-card-jeff, the lander's login
//! on every request (as a header, never argv).
use chorus_go::mcp::{self, card_args, mcp_argv, rpc_answer};
use serde_json::json;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

static ENV: Mutex<()> = Mutex::new(());

fn stand_in(tool_reply: &'static str) -> (String, Arc<Mutex<Vec<String>>>) {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/mcp", l.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let s2 = seen.clone();
    std::thread::spawn(move || {
        for mut s in l.incoming().flatten() {
            let mut got = Vec::new(); let mut buf = [0u8; 65536];
            loop {
                let n = s.read(&mut buf).unwrap_or(0); got.extend_from_slice(&buf[..n]);
                let t = String::from_utf8_lossy(&got).to_string();
                if let Some((h, b)) = t.split_once("\r\n\r\n") {
                    let len = h.lines().find_map(|l| l.to_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0))).unwrap_or(0);
                    if b.len() >= len { break; }
                }
                if n == 0 { break; }
            }
            let t = String::from_utf8_lossy(&got).to_string();
            s2.lock().unwrap().push(t.clone());
            let (extra, body) = if t.contains("\"initialize\"") { ("Mcp-Session-Id: sess-4710\r\n", r#"{"jsonrpc":"2.0","id":1,"result":{}}"#.to_string()) }
                else if t.contains("notifications/initialized") { ("", String::new()) }
                else { ("", format!("data: {tool_reply}\n\n")) };
            let _ = write!(s, "HTTP/1.1 200 OK\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        }
    });
    (url, seen)
}

#[test]
fn an_approved_argv_becomes_the_card_tools_arguments() {
    let argv: Vec<String> = ["add", "Fix X", "--owner", "silas", "--priority", "P2", "--desc", "## Experience\nx"].iter().map(|s| s.to_string()).collect();
    assert_eq!(card_args(&argv).unwrap(), json!({"title": "Fix X", "owner": "silas", "priority": "P2", "desc": "## Experience\nx"}));
    assert!(card_args(&["rm".to_string(), "x".to_string()]).is_err());
}

#[test]
fn the_lander_files_through_the_mcp_door_with_its_login_on_every_request() {
    let _g = ENV.lock().unwrap_or_else(|p| p.into_inner());
    let (url, seen) = stand_in(r#"{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"Added #4999: Fix X"}]}}"#);
    std::env::set_var("CHORUS_GO_MCP", &url);
    let r = mcp::call_tool("LANDER-LOGIN", "clearing-card-jeff", json!({"title": "Fix X"}));
    std::env::remove_var("CHORUS_GO_MCP");
    assert_eq!(r, Ok("Added #4999: Fix X".to_string()));
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 3, "initialize, initialized, tools/call");
    for req in seen.iter() { assert!(req.contains("Authorization: Bearer LANDER-LOGIN"), "a request went without the lander's login"); }
    assert!(seen[1].contains("Mcp-Session-Id: sess-4710") && seen[2].contains("Mcp-Session-Id: sess-4710"));
    assert!(seen[2].contains("\"name\":\"clearing-card-jeff\""));
    // what ps sees: no login on argv
    let argv = mcp_argv(&url, std::path::Path::new("/tmp/x"), Some("sess-4710")).join(" ");
    assert!(!argv.contains("LANDER-LOGIN") && !argv.to_lowercase().contains("authorization"), "{argv}");
}

// NEGATIVE PROOF: a tool that refuses (isError) or an RPC error is an error, never "Filed".
#[test]
fn a_refused_card_is_an_error() {
    let _g = ENV.lock().unwrap_or_else(|p| p.into_inner());
    let (url, _) = stand_in(r#"{"jsonrpc":"2.0","id":2,"result":{"isError":true,"content":[{"type":"text","text":"caller not verified"}]}}"#);
    std::env::set_var("CHORUS_GO_MCP", &url);
    let r = mcp::call_tool("L", "clearing-card-jeff", json!({}));
    std::env::remove_var("CHORUS_GO_MCP");
    assert!(r.unwrap_err().contains("caller not verified"));
    assert!(rpc_answer(r#"{"jsonrpc":"2.0","id":2,"error":{"message":"no such tool"}}"#).unwrap().get("error").is_some());
}
