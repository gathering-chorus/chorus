// @test-type: unit
// @domain: services
//! #4515 — a Skill row can name an API route as what it runs (executor
//! "http"), so the old MCP server's API tools become model rows with no new
//! program each. The runner calls the route as the caller, only on its own
//! Chorus services ({api}, {door}, {pulse} from its settings).
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
    let (tools, un) = skill_tools(&[row("POST {pulse}/api/messages")], &inputs());
    assert!(un.is_empty(), "{un:?}");
    assert!(tools[0].http);
    let (m, u) = http_target("GET {api}/api/cards/{card}", json!({ "card": "4515 x" }).as_object().unwrap()).unwrap();
    assert_eq!((m.as_str(), u.as_str()), ("GET", format!("{}/api/cards/4515%20x", chorus_runner::skills::service_base("api").unwrap()).as_str()), "a value is encoded, never spliced raw");
}

#[test]
fn negative_proof_a_route_that_is_not_a_chorus_service_is_never_offered() {
    for bad in ["POST https://example.com/hook", "GET http://127.0.0.1:3102/loki/api/v1/query_range", "GET {loki}/loki/api/v1/query_range"] {
        let (tools, un) = skill_tools(&[row(bad)], &inputs());
        assert!(tools.is_empty(), "{bad}: the caller's credential would reach something that is not Chorus");
        assert!(un[0].reason.contains("{api}"), "{un:?}");
    }
}

#[test]
fn a_caller_run_http_tool_forwards_the_callers_own_credential() {
    let (tools, _) = skill_tools(&[row("POST {pulse}/api/messages")], &inputs());
    let args = json!({ "to": "silas", "message": "hi" }).as_object().unwrap().clone();
    let p = prepare_with(&tools[0], &args, "wren", "00-t", Some("the-callers-token")).unwrap();
    let (method, url, body, token) = p.http.unwrap();
    assert_eq!((method.as_str(), url.as_str()), ("POST", format!("{}/api/messages", chorus_runner::skills::service_base("pulse").unwrap()).as_str()));
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

#[test]
fn a_route_default_fills_a_left_out_argument_and_a_given_one_wins() {
    let route = "GET {api}/api/chorus/logs/card/{card}?window={time_window=1d}";
    let (_, u) = http_target(route, json!({ "card": "4515" }).as_object().unwrap()).unwrap();
    assert!(u.ends_with("/api/chorus/logs/card/4515?window=1d"), "{u}");
    let (_, u) = http_target(route, json!({ "card": "4515", "time_window": "1h" }).as_object().unwrap()).unwrap();
    assert!(u.ends_with("?window=1h"), "{u}");
    // NEGATIVE PROOF: a placeholder with no default still refuses when left out
    assert!(http_target(route, json!({ "time_window": "1h" }).as_object().unwrap()).unwrap_err().contains("card is needed"));
}

#[test]
fn a_get_sends_no_body() {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let h = std::thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut buf = vec![0u8; 8192];
        let n = s.read(&mut buf).unwrap();
        s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").unwrap();
        String::from_utf8_lossy(&buf[..n]).to_string()
    });
    let (ok, _, _) = run_http("GET", &format!("http://127.0.0.1:{port}/x?q=1"), r#"{"a":1}"#, "t", &new_trace_id(), 5);
    let req = h.join().unwrap();
    assert!(ok && req.starts_with("GET /x?q=1 "), "{req}");
    assert!(!req.contains(r#"{"a":1}"#) && !req.to_lowercase().contains("content-length"), "a GET carries no body: {req}");
}

#[test]
fn a_route_can_default_to_the_verified_caller_and_the_caller_cannot_spoof_it() {
    let shapes = json!({ "WipInput": { "type": "object", "properties": { "role": { "type": "string" } } } }).as_object().unwrap().clone();
    let r = json!({ "name": "chorus_wip", "executor": "http", "inputShape": "WipInput", "ownedBy": "principal-wren", "runsAs": "caller",
                    "implementedBy": "GET {api}/api/chorus/context/board/wip?role={role=@caller}&who={caller}" });
    let (tools, un) = skill_tools(&[r], &shapes);
    assert!(un.is_empty(), "{un:?}");
    let url = |args: Value| prepare_with(&tools[0], args.as_object().unwrap(), "principal-wren", &new_trace_id(), Some("t")).map(|p| p.http.unwrap().1);
    assert!(url(json!({})).unwrap().ends_with("?role=wren&who=wren"));
    assert!(url(json!({ "role": "kade" })).unwrap().ends_with("?role=kade&who=wren"));
    // NEGATIVE PROOF: sending `caller` never changes who the route says called
    match url(json!({ "caller": "kade" })) {
        Ok(u) => assert!(u.ends_with("&who=wren"), "{u}"),
        Err((e, code)) => assert_eq!(code, 400, "{e}"),
    }
}
