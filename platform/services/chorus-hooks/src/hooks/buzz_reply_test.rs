//! #3893 red-first tests for the Buzz reply publish leg (DEC-1674).
//! Written BEFORE buzz_reply.rs exists — cargo test is RED (compile fail)
//! until the module lands. Kept as the module's test half via `include!`
//! from buzz_reply.rs, so the red→green transition is observable in git.

use crate::hooks::buzz_reply::*;
use secp256k1::hashes::{sha256, Hash};
use secp256k1::{Keypair, Message, Secp256k1, SecretKey, XOnlyPublicKey};
use std::io::Write as _;
use std::os::unix::fs::PermissionsExt;

const SK: &str = "0000000000000000000000000000000000000000000000000000000000000003";

fn hexd(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}
fn hexe(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn write_key(dir: &std::path::Path, role: &str, mode: u32) -> std::path::PathBuf {
    let secp = Secp256k1::new();
    let kp = Keypair::from_secret_key(&secp, &SecretKey::from_slice(&hexd(SK)).unwrap());
    let pk = hexe(&XOnlyPublicKey::from_keypair(&kp).0.serialize());
    let d = dir.join(role);
    std::fs::create_dir_all(&d).unwrap();
    let p = d.join("nostr.json");
    let mut f = std::fs::File::create(&p).unwrap();
    write!(f, r#"{{"pubkey":"{pk}","seckey":"{SK}"}}"#).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(mode)).unwrap();
    p
}

#[test]
fn key_loads_and_pubkey_matches_seckey() {
    let dir = tempfile::tempdir().unwrap();
    write_key(dir.path(), "kade", 0o600);
    let k = load_key(dir.path(), "kade").unwrap();
    assert_eq!(k.pubkey.len(), 64);
}

#[test]
fn missing_key_is_err() {
    let dir = tempfile::tempdir().unwrap();
    assert!(load_key(dir.path(), "kade").unwrap_err().contains("no key"));
}

#[test]
fn world_readable_key_is_refused() {
    // negative proof (#3734): the guarded condition — leaked credential —
    // must FAIL loudly, not load anyway.
    let dir = tempfile::tempdir().unwrap();
    write_key(dir.path(), "kade", 0o644);
    assert!(load_key(dir.path(), "kade").unwrap_err().contains("REFUSED"));
}

#[test]
fn mismatched_pubkey_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let p = write_key(dir.path(), "kade", 0o600);
    // corrupt 8 chars of the stored pubkey
    let raw = std::fs::read_to_string(&p).unwrap();
    let corrupted = raw.replacen(&raw[12..20], "deadbeef", 1);
    std::fs::write(&p, corrupted).unwrap();
    assert!(load_key(dir.path(), "kade").is_err());
}

#[test]
fn event_id_is_sha256_of_canonical_form_and_sig_verifies() {
    let dir = tempfile::tempdir().unwrap();
    write_key(dir.path(), "kade", 0o600);
    let k = load_key(dir.path(), "kade").unwrap();
    let ev = build_reply_event(&k, "kade", "the reply", "abc123", 1_786_800_000).unwrap();
    // id = sha256 of the NIP-01 canonical serialization
    let ser = serde_json::json!([0, ev.pubkey, ev.created_at, 1, ev.tags, ev.content]).to_string();
    assert_eq!(ev.id, hexe(sha256::Hash::hash(ser.as_bytes()).as_ref()));
    // schnorr sig verifies against the x-only pubkey
    let secp = Secp256k1::new();
    let msg = Message::from_digest(sha256::Hash::hash(ser.as_bytes()).to_byte_array());
    let sig = secp256k1::schnorr::Signature::from_slice(&hexd(&ev.sig)).unwrap();
    let pk = XOnlyPublicKey::from_slice(&hexd(&ev.pubkey)).unwrap();
    assert!(secp.verify_schnorr(&sig, &msg, &pk).is_ok());
    // the #3864 join key rides as a tag — correlation survives the transport
    assert!(ev.tags.iter().any(|t| t[0] == "hash" && t[1] == "abc123"));
}

#[test]
fn publish_ok_true_is_ok() {
    let (url, handle) = relay_stub(|id| format!(r#"["OK","{id}",true,""]"#));
    let dir = tempfile::tempdir().unwrap();
    write_key(dir.path(), "kade", 0o600);
    let k = load_key(dir.path(), "kade").unwrap();
    let ev = build_reply_event(&k, "kade", "t", "h", 1).unwrap();
    assert!(publish(&url, &ev).is_ok());
    handle.join().unwrap();
}

#[test]
fn publish_relay_refusal_is_err() {
    // negative proof: a relay answering OK-false must surface as Err
    let (url, handle) = relay_stub(|id| format!(r#"["OK","{id}",false,"blocked: not allowed"]"#));
    let dir = tempfile::tempdir().unwrap();
    write_key(dir.path(), "kade", 0o600);
    let k = load_key(dir.path(), "kade").unwrap();
    let ev = build_reply_event(&k, "kade", "t", "h", 1).unwrap();
    let e = publish(&url, &ev).unwrap_err();
    assert!(e.contains("refused"), "{e}");
    handle.join().unwrap();
}

#[test]
fn publish_relay_down_is_err() {
    let dir = tempfile::tempdir().unwrap();
    write_key(dir.path(), "kade", 0o600);
    let k = load_key(dir.path(), "kade").unwrap();
    let ev = build_reply_event(&k, "kade", "t", "h", 1).unwrap();
    assert!(publish("ws://127.0.0.1:1", &ev).is_err());
}

/// Minimal in-test relay: accept one WS, read the EVENT, answer via `f`.
fn relay_stub(f: fn(&str) -> String) -> (String, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let h = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut ws = tungstenite::accept(stream).unwrap();
        if let tungstenite::Message::Text(t) = ws.read().unwrap() {
            let v: serde_json::Value = serde_json::from_str(&t).unwrap();
            assert_eq!(v[0], "EVENT");
            let id = v[1]["id"].as_str().unwrap().to_string();
            ws.send(tungstenite::Message::Text(f(&id))).unwrap();
        }
    });
    (url, h)
}

/// #3893 live probe — real kade key, real Bedroom relay, one marked event.
/// Integration-gated per #3528 (a test brings its own world; this one
/// deliberately doesn't): runs ONLY with RUN_LIVE_INTEGRATION=1.
#[test]
fn live_publish_probe() {
    if std::env::var("RUN_LIVE_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let dir = std::path::PathBuf::from(std::env::var("HOME").unwrap()).join(".chorus/identity");
    let key = load_key(&dir, "kade").expect("kade key at contract path");
    let created = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let ev = build_reply_event(&key, "kade", "[#3893 probe] stop-hook publish path live-check", "probe000", created).unwrap();
    let relay = std::env::var("BUZZ_RELAY_URL").unwrap_or_else(|_| "ws://192.168.86.242:3000".into());
    let tunnel = std::env::var("BUZZ_RELAY_TUNNEL").ok();
    publish_authed(&relay, tunnel.as_deref(), &key, &ev).expect("relay accepted");
    eprintln!("relay accepted event {}", ev.id);
}

// #3893 seq tag (Wren's completeness red-line): order is a protocol property,
// COMPLETENESS is not — "message 5 never arrived" is only computable against
// a sender-side counter. Per-role, monotonic, survives restarts.
#[test]
fn seq_is_monotonic_per_role_and_persisted() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(next_seq(dir.path(), "kade"), 1);
    assert_eq!(next_seq(dir.path(), "kade"), 2);
    assert_eq!(next_seq(dir.path(), "wren"), 1);
    let on_disk: u64 = std::fs::read_to_string(dir.path().join("reply-seq-kade"))
        .unwrap().trim().parse().unwrap();
    assert_eq!(on_disk, 2);
}

#[test]
fn seq_corrupt_counter_restarts_instead_of_wedging() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("reply-seq-kade"), "not-a-number").unwrap();
    assert_eq!(next_seq(dir.path(), "kade"), 1);
}

#[test]
fn reply_event_carries_seq_tag() {
    let dir = tempfile::tempdir().unwrap();
    write_key(dir.path(), "kade", 0o600);
    let k = load_key(dir.path(), "kade").unwrap();
    let ev = build_reply_event_seq(&k, "kade", "t", "h", 1, 42).unwrap();
    assert!(ev.tags.iter().any(|t| t[0] == "seq" && t[1] == "42"), "{:?}", ev.tags);
}

// #3893 tunnel leg: the relay virtual-hosts by Host header (LAN ip → 200,
// anything else → 404, proven live 2026-08-15). Through the LNP loopback
// tunnel the TCP endpoint differs from the URL, so publish must dial the
// tunnel while keeping the relay URL's Host. BUZZ_RELAY_TUNNEL=host:port.
#[test]
fn publish_via_tunnel_keeps_relay_host_header() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let tunnel = listener.local_addr().unwrap().to_string();
    let h = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        // hand-roll the handshake so we can SEE the Host header
        let mut stream = stream;
        use std::io::{Read as _, Write as _};
        let mut buf = [0u8; 2048];
        let n = stream.read(&mut buf).unwrap();
        let req = String::from_utf8_lossy(&buf[..n]).to_string();
        // answer 404 like the real relay does on a wrong Host — the test
        // asserts on the captured request, not on publish succeeding
        stream.write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\n\r\n").unwrap();
        req
    });
    let dir = tempfile::tempdir().unwrap();
    write_key(dir.path(), "kade", 0o600);
    let k = load_key(dir.path(), "kade").unwrap();
    let ev = build_reply_event(&k, "kade", "t", "h", 1).unwrap();
    let _ = publish_via("ws://192.168.86.242:3000", Some(&tunnel), &ev);
    let req = h.join().unwrap();
    assert!(req.contains("Host: 192.168.86.242:3000"), "Host must be the relay's, got: {req}");
}

