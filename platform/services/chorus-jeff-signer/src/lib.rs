//! #4652 reopen 2 — Jeff's remote signer (NIP-46).
//!
//! Jeff's key lives with this process only. It runs as its own macOS user
//! (`chorus-jeff-signer`), from a root-owned binary, so no role session — all
//! of which run as jeffbridwell — can read the key or swap the code that holds it.
//!
//! What it signs, and nothing else:
//!   - kind 9 whose only `h` tag is #clearing (Clearing v2 posting as Jeff;
//!     Jeff 2026-10-20 13:01: his posts go to #clearing, never #team)
//!   - kind 22242 whose `relay` tag is our relay (the AUTH that lets that post in)
//!
//! Who it answers: the client keys it was started with. A request from any other
//! key gets no reply at all.
use hmac::{Hmac, Mac};
use secp256k1::{Keypair, Message, PublicKey, SecretKey, XOnlyPublicKey, SECP256K1};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

/// NIP-01 event id: sha256 of `[0,pubkey,created_at,kind,tags,content]`, compact JSON.
pub fn event_id(pubkey: &str, created_at: i64, kind: i64, tags: &Value, content: &str) -> String {
    let ser = serde_json::to_string(&json!([0, pubkey, created_at, kind, tags, content])).expect("json");
    hex::encode(Sha256::digest(ser.as_bytes()))
}

/// The key file holds hex or an `nsec1…` string; either way, hex comes out.
pub fn nip19_secret(raw: &str) -> Result<String, String> {
    let s = raw.trim();
    if s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()) {
        return Ok(s.to_ascii_lowercase());
    }
    let (hrp, data) = bech32_decode(s)?;
    if hrp != "nsec" {
        return Err(format!("expected an nsec, got {hrp}"));
    }
    let bytes = convert_bits(&data, 5, 8)?;
    if bytes.len() != 32 {
        return Err("an nsec holds 32 bytes".into());
    }
    Ok(hex::encode(bytes))
}

const CHARSET: &str = "qpzry9x8gf2tvdw0s3jn54khce6mua7l";

fn bech32_polymod(values: &[u8]) -> u32 {
    const GEN: [u32; 5] = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3];
    let mut chk: u32 = 1;
    for v in values {
        let top = chk >> 25;
        chk = ((chk & 0x1ffffff) << 5) ^ (*v as u32);
        for (i, g) in GEN.iter().enumerate() {
            if (top >> i) & 1 == 1 {
                chk ^= g;
            }
        }
    }
    chk
}

fn bech32_decode(s: &str) -> Result<(String, Vec<u8>), String> {
    let s = s.to_ascii_lowercase();
    let pos = s.rfind('1').ok_or("not bech32")?;
    let (hrp, rest) = (&s[..pos], &s[pos + 1..]);
    if hrp.is_empty() || rest.len() < 6 {
        return Err("not bech32".into());
    }
    let data: Vec<u8> = rest
        .chars()
        .map(|c| CHARSET.find(c).map(|i| i as u8).ok_or("bad bech32 character"))
        .collect::<Result<_, _>>()?;
    let mut check: Vec<u8> = hrp.bytes().map(|b| b >> 5).collect();
    check.push(0);
    check.extend(hrp.bytes().map(|b| b & 31));
    check.extend(&data);
    if bech32_polymod(&check) != 1 {
        return Err("bad bech32 checksum".into());
    }
    Ok((hrp.to_string(), data[..data.len() - 6].to_vec()))
}

fn convert_bits(data: &[u8], from: u32, to: u32) -> Result<Vec<u8>, String> {
    let (mut acc, mut bits, mut out) = (0u32, 0u32, Vec::new());
    let maxv = (1u32 << to) - 1;
    for v in data {
        acc = (acc << from) | (*v as u32);
        bits += from;
        while bits >= to {
            bits -= to;
            out.push(((acc >> bits) & maxv) as u8);
        }
    }
    if bits >= from || ((acc << (to - bits)) & maxv) != 0 {
        return Err("bad bech32 padding".into());
    }
    Ok(out)
}

