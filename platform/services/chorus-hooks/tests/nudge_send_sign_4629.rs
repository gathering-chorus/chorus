// @test-type: unit — stub roles door + pulse over HTTP and a stub relay over a websocket on local sockets; temp key, temp outbox.
// @domain: messages
//! #4629 S2 — the sender writes its own note to the room. With signing on,
//! nudge-send signs a kind 9 #team note with the sender's key (msg=trace-<uuid>,
//! the trace pulse stores), POSTs with buzz=sender so pulse makes no bridge
//! copy, then publishes. A relay that is down queues the signed note in the
//! sender's outbox (0700/0600) and the nudge is still sent; the next send
//! flushes the outbox first. No key: no claim of buzz=sender.

use chorus_hooks::nudge_send::{send, BuzzDeps, Deps};
use secp256k1::{Keypair, Secp256k1, SecretKey};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

type Seen = Arc<Mutex<Vec<String>>>;

fn http_stub() -> (String, Seen) {
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
            log.lock().unwrap().push(format!("{line} {}", req.split("\r\n\r\n").nth(1).unwrap_or("")));
            let (status, body) = if line.contains("/v1/roles/roles") {
                (200, r#"{"data":[{"name":"wren","roleKind":"agent"},{"name":"silas","roleKind":"agent"}]}"#.to_string())
            } else if line.contains("/properties/resolve") {
                (200, r#"{"value":50}"#.to_string())
            } else {
                (200, r#"{"resolved":"silas @ /dev/ttys010"}"#.to_string())
            };
            let _ = sock.write_all(format!("HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes());
        }
    });
    (base, seen)
}

/// A relay that answers every EVENT with OK true and records it.
fn relay_stub() -> (String, Arc<Mutex<Vec<serde_json::Value>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let got = Arc::new(Mutex::new(Vec::new()));
    let log = got.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let Ok(mut ws) = tungstenite::accept(stream) else { continue };
            while let Ok(msg) = ws.read() {
                let Ok(text) = msg.into_text() else { break };
                let Ok(f) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
                if f[0] == "EVENT" {
                    let id = f[1]["id"].as_str().unwrap_or("").to_string();
                    log.lock().unwrap().push(f[1].clone());
                    let _ = ws.send(tungstenite::Message::Text(format!(r#"["OK","{id}",true,""]"#)));
                }
            }
        }
    });
    (url, got)
}

fn temp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("nudge-sign-4629-{tag}-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Writes a 0600 key for `role` under `dir` and returns its pubkey.
fn key_for(dir: &Path, role: &str) -> String {
    let secp = Secp256k1::new();
    let sk = SecretKey::from_slice(&[7u8; 32]).unwrap();
    let pk = Keypair::from_secret_key(&secp, &sk).x_only_public_key().0.to_string();
    let d = dir.join(role);
    std::fs::create_dir_all(&d).unwrap();
    let f = d.join("nostr.json");
    std::fs::write(&f, format!(r#"{{"seckey":"{}","pubkey":"{pk}"}}"#, "07".repeat(32))).unwrap();
    std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o600)).unwrap();
    pk
}

fn run(base: &str, buzz: Option<BuzzDeps>, msg: &str) -> (Result<String, String>, Vec<String>) {
    let spine: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let write = |e: &str, r: &str, p: &str| spine.lock().unwrap().push(format!("{e} {r} {p}"));
    let deps = Deps { door: base.into(), api: base.into(), pulse_url: format!("{base}/api/nudge"), pulse_secret: Some("s3".into()), identity_token: None, spine: &write, buzz };
    let out = send(&["wren".into(), "silas".into(), msg.into()], &deps);
    let events = spine.lock().unwrap().clone();
    (out, events)
}

fn buzz(id: &Path, relay: &str, outbox: &Path) -> BuzzDeps {
    BuzzDeps { identity_dir: id.into(), relay_url: Some(relay.into()), tunnel: None, outbox_dir: outbox.into(), channel: "team-chan".into() }
}

fn tag<'a>(ev: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    ev["tags"].as_array()?.iter().find(|t| t[0] == name).and_then(|t| t[1].as_str())
}

#[test]
fn the_sender_signs_its_own_note_and_pulse_is_told_not_to_copy_it() {
    let (base, seen) = http_stub();
    let (relay, got) = relay_stub();
    let (id, outbox) = (temp("id"), temp("out"));
    let pk = key_for(&id, "wren");
    let (out, spine) = run(&base, Some(buzz(&id, &relay, &outbox)), "hello from the room");
    let line = out.expect("sent");
    assert!(line.contains("Buzz: stored"), "{line}");
    let post = seen.lock().unwrap().iter().find(|l| l.contains("/api/nudge")).cloned().unwrap();
    assert!(post.contains(r#""buzz":"sender""#), "{post}");
    let ev = got.lock().unwrap()[0].clone();
    assert_eq!(ev["pubkey"], pk);
    assert_eq!(ev["kind"], 9);
    assert_eq!(tag(&ev, "h"), Some("team-chan"));
    assert_eq!((tag(&ev, "author"), tag(&ev, "to"), tag(&ev, "kind")), (Some("wren"), Some("silas"), Some("nudge")));
    let trace = post.split(r#""traceId":""#).nth(1).unwrap().split('"').next().unwrap();
    assert_eq!(tag(&ev, "msg"), Some(format!("trace-{trace}").as_str()), "the note names the row pulse stores by trace");
    assert_eq!(ev["content"], "[nudge from wren → silas] hello from the room");
    assert!(spine.iter().any(|e| e.starts_with("buzz.publish.stored wren")));
    assert!(!seen.lock().unwrap().iter().any(|l| l.contains("/api/buzz/release")), "NEGATIVE PROOF: a stored note releases nothing; pulse waits for it in the room");
}

#[test]
fn negative_proof_relay_down_still_sends_and_queues_the_signed_note_owner_only() {
    let (base, seen) = http_stub();
    let (id, outbox) = (temp("id"), temp("out"));
    key_for(&id, "wren");
    let dead = { let l = TcpListener::bind("127.0.0.1:0").unwrap(); format!("ws://{}", l.local_addr().unwrap()) };
    let (out, spine) = run(&base, Some(buzz(&id, &dead, &outbox)), "relay is down");
    let line = out.expect("the nudge is still sent (Jeff 10-18: deliver anyway)");
    assert!(line.contains("Buzz: queued"), "{line}");
    assert!(seen.lock().unwrap().iter().any(|l| l.contains("/api/nudge")));
    let dir = outbox.join("wren");
    assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
    let files: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].metadata().unwrap().permissions().mode() & 0o777, 0o600);
    assert!(spine.iter().any(|e| e.starts_with("buzz.publish.queued wren")));
    // #4629 S3 — pulse holds a sender-signed nudge for its note; with the note queued, the sender releases it at once
    let calls = seen.lock().unwrap().clone();
    let trace = calls.iter().find(|l| l.contains("/api/nudge")).unwrap().split(r#""traceId":""#).nth(1).unwrap().split('"').next().unwrap().to_string();
    assert!(calls.iter().any(|l| l.contains("/api/buzz/release") && l.contains(&trace)), "{calls:?}");
}

#[test]
fn the_next_send_flushes_the_outbox_first() {
    let (base, _) = http_stub();
    let (id, outbox) = (temp("id"), temp("out"));
    key_for(&id, "wren");
    let dead = { let l = TcpListener::bind("127.0.0.1:0").unwrap(); format!("ws://{}", l.local_addr().unwrap()) };
    run(&base, Some(buzz(&id, &dead, &outbox)), "queued one").0.unwrap();
    let (relay, got) = relay_stub();
    let (_, spine) = run(&base, Some(buzz(&id, &relay, &outbox)), "relay is back");
    let contents: Vec<String> = got.lock().unwrap().iter().map(|e| e["content"].as_str().unwrap().to_string()).collect();
    assert_eq!(contents, vec!["[nudge from wren → silas] queued one", "[nudge from wren → silas] relay is back"]);
    assert_eq!(std::fs::read_dir(outbox.join("wren")).unwrap().count(), 0);
    assert!(spine.iter().any(|e| e.starts_with("buzz.publish.flushed wren")));
}

#[test]
fn negative_proof_no_key_means_no_buzz_sender_claim_so_pulse_keeps_its_copy() {
    let (base, seen) = http_stub();
    let (relay, got) = relay_stub();
    let (id, outbox) = (temp("id"), temp("out"));
    let (out, spine) = run(&base, Some(buzz(&id, &relay, &outbox)), "no key here");
    assert!(!out.unwrap().contains("Buzz:"));
    let post = seen.lock().unwrap().iter().find(|l| l.contains("/api/nudge")).cloned().unwrap();
    assert!(!post.contains("buzz"), "{post}");
    assert!(got.lock().unwrap().is_empty());
    assert!(spine.iter().any(|e| e.starts_with("buzz.sign.skipped wren")));
}

#[test]
fn negative_proof_no_relay_address_means_no_signing_and_no_literal_fallback() {
    let (base, seen) = http_stub();
    let (id, outbox) = (temp("id"), temp("out"));
    key_for(&id, "wren");
    let mut b = buzz(&id, "unused", &outbox);
    b.relay_url = None;
    let (out, spine) = run(&base, Some(b), "no relay set");
    assert!(!out.unwrap().contains("Buzz:"));
    let post = seen.lock().unwrap().iter().find(|l| l.contains("/api/nudge")).cloned().unwrap();
    assert!(!post.contains("buzz"), "pulse keeps its copy: {post}");
    assert!(spine.iter().any(|e| e.starts_with("buzz.sign.skipped wren") && e.contains("BUZZ_RELAY_URL")));
}

#[test]
fn a_flush_that_meets_a_down_relay_queues_the_new_note_without_trying_it() {
    let (base, _) = http_stub();
    let (id, outbox) = (temp("id"), temp("out"));
    key_for(&id, "wren");
    let dead = { let l = TcpListener::bind("127.0.0.1:0").unwrap(); format!("ws://{}", l.local_addr().unwrap()) };
    run(&base, Some(buzz(&id, &dead, &outbox)), "first").0.unwrap();
    let (out, spine) = run(&base, Some(buzz(&id, &dead, &outbox)), "second");
    assert!(out.unwrap().contains("Buzz: queued"));
    assert_eq!(std::fs::read_dir(outbox.join("wren")).unwrap().count(), 2, "both wait, oldest first next time");
    assert_eq!(spine.iter().filter(|e| e.starts_with("buzz.publish.queued")).count(), 1);
}

#[test]
fn negative_proof_an_unreadable_outbox_file_is_reported_not_skipped_silently() {
    let (base, _) = http_stub();
    let (relay, got) = relay_stub();
    let (id, outbox) = (temp("id"), temp("out"));
    key_for(&id, "wren");
    let dir = outbox.join("wren");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("broken.json"), "not json").unwrap();
    let (out, spine) = run(&base, Some(buzz(&id, &relay, &outbox)), "after a broken file");
    assert!(out.unwrap().contains("Buzz: stored"));
    assert!(spine.iter().any(|e| e.starts_with("buzz.outbox.unreadable wren") && e.contains("broken.json")));
    assert!(dir.join("broken.json").exists(), "left for a person to look at");
    assert_eq!(got.lock().unwrap().len(), 1);
}