// NIP-42: the live relay refused with "auth-required" (2026-08-15) — it
// challenges with ["AUTH", chal]; the client answers a signed kind-22242
// event carrying relay+challenge tags, then re-sends the EVENT.
#[test]
fn publish_answers_nip42_auth_challenge() {
    use std::io::Write as _;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let url2 = url.clone();
    let h = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut ws = tungstenite::accept(stream).unwrap();
        // first EVENT arrives → challenge instead of OK
        let first = ws.read().unwrap();
        let v: serde_json::Value = serde_json::from_str(first.to_text().unwrap()).unwrap();
        assert_eq!(v[0], "EVENT");
        let id = v[1]["id"].as_str().unwrap().to_string();
        ws.send(tungstenite::Message::Text(format!(r#"["OK","{id}",false,"auth-required: not authenticated"]"#))).unwrap();
        ws.send(tungstenite::Message::Text(r#"["AUTH","chal-123"]"#.to_string())).unwrap();
        // client must answer AUTH with a signed 22242 carrying our challenge
        let auth = ws.read().unwrap();
        let a: serde_json::Value = serde_json::from_str(auth.to_text().unwrap()).unwrap();
        assert_eq!(a[0], "AUTH");
        assert_eq!(a[1]["kind"], 22242);
        let tags = a[1]["tags"].as_array().unwrap();
        assert!(tags.iter().any(|t| t[0] == "challenge" && t[1] == "chal-123"), "{tags:?}");
        assert!(tags.iter().any(|t| t[0] == "relay"), "{tags:?}");
        // then the EVENT again → accept it
        let second = ws.read().unwrap();
        let v2: serde_json::Value = serde_json::from_str(second.to_text().unwrap()).unwrap();
        assert_eq!(v2[0], "EVENT");
        let id2 = v2[1]["id"].as_str().unwrap().to_string();
        ws.send(tungstenite::Message::Text(format!(r#"["OK","{id2}",true,""]"#))).unwrap();
        let _ = ws.flush();
    });
    let dir = tempfile::tempdir().unwrap();
    write_key(dir.path(), "kade", 0o600);
    let k = load_key(dir.path(), "kade").unwrap();
    let ev = build_reply_event(&k, "kade", "t", "h", 1).unwrap();
    publish_authed(&url2, None, &k, &ev).expect("auth flow succeeds");
    h.join().unwrap();
}

// Live frame order (2026-08-15, node trace): AUTH challenge arrives FIRST,
// the pre-auth send's refusal SECOND, the re-send's OK true LAST. The first
// implementation returned Err on that middle frame.
#[test]
fn publish_survives_auth_refusal_arriving_after_challenge() {
    use std::io::Write as _;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let h = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut ws = tungstenite::accept(stream).unwrap();
        let first = ws.read().unwrap();
        let v: serde_json::Value = serde_json::from_str(first.to_text().unwrap()).unwrap();
        let id = v[1]["id"].as_str().unwrap().to_string();
        ws.send(tungstenite::Message::Text(r#"["AUTH","chal-x"]"#.to_string())).unwrap();
        ws.send(tungstenite::Message::Text(format!(r#"["OK","{id}",false,"auth-required: not authenticated"]"#))).unwrap();
        let _auth = ws.read().unwrap();
        let second = ws.read().unwrap();
        let v2: serde_json::Value = serde_json::from_str(second.to_text().unwrap()).unwrap();
        let id2 = v2[1]["id"].as_str().unwrap().to_string();
        ws.send(tungstenite::Message::Text(format!(r#"["OK","{id2}",true,""]"#))).unwrap();
        let _ = ws.flush();
    });
    let dir = tempfile::tempdir().unwrap();
    write_key(dir.path(), "kade", 0o600);
    let k = load_key(dir.path(), "kade").unwrap();
    let ev = build_reply_event(&k, "kade", "t", "h", 1).unwrap();
    publish_authed(&url, None, &k, &ev).expect("survives live ordering");
    h.join().unwrap();
}