/// NIP-44 v2 — the encryption NIP-46 requests and replies ride in.
pub mod nip44 {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine};

    fn hmac(key: &[u8], parts: &[&[u8]]) -> [u8; 32] {
        let mut m = HmacSha256::new_from_slice(key).expect("hmac key");
        for p in parts {
            m.update(p);
        }
        m.finalize().into_bytes().into()
    }

    /// HKDF-extract(salt "nip44-v2", ECDH x) — the same key on both sides.
    pub fn conversation_key(sk: &[u8], peer_xonly_hex: &str) -> Result<[u8; 32], String> {
        let sk = SecretKey::from_slice(sk).map_err(|e| e.to_string())?;
        let x = XOnlyPublicKey::from_slice(&hex::decode(peer_xonly_hex).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let peer = PublicKey::from_x_only_public_key(x, secp256k1::Parity::Even);
        let point = secp256k1::ecdh::shared_secret_point(&peer, &sk);
        let (prk, _) = hkdf::Hkdf::<Sha256>::extract(Some(b"nip44-v2"), &point[..32]);
        Ok(prk.into())
    }

    /// HKDF-expand(conversation key, nonce, 76) → chacha key, chacha nonce, hmac key.
    fn message_keys(ck: &[u8; 32], nonce: &[u8; 32]) -> ([u8; 32], [u8; 12], [u8; 32]) {
        let mut okm = [0u8; 76];
        hkdf::Hkdf::<Sha256>::from_prk(ck).expect("32-byte prk").expand(nonce, &mut okm).expect("76 bytes");
        let mut key = [0u8; 32];
        let mut cn = [0u8; 12];
        let mut hk = [0u8; 32];
        key.copy_from_slice(&okm[..32]);
        cn.copy_from_slice(&okm[32..44]);
        hk.copy_from_slice(&okm[44..76]);
        (key, cn, hk)
    }

    /// ChaCha20 (RFC 8439), counter from 0 — RustCrypto's, not ours.
    fn stream(key: &[u8; 32], nonce: &[u8; 12], data: &mut [u8]) {
        use chacha20::cipher::{KeyIvInit, StreamCipher};
        chacha20::ChaCha20::new(key.into(), nonce.into()).apply_keystream(data);
    }

    pub fn padded_len(len: usize) -> usize {
        if len <= 32 {
            return 32;
        }
        let next_power = 1usize << (usize::BITS - (len - 1).leading_zeros());
        let chunk = if next_power <= 256 { 32 } else { next_power / 8 };
        chunk * ((len - 1) / chunk + 1)
    }

    pub fn encrypt_with_nonce(msg: &str, ck: &[u8; 32], nonce: &[u8; 32]) -> String {
        let m = msg.as_bytes();
        let mut padded = Vec::with_capacity(2 + padded_len(m.len()));
        padded.extend_from_slice(&(m.len() as u16).to_be_bytes());
        padded.extend_from_slice(m);
        padded.resize(2 + padded_len(m.len()), 0);
        let (key, cn, hk) = message_keys(ck, nonce);
        stream(&key, &cn, &mut padded);
        let mac = hmac(&hk, &[nonce, &padded]);
        let mut out = vec![2u8];
        out.extend_from_slice(nonce);
        out.extend_from_slice(&padded);
        out.extend_from_slice(&mac);
        STANDARD.encode(out)
    }

    pub fn encrypt(msg: &str, ck: &[u8; 32]) -> String {
        let mut nonce = [0u8; 32];
        getrandom::getrandom(&mut nonce).expect("random nonce");
        encrypt_with_nonce(msg, ck, &nonce)
    }

    pub fn decrypt(payload: &str, ck: &[u8; 32]) -> Result<String, String> {
        let raw = STANDARD.decode(payload).map_err(|_| "not base64".to_string())?;
        if raw.len() < 99 || raw[0] != 2 {
            return Err("not a nip44 v2 payload".into());
        }
        let nonce: [u8; 32] = raw[1..33].try_into().unwrap();
        let (ct, mac) = raw[33..].split_at(raw.len() - 33 - 32);
        let (key, cn, hk) = message_keys(ck, &nonce);
        let mut m = HmacSha256::new_from_slice(&hk).expect("hmac key");
        m.update(&nonce);
        m.update(ct);
        m.verify_slice(mac).map_err(|_| "bad mac".to_string())?;
        let mut padded = ct.to_vec();
        stream(&key, &cn, &mut padded);
        let len = u16::from_be_bytes([padded[0], padded[1]]) as usize;
        if len == 0 || 2 + len > padded.len() || padded.len() != 2 + padded_len(len) {
            return Err("bad padding".into());
        }
        String::from_utf8(padded[2..2 + len].to_vec()).map_err(|_| "not utf-8".into())
    }
}

