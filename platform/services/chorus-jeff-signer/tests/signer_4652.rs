// @test-type: unit — hermetic: crypto vectors, the signing policy and the request handler; no network.
// @domain: security
// @card: #4652
// @owner: silas
//! #4652 reopen 2 — Jeff's remote signer (Jeff picked A2 at 11:53, a separate
//! macOS user at 11:59). The signer holds Jeff's key and answers NIP-46 requests
//! from Clearing v2 only, and it signs only two things: a kind 9 in #team, and a
//! kind 22242 AUTH for our relay. Everything else is refused, and a request from
//! a client it does not know gets no answer at all.
use chorus_jeff_signer::{event_id, nip19_secret, nip44, Policy, Signer};
use secp256k1::{schnorr::Signature, Message, XOnlyPublicKey, SECP256K1};
use serde_json::{json, Value};

const SEC1: &str = "0000000000000000000000000000000000000000000000000000000000000001";
const SEC2: &str = "0000000000000000000000000000000000000000000000000000000000000002";
/// #clearing — the only channel Jeff's key signs for (Jeff 2026-10-20 13:01).
const CLEARING: &str = "03e2b251-240f-47fe-89b5-fe4e7ac648fb";
/// #team — nudges, alerts and lands; never signed as Jeff.
const TEAM: &str = "160e1f5d-6816-4c68-94b3-b5f1ade75565";
const RELAY: &str = "ws://192.168.86.242:3000";

fn pubkey(sec: &str) -> String {
    let sk = secp256k1::SecretKey::from_slice(&hex::decode(sec).unwrap()).unwrap();
    hex::encode(sk.x_only_public_key(SECP256K1).0.serialize())
}

fn policy() -> Policy {
    Policy { channel_h: CLEARING.into(), relay: RELAY.into() }
}

/// Jeff's signer is SEC1; Clearing v2's client key is SEC2.
fn signer() -> Signer {
    Signer::new(SEC1, vec![pubkey(SEC2)], policy()).unwrap()
}

fn verifies(ev: &Value) -> bool {
    let id = event_id(
        ev["pubkey"].as_str().unwrap(),
        ev["created_at"].as_i64().unwrap(),
        ev["kind"].as_i64().unwrap(),
        &ev["tags"],
        ev["content"].as_str().unwrap(),
    );
    if id != ev["id"].as_str().unwrap() {
        return false;
    }
    let sig = Signature::from_slice(&hex::decode(ev["sig"].as_str().unwrap()).unwrap()).unwrap();
    let pk = XOnlyPublicKey::from_slice(&hex::decode(ev["pubkey"].as_str().unwrap()).unwrap()).unwrap();
    let msg = Message::from_digest_slice(&hex::decode(&id).unwrap()).unwrap();
    SECP256K1.verify_schnorr(&sig, &msg, &pk).is_ok()
}

/// A NIP-46 request from `from_sec` to Jeff's signer, as Clearing v2 would send it.
fn request(from_sec: &str, body: Value) -> Value {
    let ck = nip44::conversation_key(&hex::decode(from_sec).unwrap(), &pubkey(SEC1)).unwrap();
    json!({
        "kind": 24133, "pubkey": pubkey(from_sec), "created_at": 1760000000,
        "tags": [["p", pubkey(SEC1)]], "content": nip44::encrypt(&body.to_string(), &ck),
    })
}

/// Clearing v2's view of the signer's reply.
fn reply_body(reply: &Value) -> Value {
    assert_eq!(reply["kind"], 24133);
    assert!(verifies(reply), "the reply itself must be signed by Jeff's key");
    let ck = nip44::conversation_key(&hex::decode(SEC2).unwrap(), &pubkey(SEC1)).unwrap();
    serde_json::from_str(&nip44::decrypt(reply["content"].as_str().unwrap(), &ck).unwrap()).unwrap()
}

fn unsigned(kind: i64, tags: Value) -> String {
    json!({"kind": kind, "created_at": 1760000000, "tags": tags, "content": "hello"}).to_string()
}

// ---------- NIP-44 v2, against the spec vector Wren's client also passes ----------

#[test]
fn nip44_the_spec_vector() {
    let ck = nip44::conversation_key(&hex::decode(SEC1).unwrap(), &pubkey(SEC2)).unwrap();
    assert_eq!(hex::encode(ck), "c41c775356fd92eadc63ff5a0dc1da211b268cbea22316767095b2871ea1412d");
    let mut nonce = [0u8; 32];
    nonce[31] = 1;
    assert_eq!(
        nip44::encrypt_with_nonce("a", &ck, &nonce),
        "AgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABee0G5VSK0/9YypIObAtDKfYEAjD35uVkHyB0F4DwrcNaCXlCWZKaArsGrY6M9wnuTMxWfp1RTN9Xga8no+kF5Vsb"
    );
}

