// @test-type: unit
// @domain: messages
//! #4515 — clearing-nudge moved into `chorus-hook-shim nudge-send`.
//! What a role sees: a nudge to a listed role arrives through pulse and says
//! where it went; a nudge to anyone else, or over the nudge cap, is refused
//! with the reason and leaves no trace of having been sent.
//!
//! Each test uses its own sender name: the cap cache is per process and keyed
//! on the role, so a shared sender would let one test read another's cap.

use chorus_hooks::nudge_send::{peers_from, send, Deps};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

type Seen = Arc<Mutex<Vec<String>>>;

/// One stub for all three services, routed by path. Records each request line
/// and body so a test can prove what was (and was not) called.
fn stub(roles: &'static str, cap: Option<u32>, pulse_status: u16) -> (String, Seen) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        for mut sock in listener.incoming().flatten() {
            // Read the whole request (headers, then Content-Length of body)
            // before answering: answering early resets a client still writing.
            let mut raw = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                let n = sock.read(&mut buf).unwrap_or(0);
                if n == 0 { break; }
                raw.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&raw).to_string();
                if let Some(end) = text.find("\r\n\r\n") {
                    let len = text[..end].lines()
                        .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)))
                        .unwrap_or(0);
                    if raw.len() >= end + 4 + len { break; }
                }
            }
            let req = String::from_utf8_lossy(&raw).to_string();
            let line = req.lines().next().unwrap_or("").to_string();
            let body_in = req.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
            log.lock().unwrap().push(format!("{line} {body_in}"));
            let (status, body) = if line.contains("/v1/roles/roles") {
                (200, roles.to_string())
            } else if line.contains("/properties/resolve") {
                match cap { Some(c) => (200, format!(r#"{{"value":{c}}}"#)), None => (503, "{}".into()) }
            } else if line.contains("/api/nudge") {
                (pulse_status, r#"{"resolved":"wren @ /dev/ttys003"}"#.to_string())
            } else {
                (404, "{}".into())
            };
            let _ = sock.write_all(format!("HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes());
        }
    });
    (base, seen)
}

const ROLES: &str = r#"{"data":[{"name":"wren","roleKind":"agent"},{"name":"silas","roleKind":"agent"},{"name":"jeff","roleKind":"human"},{"name":"herald","roleKind":"service"}]}"#;

fn args(v: &[&str]) -> Vec<String> { v.iter().map(|s| s.to_string()).collect() }

fn words(n: usize) -> String { vec!["word"; n].join(" ") }

/// Runs one send; returns its result, the spine events written, and the requests the stub saw.
fn run(base: &str, a: &[&str]) -> (Result<String, String>, Vec<String>) {
    let spine: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let write = |e: &str, r: &str, p: &str| spine.lock().unwrap().push(format!("{e} {r} {p}"));
    let deps = Deps { door: base.into(), api: base.into(), pulse_url: format!("{base}/api/nudge"), pulse_secret: Some("s3".into()), spine: &write };
    let out = send(&args(a), &deps);
    let events = spine.lock().unwrap().clone();
    (out, events)
}

fn pulse_calls(seen: &Seen) -> Vec<String> {
    seen.lock().unwrap().iter().filter(|l| l.contains("/api/nudge")).cloned().collect()
}

#[test]
fn a_nudge_to_a_listed_role_goes_through_pulse_and_says_where_it_went() {
    let (base, seen) = stub(ROLES, Some(50), 200);
    let (out, events) = run(&base, &["sender-a", "wren", "AC3 draft is in the scratch file", "--expects", "reply"]);
    let line = out.expect("sent");
    assert!(line.starts_with("nudge sent: sender-a → wren @ /dev/ttys003 (trace="), "{line}");
    assert_eq!(events.len(), 2, "requested + emitted, before the send: {events:?}");
    assert!(events[0].starts_with("nudge.requested sender-a from=sender-a,to=wren,"), "{events:?}");
    assert!(events[1].starts_with("nudge.emitted sender-a "), "{events:?}");
    let calls = pulse_calls(&seen);
    assert_eq!(calls.len(), 1);
    assert!(calls[0].contains(r#""content":"AC3 draft is in the scratch file""#) && calls[0].contains(r#""expects":"reply""#) && calls[0].contains(r#""from":"sender-a""#), "{calls:?}");
}

#[test]
fn an_unknown_recipient_is_refused_and_nothing_is_recorded_or_sent() {
    let (base, seen) = stub(ROLES, Some(50), 200);
    let (out, events) = run(&base, &["sender-b", "herald", "hello"]);
    let why = out.expect_err("a service role is not a peer");
    assert!(why.contains("Unknown recipient 'herald'") && why.contains("jeff | silas | wren"), "{why}");
    assert!(events.is_empty());
    assert!(pulse_calls(&seen).is_empty());
}

#[test]
fn negative_proof_an_over_cap_nudge_is_refused_with_its_count_and_never_sent() {
    let (base, seen) = stub(ROLES, Some(50), 200);
    let (out, events) = run(&base, &["sender-c", "wren", &words(51)]);
    let why = out.expect_err("51 words over a 50 cap");
    assert!(why.starts_with("word-cap: 51 words, cap is 50."), "{why}");
    assert!(events.is_empty(), "an over-cap nudge leaves no trace: {events:?}");
    assert!(pulse_calls(&seen).is_empty());
    let (ok, _) = run(&base, &["sender-c", "wren", &words(50)]);
    assert!(ok.is_ok(), "exactly the cap is allowed: {ok:?}");
}

#[test]
fn the_nudge_cap_fails_open_to_50_not_the_response_cap_of_100() {
    let (base, _) = stub(ROLES, None, 200);
    let (out, _) = run(&base, &["sender-d", "wren", &words(51)]);
    assert!(out.expect_err("store down: nudge cap is 50").contains("cap is 50"));
}

#[test]
fn a_failed_pulse_post_fails_the_call() {
    let (base, _) = stub(ROLES, Some(50), 500);
    let (out, _) = run(&base, &["sender-e", "wren", "hello"]);
    assert!(out.expect_err("pulse 500").starts_with("pulse POST returned 500"));
}

#[test]
fn a_door_that_does_not_answer_refuses_rather_than_guessing() {
    let (out, events) = run("http://127.0.0.1:9", &["sender-f", "wren", "hello"]);
    assert!(out.expect_err("door down").contains("the roles door did not answer"));
    assert!(events.is_empty());
}

#[test]
fn an_unverified_sender_sends_nothing() {
    let (base, seen) = stub(ROLES, Some(50), 200);
    let (out, _) = run(&base, &["unverified", "wren", "hello"]);
    assert!(out.expect_err("no caller").contains("verified sender"));
    assert!(pulse_calls(&seen).is_empty());
}

#[test]
fn peers_are_agents_and_humans_only() {
    let v: serde_json::Value = serde_json::from_str(ROLES).unwrap();
    assert_eq!(peers_from(&v).unwrap(), vec!["jeff", "silas", "wren"]);
    let none: serde_json::Value = serde_json::from_str(r#"{"data":[{"name":"jeff","roleKind":"human"}]}"#).unwrap();
    assert!(peers_from(&none).is_err(), "a door with no agent is refused");
}
