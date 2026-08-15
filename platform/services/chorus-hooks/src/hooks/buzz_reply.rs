//! #3893 — replies ride Buzz, not a bespoke path.
//!
//! Jeff's ruling, 2026-08-15, verbatim: "do not build a new custom delivery
//! path." Buzz (#3674, the Nostr relay on Bedroom) IS the delivery path: the
//! Stop hook publishes each final reply as a signed kind-1 event with the
//! role's derived-credential key; Clearing renders from a relay subscription
//! with replay (Wren's leg). Order and completeness are protocol properties —
//! event ids + created_at — so no bespoke seq/retry machinery here.
//!
//! Key contract (Silas's leg): `~/.chorus/identity/<role>/nostr.json`, mode
//! 0600, `{"pubkey": <hex-x-only>, "seckey": <hex>}`. Missing key = deploy
//! gap (spine event only); group/world-readable key = REFUSED — publishing
//! with a leaked credential is worse than not publishing.
//!
//! The `["hash", content_hash]` tag carries the #3864 join key, so the
//! emitted↔rendered correlation survives the transport change intact.

use secp256k1::hashes::{sha256, Hash};
use secp256k1::{Keypair, Message, Secp256k1, SecretKey, XOnlyPublicKey};
use std::os::unix::fs::PermissionsExt;

#[derive(Debug, Clone)]
pub struct RoleKey {
    pub pubkey: String,
    seckey: [u8; 32],
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct NostrEvent {
    pub id: String,
    pub pubkey: String,
    pub created_at: u64,
    pub kind: u32,
    pub tags: Vec<Vec<String>>,
    pub content: String,
    pub sig: String,
}

/// Load the role's derived nostr key. Err strings are the LOUD path.
pub fn load_key(identity_dir: &std::path::Path, role: &str) -> Result<RoleKey, String> {
    let path = identity_dir.join(role).join("nostr.json");
    let meta = std::fs::metadata(&path).map_err(|_| format!("no key at {}", path.display()))?;
    let mode = meta.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        return Err(format!(
            "REFUSED: {} is mode {mode:o}, group/world-readable — fix to 0600",
            path.display()
        ));
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("unreadable key: {e}"))?;
    let v: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("key not JSON: {e}"))?;
    let seckey_hex = v["seckey"].as_str().ok_or("key JSON missing seckey")?;
    let pubkey = v["pubkey"].as_str().ok_or("key JSON missing pubkey")?.to_string();
    let bytes = hex_decode(seckey_hex).ok_or("seckey not valid hex")?;
    let seckey: [u8; 32] = bytes.try_into().map_err(|_| "seckey not 32 bytes")?;
    // the stored pubkey must MATCH the seckey — a mismatch means the graph
    // binding points at a key this file cannot produce
    let secp = Secp256k1::new();
    let kp = Keypair::from_secret_key(
        &secp,
        &SecretKey::from_slice(&seckey).map_err(|e| format!("bad seckey: {e}"))?,
    );
    let derived = XOnlyPublicKey::from_keypair(&kp).0.serialize();
    if hex_encode(&derived) != pubkey.to_lowercase() {
        return Err("pubkey in file does not match seckey".to_string());
    }
    Ok(RoleKey { pubkey, seckey })
}

use std::sync::Mutex;

static SEQ_LOCK: Mutex<()> = Mutex::new(());

/// Per-role monotonic reply counter, persisted at <dir>/reply-seq-<role>.
/// Wren's completeness red-line: ordering is a relay property, completeness
/// is not — a HOLE ("message 5 never arrived") is only computable against a
/// sender-side counter. The single hooks daemon is the only writer (process
/// mutex); write-then-rename keeps a crash from corrupting the counter. A
/// corrupt/missing file restarts at 1 — the consumer treats a seq REGRESSION
/// as a counter reset, not a hole.
pub fn next_seq(dir: &std::path::Path, role: &str) -> u64 {
    let _g = SEQ_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let path = dir.join(format!("reply-seq-{role}"));
    let prev: u64 = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    let next = prev + 1;
    let _ = std::fs::create_dir_all(dir);
    let tmp = dir.join(format!(".reply-seq-{role}.tmp"));
    if std::fs::write(&tmp, next.to_string()).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
    next
}

/// Build + schnorr-sign a kind-1 reply event. Deterministic given inputs —
/// the caller owns the clock (created_at is an argument, never read here).
pub fn build_reply_event(
    key: &RoleKey,
    role: &str,
    text: &str,
    content_hash: &str,
    created_at: u64,
) -> Result<NostrEvent, String> {
    build_reply_event_tags(key, role, text, content_hash, created_at, None)
}

/// As build_reply_event, with the per-role seq riding as a ["seq", n] tag.
pub fn build_reply_event_seq(
    key: &RoleKey,
    role: &str,
    text: &str,
    content_hash: &str,
    created_at: u64,
    seq: u64,
) -> Result<NostrEvent, String> {
    build_reply_event_tags(key, role, text, content_hash, created_at, Some(seq))
}