#[test]
fn nip44_both_sides_agree_and_a_long_message_round_trips() {
    let a = nip44::conversation_key(&hex::decode(SEC1).unwrap(), &pubkey(SEC2)).unwrap();
    let b = nip44::conversation_key(&hex::decode(SEC2).unwrap(), &pubkey(SEC1)).unwrap();
    assert_eq!(a, b);
    let msg = "x".repeat(1000);
    assert_eq!(nip44::decrypt(&nip44::encrypt(&msg, &a), &b).unwrap(), msg);
}

// NEGATIVE PROOF: one flipped bit in the ciphertext is refused, never decrypted.
#[test]
fn nip44_a_tampered_payload_is_refused() {
    use base64::Engine;
    let ck = nip44::conversation_key(&hex::decode(SEC1).unwrap(), &pubkey(SEC2)).unwrap();
    let mut raw = base64::engine::general_purpose::STANDARD.decode(nip44::encrypt("hello", &ck)).unwrap();
    raw[40] ^= 1;
    let bad = base64::engine::general_purpose::STANDARD.encode(raw);
    assert_eq!(nip44::decrypt(&bad, &ck).unwrap_err(), "bad mac");
}

/// The spec vector is one block; a sign_event request is many. This reference
/// was computed by @noble/ciphers + @noble/hashes (Wren's client libraries)
/// with the spec's conversation key and a fixed nonce: 300+ bytes, non-ASCII.
#[test]
fn nip44_a_multi_block_message_matches_the_noble_reference() {
    let ck = nip44::conversation_key(&hex::decode(SEC1).unwrap(), &pubkey(SEC2)).unwrap();
    let mut nonce = [0u8; 32];
    nonce[31] = 7;
    let msg = format!("{}é🪶", "x".repeat(300));
    let want = "AgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHpReXINExBfjWaAnIl39+JWkPyb9NGJQ0LFjSXVsP5mqFjKhvZJevgstQBYqEjO031Gpdwvl8w3pZyYgdiEwz1cXiMPTCmMJkgST4/LtbVpeK56wG8a+9HxLN5MlixRK2/03si2pMrRsnXEKHEbtpqDv2E0h+JS/kijaVRGOvMmmBa/Ec9qYPEND6aQn2BaCzN6XYvHZBwt6JCASoQeoNWAc/aAhdB3UkB8e3yEgpUPVgC1Q+Kjyrk22ypmyoBkCm5FgZXD+eQMcLBCx5ndtx+aC/HZMukizbPsOtDM3lofVTmDENqzNrG/H8Kx43+0BqARYpIlGFt0VWrgeZHOX1WJukaxTfajjHSRh2k0N7eI9Z6a6e5cpNWwIMUc0LUaIv+/V/nle7W4kAlQAb1+1VLnLFF4egptGBmKuHfYma6QwJEKBPepTlSFFmqzWN4l/wU3dYwlGkwzfug/8/5yXAkOtB";
    assert_eq!(nip44::encrypt_with_nonce(&msg, &ck, &nonce), want);
    assert_eq!(nip44::decrypt(want, &ck).unwrap(), msg);
}

#[test]
fn nip44_padding_follows_the_spec_table() {
    let got: Vec<usize> = [1, 32, 33, 64, 65, 256, 257, 1000].iter().map(|n| nip44::padded_len(*n)).collect();
    assert_eq!(got, vec![32, 32, 64, 64, 96, 256, 320, 1024]);
}

// ---------- the event id: NIP-01 serialization ----------

/// The expected id was computed by node's JSON.stringify + sha256 (pulse's own
/// libraries), with quotes, a backslash, a newline and non-ASCII in the content.
#[test]
fn event_id_matches_the_nip01_serialization() {
    let id = event_id(
        "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
        1760000000,
        9,
        &json!([["h", TEAM], ["t", "x\"y"]]),
        "line1\nline2 \"q\" \\ é 🪶",
    );
    assert_eq!(id, "ffc010f902cc41da6b54afa70e74e470ed0cddafdd6c35f777671b7fbbe128c9");
}

#[test]
fn the_key_file_takes_hex_or_nsec() {
    // NIP-19's own example pair.
    assert_eq!(
        nip19_secret("nsec1vl029mgpspedva04g90vltkh6fvh240zqtv9k0t9af8935ke9laqsnlfe5").unwrap(),
        "67dea2ed018072d675f5415ecfaed7d2597555e202d85b3d65ea4e58d2d92ffa"
    );
    assert_eq!(nip19_secret(&format!(" {SEC1}\n")).unwrap(), SEC1);
    assert!(nip19_secret("npub1vl029mgpspedva04g90vltkh6fvh240zqtv9k0t9af8935ke9laqsnlfe5").is_err());
    assert!(nip19_secret("nsec1vl029mgpspedva04g90vltkh6fvh240zqtv9k0t9af8935ke9laqsnlfe6").is_err(), "a bad checksum is refused");
}

// ---------- the policy: what Jeff's key will sign ----------

