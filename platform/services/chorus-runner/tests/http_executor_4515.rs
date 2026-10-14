// @test-type: unit
// @domain: services
//! #4515 — a Skill row can name an API route as what it runs (executor
//! "http"), so the old MCP server's API tools become model rows with no new
//! program each. The runner calls the route as the caller, only on loopback.
use chorus_runner::server::{prepare_with, run_http};
use chorus_runner::skills::{http_target, skill_tools};
use chorus_runner::new_trace_id;
use serde_json::{json, Map, Value};
use std::io::{Read, Write};

fn inputs() -> Map<String, Value> {
    json!({ "NudgeInput": { "type": "object", "properties": { "to": { "type": "string" }, "message": { "type": "string" } }, "required": ["to", "message"] } })
        .as_object().unwrap().clone()
}

fn row(implemented_by: &str) -> Value {
    json!({ "name": "chorus_nudge_message", "executor": "http", "inputShape": "NudgeInput",
            "implementedBy": implemented_by, "ownedBy": "principal-wren", "runsAs": "caller" })
}

#[test]
fn an_http_row_is_a_tool_and_names_its_route() {
    let (tools, un) = skill_tools(&[row("POST http://127.0.0.1:3475/api/messages")], &inputs());
    assert!(un.is_empty(), "{un:?}");
    assert!(tools[0].http);
    let (m, u) = http_target("GET http://127.0.0.1:3340/api/cards/{card}", json!({ "card": "4515 x" }).as_object().unwrap()).unwrap();
    assert_eq!((m.as_str(), u.as_str()), ("GET", "http://127.0.0.1:3340/api/cards/4515%20x"), "a value is encoded, never spliced raw");
}

#[test]
fn negative_proof_a_route_off_the_machine_is_never_offered() {
    let (tools, un) = skill_tools(&[row("POST https://example.com/hook")], &inputs());
    assert!(tools.is_empty(), "a caller's credential could leave the machine");
    assert!(un[0].reason.contains("127.0.0.1"), "{un:?}");
}

#[test]
fn a_caller_run_http_tool_forwards_the_callers_own_credential() {
    let (tools, _) = skill_tools(&[row("POST http://127.0.0.1:9/api/messages")], &inputs());
    let args = json!({ "to": "silas", "message": "hi" }).as_object().unwrap().clone();
    let p = prepare_with(&tools[0], &args, "wren", "00-t", Some("the-callers-token")).unwrap();
    let (method, url, body, token) = p.http.unwrap();
    assert_eq!((method.as_str(), url.as_str()), ("POST", "http://127.0.0.1:9/api/messages"));
    assert_eq!(token, "the-callers-token", "the forwarded credential is the caller's, not a role header");
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), json!({ "to": "silas", "message": "hi" }));
}

#[test]
fn the_call_sends_the_bearer_and_body_and_reads_the_status() {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let h = std::thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut buf = vec![0u8; 8192];
        let n = s.read(&mut buf).unwrap();
        let req = String::from_utf8_lossy(&buf[..n]).to_string();
        s.write_all(b"HTTP/1.1 201 Created\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok").unwrap();
        req
    });
    let (ok, text, code) = run_http("POST", &format!("http://127.0.0.1:{port}/api/x"), r#"{"a":1}"#, "tok123", &new_trace_id(), 5);
    let req = h.join().unwrap();
    assert!(ok && code == 201, "{text}");
    assert!(req.contains("Authorization: Bearer tok123"), "{req}");
    assert!(req.contains("traceparent:"), "{req}");
    // NEGATIVE PROOF: a refusal is not success
    let l2 = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let p2 = l2.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let (mut s, _) = l2.accept().unwrap();
        let mut b = [0u8; 4096];
        let _ = s.read(&mut b);
        s.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
    });
    let (ok2, _, code2) = run_http("POST", &format!("http://127.0.0.1:{p2}/api/x"), "{}", "t", &new_trace_id(), 5);
    assert!(!ok2 && code2 == 403);
}
