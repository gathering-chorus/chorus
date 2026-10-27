// @test-type: unit
// @domain: services
//! #4515 (Jeff 2026-10-14) — rows are read and written with one family of
//! tools named after the door's own URL shape: athena-get, athena-list,
//! athena-add, athena-set, and athena-delete (#4762: src/lib ATHENA_TOOLS, src/server
//! run_http sends a DELETE with no body) over /v1/<domain>/<class>[/<id>]. Each call goes to
//! the door as the caller, with the caller's own credential; a refusal from
//! the door is a failed call carrying the door's message; a name that is not
//! a plain domain, class or id never becomes a request. They replace
//! door_get/door_list.

mod common;
use chorus_runner::client::args_from_pairs;
use chorus_runner::server::run_athena;
use chorus_runner::{athena_call, new_trace_id};
use common::*;
use http::StatusCode;
use serde_json::{json, Map, Value};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;

fn cols() -> Vec<String> {
    vec!["/cards/cards".into(), "/decisions/adrs".into()]
}

fn args(v: Value) -> Map<String, Value> {
    v.as_object().unwrap().clone()
}

/// A loopback fake door: answers one request with `status` and `body`, and
/// hands back the request it saw.
fn fake_door(status: &'static str, body: &'static str) -> (String, JoinHandle<String>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://127.0.0.1:{}", l.local_addr().unwrap().port());
    let h = std::thread::spawn(move || {
        // a call that never comes fails the test, never hangs it
        l.set_nonblocking(true).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut s = loop {
            match l.accept() {
                Ok((s, _)) => break s,
                Err(_) if std::time::Instant::now() < deadline => std::thread::sleep(std::time::Duration::from_millis(20)),
                Err(_) => return String::new(),
            }
        };
        s.set_nonblocking(false).unwrap();
        let mut req = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            let n = s.read(&mut buf).unwrap();
            req.extend_from_slice(&buf[..n]);
            let text = String::from_utf8_lossy(&req).to_string();
            if let Some(end) = text.find("\r\n\r\n") {
                let len = text
                    .lines()
                    .find_map(|l| l.to_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)))
                    .unwrap_or(0);
                if req.len() >= end + 4 + len || n == 0 {
                    break;
                }
            }
            if n == 0 {
                break;
            }
        }
        s.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).unwrap();
        String::from_utf8_lossy(&req).to_string()
    });
    (base, h)
}

/// A listener that must never be called: true when nothing connected.
fn untouched(l: &TcpListener) -> bool {
    l.set_nonblocking(true).unwrap();
    matches!(l.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock)
}

fn call(base: &str, tool: &str, a: Value) -> (bool, String, u16) {
    run_athena(base, tool, &args(a), &cols(), "principal-wren", Some("the-callers-token"), &new_trace_id())
}

#[test]
fn get_list_add_set_hit_the_right_method_and_path_as_the_caller() {
    let cases: [(&str, Value, &str, Option<Value>); 5] = [
        ("athena-get", json!({"domain":"cards","class":"cards","id":"card-4515"}), "GET /v1/cards/cards/card-4515 ", None),
        ("athena-list", json!({"domain":"decisions","class":"adrs","limit":5}), "GET /v1/decisions/adrs?limit=5 ", None),
        ("athena-add", json!({"domain":"cards","class":"cards","fields":{"title":"x"}}), "POST /v1/cards/cards ", Some(json!({"title":"x"}))),
        ("athena-set", json!({"domain":"cards","class":"cards","id":"card-4515","fields":{"status":"done"}}), "PATCH /v1/cards/cards/card-4515 ", Some(json!({"status":"done"}))),
        // #4762 — a retired row leaves through the same door, as the caller
        ("athena-delete", json!({"domain":"cards","class":"cards","id":"card-4762"}), "DELETE /v1/cards/cards/card-4762 ", None),
    ];
    for (tool, a, line, body) in cases {
        let (base, h) = fake_door("200 OK", r#"{"ok":true}"#);
        let (ok, text, code) = call(&base, tool, a);
        assert!(ok && code == 200, "{tool}: {text}");
        let req = h.join().unwrap();
        assert!(req.starts_with(line), "{tool}: {req}");
        assert!(req.contains("Authorization: Bearer the-callers-token"), "{tool} forwards the caller's own credential: {req}");
        match body {
            Some(b) => {
                let sent = req.split("\r\n\r\n").nth(1).unwrap();
                assert_eq!(serde_json::from_str::<Value>(sent).unwrap(), b, "{tool}");
            }
            None => assert!(!req.to_lowercase().contains("content-length"), "{tool}: a read carries no body: {req}"),
        }
    }
}

/// NEGATIVE PROOF: a 403 from the door is a failed call with the door's
/// message, never a success.
#[test]
fn a_403_from_the_door_is_a_failed_call_with_its_message() {
    let (base, h) = fake_door("403 Forbidden", r#"{"error":"wren may not write cards owned by silas"}"#);
    let (ok, text, code) = call(&base, "athena-set", json!({"domain":"cards","class":"cards","id":"card-1","fields":{"status":"done"}}));
    h.join().unwrap();
    assert!(!ok, "a 403 is not success: {text}");
    assert_eq!(code, 403);
    assert!(text.contains("wren may not write cards owned by silas"), "the door's message reaches the caller: {text}");
}

/// NEGATIVE PROOF: a domain, class or id that is not a plain name is refused,
/// by name, before any request — no path traversal reaches the door.
#[test]
fn a_bad_name_is_refused_before_any_request() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://127.0.0.1:{}", l.local_addr().unwrap().port());
    for (field, a) in [
        ("domain", json!({"domain":"../x","class":"cards","id":"a"})),
        ("domain", json!({"domain":"Cards","class":"cards","id":"a"})),
        ("class", json!({"domain":"cards","class":"cards/../../security","id":"a"})),
        ("id", json!({"domain":"cards","class":"cards","id":"../../security/credentials/x"})),
        ("id", json!({"domain":"cards","class":"cards","id":"a?b=1"})),
    ] {
        let (ok, text, code) = call(&base, "athena-get", a.clone());
        assert!(!ok && code == 400, "{a}: {text}");
        assert!(text.contains(&format!("{field} ")), "refused naming the {field}: {text}");
    }
    assert!(untouched(&l), "nothing reached the door");
}

#[test]
fn a_collection_the_door_does_not_serve_is_refused() {
    let bad = athena_call("athena-list", &args(json!({"domain":"security","class":"credentials"})), &cols()).unwrap_err();
    assert!(bad.reason.contains("does not serve /security/credentials"), "{}", bad.reason);
}

/// NEGATIVE PROOF: add (and set) without fields is refused before any request.
#[test]
fn add_without_fields_is_refused() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://127.0.0.1:{}", l.local_addr().unwrap().port());
    for (tool, a) in [
        ("athena-add", json!({"domain":"cards","class":"cards"})),
        ("athena-add", json!({"domain":"cards","class":"cards","fields":{}})),
        ("athena-add", json!({"domain":"cards","class":"cards","fields":"not json"})),
        ("athena-set", json!({"domain":"cards","class":"cards","id":"card-1"})),
    ] {
        let (ok, text, code) = call(&base, tool, a.clone());
        assert!(!ok && code == 400 && text.contains("fields"), "{tool} {a}: {text}");
    }
    assert!(untouched(&l), "nothing reached the door");
}