/// #4657 — where the signer connects. `CHORUS_SIGNER_RELAY` is the relay's own
/// address, the one every AUTH names and the policy checks; once that is the
/// public wss:// address (which this build cannot dial: no TLS), the daemon still
/// connects on the LAN through `CHORUS_SIGNER_DIAL`. Unset or blank: the relay address.
pub fn dial_address(relay: &str, dial: Option<&str>) -> String {
    match dial.map(str::trim) {
        Some(d) if !d.is_empty() => d.to_string(),
        _ => relay.to_string(),
    }
}

/// #4737 — where the socket connects (host:port of the dial address) and the
/// URL its handshake names. The relay virtual-hosts by Host header: after the
/// public-name cutover it answers only to its public name, so the handshake
/// names the relay (wss → ws: this build speaks no TLS, the bytes ride the LAN)
/// while the TCP connection goes to the dial address.
pub fn handshake(relay: &str, dial: Option<&str>) -> (String, String) {
    let target = dial_address(relay, dial);
    let hostport = |u: &str| {
        let rest = u.split_once("://").map(|(_, r)| r).unwrap_or(u);
        let hp = rest.split('/').next().unwrap_or(rest).to_string();
        if hp.contains(':') { hp } else if u.starts_with("wss://") { format!("{hp}:443") } else { format!("{hp}:80") }
    };
    let url = match relay.strip_prefix("wss://") { Some(r) => format!("ws://{r}"), None => relay.to_string() };
    (hostport(&target), url)
}

/// What Jeff's key will sign.
pub struct Policy {
    pub channel_h: String,
    pub relay: String,
}

impl Policy {
    pub fn allows(&self, ev: &Value) -> Result<(), String> {
        let kind = ev["kind"].as_i64().ok_or("refused: event has no kind")?;
        let tags = ev["tags"].as_array().cloned().unwrap_or_default();
        let values = |name: &str| -> Vec<String> {
            tags.iter()
                .filter(|t| t[0] == name)
                .filter_map(|t| t[1].as_str().map(str::to_string))
                .collect()
        };
        match kind {
            9 => {
                let h = values("h");
                if h.len() == 1 && h[0] == self.channel_h {
                    Ok(())
                } else {
                    Err(format!("refused: kind 9 must carry exactly one h tag, #clearing; got {h:?}"))
                }
            }
            22242 => {
                let relay = values("relay");
                if relay.len() == 1 && relay[0] == self.relay {
                    Ok(())
                } else {
                    Err(format!("refused: kind 22242 only for {}; got {relay:?}", self.relay))
                }
            }
            k => Err(format!("refused: kind {k} is not signed by Jeff's signer")),
        }
    }
}

pub struct Signer {
    keypair: Keypair,
    sk: [u8; 32],
    pub pubkey: String,
    clients: Vec<String>,
    pub policy: Policy,
}