fn build_reply_event_tags(
    key: &RoleKey,
    role: &str,
    text: &str,
    content_hash: &str,
    created_at: u64,
    seq: Option<u64>,
) -> Result<NostrEvent, String> {
    let mut tags = vec![
        vec!["t".to_string(), "reply".to_string()],
        vec!["role".to_string(), role.to_string()],
        vec!["hash".to_string(), content_hash.to_string()],
    ];
    if let Some(n) = seq {
        tags.push(vec!["seq".to_string(), n.to_string()]);
    }
    // NIP-01 canonical serialization: [0, pubkey, created_at, kind, tags, content]
    let ser = serde_json::json!([0, key.pubkey, created_at, 1, tags, text]).to_string();
    let digest = sha256::Hash::hash(ser.as_bytes());
    let id = hex_encode(digest.as_ref());
    let secp = Secp256k1::new();
    let kp = Keypair::from_secret_key(
        &secp,
        &SecretKey::from_slice(&key.seckey).map_err(|e| format!("bad seckey: {e}"))?,
    );
    let msg = Message::from_digest(digest.to_byte_array());
    let sig = secp.sign_schnorr_no_aux_rand(&msg, &kp);
    Ok(NostrEvent {
        id,
        pubkey: key.pubkey.clone(),
        created_at,
        kind: 1,
        tags,
        content: text.to_string(),
        sig: hex_encode(sig.as_ref()),
    })
}

/// Publish over the relay websocket; Ok only on the relay's ["OK", id, true].
/// Blocking (tungstenite) — call from spawn_blocking.
pub fn publish(relay_url: &str, event: &NostrEvent) -> Result<(), String> {
    let (mut ws, _) =
        tungstenite::connect(relay_url).map_err(|e| format!("relay connect: {e}"))?;
    let frame = serde_json::json!(["EVENT", event]).to_string();
    ws.send(tungstenite::Message::Text(frame))
        .map_err(|e| format!("relay send: {e}"))?;
    // read until the OK for OUR event id (relays may interleave other frames)
    for _ in 0..10 {
        let msg = ws.read().map_err(|e| format!("relay read: {e}"))?;
        if let tungstenite::Message::Text(t) = msg {
            let v: serde_json::Value = match serde_json::from_str(&t) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if v[0] == "OK" && v[1] == event.id.as_str() {
                return if v[2] == true {
                    Ok(())
                } else {
                    Err(format!("relay refused: {}", v[3].as_str().unwrap_or("")))
                };
            }
        }
    }
    Err("no OK from relay for our event".to_string())
}

fn hex_encode(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

/// The Stop hook's whole delivery step: load key → sign → publish → emit the
/// outcome on the spine. Returns Some(warning) when the failure must be LOUD
/// on the role's terminal (a missing key is a deploy gap — spine only).
pub async fn deliver_and_emit(role: &str, text: &str, content_hash: &str) -> Option<String> {
    let (r2, t2, h2) = (role.to_string(), text.to_string(), content_hash.to_string());
    let published = tokio::task::spawn_blocking(move || deliver_reply(&r2, &t2, &h2))
        .await
        .unwrap_or_else(|e| Err(format!("join: {e}")));
    match published {
        Ok(event_id) => {
            crate::state::chorus_log(
                "reply.published",
                role,
                &[("hash", content_hash), ("event_id", event_id.as_str()), ("surface", "buzz")],
            )
            .await;
            None
        }
        Err(e) => {
            crate::state::chorus_log(
                "reply.delivery.failed",
                role,
                &[("hash", content_hash), ("error", e.as_str()), ("surface", "buzz")],
            )
            .await;
            if e.starts_with("no key") {
                None
            } else {
                Some(format!(
                    "reply NOT delivered to Buzz: {e} — Jeff will not see this reply in Clearing (#3893: 100% delivery)"
                ))
            }
        }
    }
}

/// Blocking half: env + clock + key → signed event → relay accept.
fn deliver_reply(role: &str, text: &str, content_hash: &str) -> Result<String, String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/jeffbridwell".to_string());
    let identity_dir = std::path::PathBuf::from(home).join(".chorus/identity");
    let relay = std::env::var("BUZZ_RELAY_URL")
        .unwrap_or_else(|_| "ws://192.168.86.242:3000".to_string());
    let created_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let key = load_key(&identity_dir, role)?;
    let state_dir = std::path::PathBuf::from(
        std::env::var("HOME").unwrap_or_else(|_| "/Users/jeffbridwell".to_string()),
    )
    .join(".chorus/state");
    let seq = next_seq(&state_dir, role);
    let ev = build_reply_event_seq(&key, role, text, content_hash, created_at, seq)?;
    publish(&relay, &ev).map(|()| ev.id)
}