#[test]
fn policy_signs_a_kind_9_in_clearing() {
    assert!(policy().allows(&json!({"kind": 9, "tags": [["h", CLEARING], ["t", "silas"]]})).is_ok());
}

#[test]
fn policy_signs_an_auth_for_our_relay() {
    assert!(policy().allows(&json!({"kind": 22242, "tags": [["relay", RELAY], ["challenge", "abc"]]})).is_ok());
}

// NEGATIVE PROOF: each of these would let the room speak as Jeff somewhere he did not pick.
#[test]
fn policy_refuses_everything_else() {
    let refused = [
        json!({"kind": 9, "tags": [["h", "another-channel"]]}),
        json!({"kind": 9, "tags": [["h", TEAM]]}),
        json!({"kind": 9, "tags": []}),
        json!({"kind": 9, "tags": [["h", CLEARING], ["h", TEAM]]}),
        json!({"kind": 1, "tags": [["h", CLEARING]]}),
        json!({"kind": 0, "tags": []}),
        json!({"kind": 5, "tags": [["h", CLEARING]]}),
        json!({"kind": 24242, "tags": [["t", "get"]]}),
        json!({"kind": 22242, "tags": [["relay", "wss://some-other-relay.example"], ["challenge", "abc"]]}),
        json!({"kind": 22242, "tags": [["challenge", "abc"]]}),
    ];
    for ev in refused {
        assert!(policy().allows(&ev).is_err(), "signed what it must refuse: {ev}");
    }
}

// ---------- the handler: NIP-46 requests end to end ----------

#[test]
fn sign_event_for_clearing_returns_an_event_signed_by_jeff() {
    let s = signer();
    let reply = s.handle(&request(SEC2, json!({"id": "r1", "method": "sign_event", "params": [unsigned(9, json!([["h", CLEARING]]))]}))).unwrap();
    let body = reply_body(&reply);
    assert_eq!(body["id"], "r1");
    let signed: Value = serde_json::from_str(body["result"].as_str().unwrap()).unwrap();
    assert_eq!(signed["pubkey"], pubkey(SEC1));
    assert_eq!(signed["content"], "hello");
    assert!(verifies(&signed), "the signed event must verify: {signed}");
}

#[test]
fn sign_event_outside_the_policy_comes_back_as_an_error_with_the_reason() {
    let s = signer();
    let reply = s.handle(&request(SEC2, json!({"id": "r2", "method": "sign_event", "params": [unsigned(1, json!([]))]}))).unwrap();
    let body = reply_body(&reply);
    assert_eq!(body["id"], "r2");
    assert!(body.get("result").is_none_or(|r| r.is_null() || r == ""), "no signature for a refused kind: {body}");
    assert!(body["error"].as_str().unwrap().contains("kind 1"), "the reason names the kind: {body}");
}

#[test]
fn connect_ping_and_get_public_key_answer() {
    let s = signer();
    for (method, want) in [("connect", json!("ack")), ("ping", json!("pong")), ("get_public_key", json!(pubkey(SEC1)))] {
        let body = reply_body(&s.handle(&request(SEC2, json!({"id": method, "method": method, "params": []}))).unwrap());
        assert_eq!(body["result"], want, "{method}");
    }
}

#[test]
fn any_other_method_is_refused() {
    let s = signer();
    for method in ["nip44_decrypt", "nip04_decrypt", "nip44_encrypt", "switch_relays"] {
        let body = reply_body(&s.handle(&request(SEC2, json!({"id": "m", "method": method, "params": []}))).unwrap());
        assert!(body["error"].as_str().unwrap().contains("not allowed"), "{method}: {body}");
    }
}

// NEGATIVE PROOF: a client key the signer was not given gets nothing, not even an error.
#[test]
fn an_unknown_client_gets_no_answer() {
    let stranger = "0000000000000000000000000000000000000000000000000000000000000003";
    let s = signer();
    assert!(s.handle(&request(stranger, json!({"id": "x", "method": "sign_event", "params": [unsigned(9, json!([["h", CLEARING]]))]}))).is_none());
}

#[test]
fn a_request_addressed_to_another_key_is_ignored() {
    let s = signer();
    let mut req = request(SEC2, json!({"id": "x", "method": "ping", "params": []}));
    req["tags"] = json!([["p", pubkey(SEC2)]]);
    assert!(s.handle(&req).is_none());
}

#[test]
fn garbage_content_is_ignored_not_a_crash() {
    let s = signer();
    let mut req = request(SEC2, json!({"id": "x", "method": "ping", "params": []}));
    req["content"] = json!("not base64 at all");
    assert!(s.handle(&req).is_none());
}

#[test]
fn the_auth_event_is_for_our_relay_only() {
    let s = signer();
    let ev = s.auth_event("challenge-1", 1760000000);
    assert_eq!(ev["kind"], 22242);
    assert_eq!(ev["tags"], json!([["relay", RELAY], ["challenge", "challenge-1"]]));
    assert!(verifies(&ev));
}