#[test]
fn the_cli_key_value_form_makes_the_same_call() {
    // chorus-runner call athena-add domain=cards class=cards fields='{"title":"x"}'
    let a = args_from_pairs(&["domain=cards".into(), "class=cards".into(), r#"fields={"title":"x"}"#.into()]).unwrap();
    let (method, path, body) = athena_call("athena-add", a.as_object().unwrap(), &cols()).unwrap();
    assert_eq!((method.as_str(), path.as_str()), ("POST", "/v1/cards/cards"));
    assert_eq!(serde_json::from_str::<Value>(&body.unwrap()).unwrap(), json!({"title":"x"}));
    let a = args_from_pairs(&["domain=cards".into(), "class=cards".into(), "limit=7".into()]).unwrap();
    assert_eq!(athena_call("athena-list", a.as_object().unwrap(), &cols()).unwrap().1, "/v1/cards/cards?limit=7");
}

/// NEGATIVE PROOF: the caller's credential never goes to a door off this
/// machine, and an unverified caller is refused.
#[test]
fn the_credential_stays_on_loopback_and_needs_a_caller() {
    let (ok, text, _) = call("http://door.example.com:3360", "athena-get", json!({"domain":"cards","class":"cards","id":"a"}));
    assert!(!ok && text.contains("not on this machine"), "{text}");
    let (ok, text, code) = run_athena("http://127.0.0.1:9", "athena-get", &args(json!({"domain":"cards","class":"cards","id":"a"})), &cols(), "unverified", None, &new_trace_id());
    assert!(!ok && code == 403, "{text}");
}

/// NEGATIVE PROOF: the old underscore tools are gone from tools/list.
#[tokio::test]
async fn tools_list_offers_the_athena_tools_and_not_door_get_or_door_list() {
    let (status, body) = send(rpc("tools/list", r#"{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientInfo":{"name":"t","version":"0"},"io.modelcontextprotocol/clientCapabilities":{}}}"#, "2026-07-28", &good_token())).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    for t in ["athena-get", "athena-list", "athena-add", "athena-set", "athena-delete"] {
        assert!(body.contains(&format!("\"{t}\"")), "{t} offered: {body}");
    }
    assert!(!body.contains("door_get") && !body.contains("door_list"), "{body}");
}

/// #4762 NEGATIVE PROOF: the door refuses a delete by a caller who does not
/// own the row; the refusal is a failed call with the door's message.
#[test]
fn a_delete_the_door_refuses_is_a_failed_call_with_its_message() {
    let (base, h) = fake_door("403 Forbidden", r#"{"error":"wren may not delete cards owned by silas"}"#);
    let (ok, text, code) = call(&base, "athena-delete", json!({"domain":"cards","class":"cards","id":"card-4762"}));
    let req = h.join().unwrap();
    assert!(req.starts_with("DELETE /v1/cards/cards/card-4762 "), "{req}");
    assert!(!ok && code == 403, "a refused delete is not success: {text}");
    assert!(text.contains("may not delete"), "{text}");
}

/// #4762 NEGATIVE PROOF: a delete with no id never becomes a request (it
/// would name the whole collection).
#[test]
fn a_delete_with_no_id_is_refused_before_any_request() {
    let cols = vec!["/security/permissions".to_string()];
    let a = json!({"domain":"security","class":"permissions"});
    let bad = chorus_runner::athena_call("athena-delete", a.as_object().unwrap(), &cols).unwrap_err();
    assert!(bad.reason.contains("id is required"), "{}", bad.reason);
}