impl Signer {
    pub fn new(sk_hex: &str, clients: Vec<String>, policy: Policy) -> Result<Self, String> {
        let bytes = hex::decode(sk_hex.trim()).map_err(|e| e.to_string())?;
        let sk = SecretKey::from_slice(&bytes).map_err(|e| e.to_string())?;
        let keypair = Keypair::from_secret_key(SECP256K1, &sk);
        let pubkey = hex::encode(keypair.x_only_public_key().0.serialize());
        let clients = clients
            .into_iter()
            .map(|c| c.trim().to_ascii_lowercase())
            .filter(|c| !c.is_empty())
            .collect();
        Ok(Signer { keypair, sk: bytes.try_into().unwrap(), pubkey, clients, policy })
    }

    /// Sign a complete event as Jeff. Callers check the policy first.
    fn finish(&self, created_at: i64, kind: i64, tags: Value, content: &str) -> Value {
        let id = event_id(&self.pubkey, created_at, kind, &tags, content);
        let msg = Message::from_digest_slice(&hex::decode(&id).unwrap()).unwrap();
        let mut aux = [0u8; 32];
        getrandom::getrandom(&mut aux).expect("random");
        let sig = SECP256K1.sign_schnorr_with_aux_rand(&msg, &self.keypair, &aux);
        json!({"id": id, "pubkey": self.pubkey, "created_at": created_at, "kind": kind,
               "tags": tags, "content": content, "sig": hex::encode(sig.as_ref())})
    }

    /// The AUTH this signer sends for its own connection to our relay.
    pub fn auth_event(&self, challenge: &str, now: i64) -> Value {
        self.finish(now, 22242, json!([["relay", self.policy.relay], ["challenge", challenge]]), "")
    }

    fn sign_event(&self, raw: &str) -> Result<Value, String> {
        let ev: Value =
            serde_json::from_str(raw).map_err(|_| "refused: sign_event wants one event as JSON".to_string())?;
        self.policy.allows(&ev)?;
        let created_at = ev["created_at"].as_i64().ok_or("refused: event has no created_at")?;
        let content = ev["content"].as_str().unwrap_or("");
        Ok(self.finish(created_at, ev["kind"].as_i64().unwrap(), ev["tags"].clone(), content))
    }

    /// One kind 24133 request in, one signed reply out — or nothing, when the
    /// request is not for this signer or comes from a client it does not know.
    pub fn handle(&self, req: &Value) -> Option<Value> {
        let from = req["pubkey"].as_str()?.to_ascii_lowercase();
        let to_me = req["tags"].as_array()?.iter().any(|t| t[0] == "p" && t[1] == self.pubkey.as_str());
        if req["kind"] != 24133 || !to_me || from.len() != 64 {
            return None;
        }
        if !self.clients.contains(&from) {
            eprintln!("chorus-jeff-signer refused: unknown client {}", &from[..8]);
            return None;
        }
        let ck = nip44::conversation_key(&self.sk, &from).ok()?;
        let body: Value = serde_json::from_str(&nip44::decrypt(req["content"].as_str()?, &ck).ok()?).ok()?;
        let method = body["method"].as_str().unwrap_or("");
        let outcome: Result<String, String> = match method {
            "connect" => Ok("ack".into()),
            "ping" => Ok("pong".into()),
            "get_public_key" => Ok(self.pubkey.clone()),
            "sign_event" => body["params"][0]
                .as_str()
                .ok_or_else(|| "refused: sign_event wants one event as JSON".to_string())
                .and_then(|raw| self.sign_event(raw))
                .map(|ev| ev.to_string()),
            m => Err(format!("refused: method {m} not allowed")),
        };
        eprintln!(
            "chorus-jeff-signer {method} {} for {}",
            if outcome.is_ok() { "ok" } else { "refused" },
            &from[..8]
        );
        let reply = match outcome {
            Ok(result) => json!({"id": body["id"], "result": result}),
            Err(error) => json!({"id": body["id"], "error": error}),
        };
        Some(self.finish(unix_now(), 24133, json!([["p", from]]), &nip44::encrypt(&reply.to_string(), &ck)))
    }
}

pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
