// @test-type: unit
// @domain: messages
// @card: #4916
// @owner: silas
//! #4916 — an alert about the roles door, or about delivery itself, must reach its owner
//! without going through the roles door (when the door is dead, its alarm cannot ride on it).

use chorus_hooks::nudge_send::{machine_sender, send, Deps};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

type Seen = Arc<Mutex<Vec<String>>>;

const DISCOVERY_FIXTURE: &str = r#"{"apiVersion":"v1","service":"athena-make","kind":"Discovery","count":105,"primitives":[]}"#;

fn stub(roles: &'static str, pulse_status: u16, pulse_body: &'static str) -> (String, Seen) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        for mut sock in listener.incoming().flatten() {
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
                (200, r#"{"value":100}"#.to_string())
            } else if line.contains("/api/nudge") {
                (pulse_status, pulse_body.to_string())
            } else {
                (404, "{}".into())
            };
            let _ = sock.write_all(format!("HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes());
        }
    });
    (base, seen)
}

fn args(v: &[&str]) -> Vec<String> { v.iter().map(|s| s.to_string()).collect() }

#[test]
fn only_machine_senders_skip_the_door() {
    for m in ["system", "pulse", "alert", "grafana", "herald", "sensor", "ops"] { assert!(machine_sender(m), "{m}"); }
    // negative proof: a role is never a machine sender, so a role's nudge still needs the door
    for r in ["wren", "silas", "kade", "jeff", "borg-queen", ""] { assert!(!machine_sender(r), "{r}"); }
}

#[test]
fn door_answering_discovery_refuses_regular_nudge() {
    let (base, seen) = stub(DISCOVERY_FIXTURE, 200, r#"{"resolved":"kade","delivery":"delivered"}"#);
    let deps = Deps {
        door: base.clone(),
        api: base.clone(),
        pulse_url: format!("{base}/api/nudge"),
        pulse_secret: Some("s3".into()),
        identity_token: None,
        spine: &|_e, _r, _p| {},
        buzz: None,
        wake: None,
    };
    let res = send(&args(&["wren", "kade", "hello"]), &deps);
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("the roles door answered with no data list"));
    // Verified that door was called
    let calls = seen.lock().unwrap().clone();
    assert!(calls.iter().any(|c| c.contains("/v1/roles/roles")));
}

#[test]
fn negative_proof_door_down_and_a_machine_alert_still_reaches_pulse() {
    let (base, seen) = stub(DISCOVERY_FIXTURE, 200, r#"{"resolved":"silas @ /dev/ttys012","delivery":"delivered"}"#);
    let deps = Deps {
        door: base.clone(),
        api: base.clone(),
        pulse_url: format!("{base}/api/nudge"),
        pulse_secret: Some("s3".into()),
        identity_token: None,
        spine: &|_e, _r, _p| {},
        buzz: None,
        wake: None,
    };
    let res = send(&args(&["system", "silas", "grafana: Roles door: pulse could not read the peer list — the roles door answered HTTP 502"]), &deps);
    assert!(res.is_ok(), "nudge-send failed: {:?}", res);
    let line = res.unwrap();
    assert!(line.contains("nudge delivered: system → silas @ /dev/ttys012"));

    // the door was read and could not answer; the alert went on to pulse anyway
    let calls = seen.lock().unwrap().clone();
    assert!(calls.iter().any(|c| c.contains("/v1/roles/roles")), "the door is still asked first: {:?}", calls);
    assert!(calls.iter().any(|c| c.contains("/api/nudge")), "pulse should be called: {:?}", calls);
}
