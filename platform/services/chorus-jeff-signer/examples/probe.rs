//! #4652 live probe: act as Clearing v2 and ask the running signer to sign.
//! cargo run --example probe -- <relay> <client_sec_hex> <signer_pub_hex> <kind> <h-or-relay-tag>
use chorus_jeff_signer::{event_id, nip44, unix_now};
use secp256k1::{Keypair, Message, SecretKey, SECP256K1};
use serde_json::{json, Value};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (relay, csec, spub, kind, tagv) = (&a[1], &a[2], &a[3], a[4].parse::<i64>().unwrap(), &a[5]);
    let kp = Keypair::from_secret_key(SECP256K1, &SecretKey::from_slice(&hex::decode(csec).unwrap()).unwrap());
    let cpub = hex::encode(kp.x_only_public_key().0.serialize());
    let sign = |kind: i64, tags: Value, content: &str| {
        let now = unix_now();
        let id = event_id(&cpub, now, kind, &tags, content);
        let sig = SECP256K1.sign_schnorr_no_aux_rand(&Message::from_digest_slice(&hex::decode(&id).unwrap()).unwrap(), &kp);
        json!({"id": id, "pubkey": cpub, "created_at": now, "kind": kind, "tags": tags, "content": content, "sig": hex::encode(sig.as_ref())})
    };
    let ck = nip44::conversation_key(&hex::decode(csec).unwrap(), spub).unwrap();
    let tags = if kind == 22242 { json!([["relay", tagv], ["challenge", "x"]]) } else { json!([["h", tagv]]) };
    let unsigned = json!({"kind": kind, "created_at": unix_now(), "tags": tags, "content": "probe"}).to_string();
    let body = json!({"id": "p1", "method": "sign_event", "params": [unsigned]}).to_string();
    let req = sign(24133, json!([["p", spub]]), &nip44::encrypt(&body, &ck));
    let (mut ws, _) = tungstenite::connect(relay.as_str()).unwrap();
    let start = std::time::Instant::now();
    let mut sent = false;
    while start.elapsed().as_secs() < 15 {
        let Ok(tungstenite::Message::Text(t)) = ws.read() else { continue };
        let f: Value = serde_json::from_str(&t).unwrap();
        match f[0].as_str() {
            Some("AUTH") => { ws.send(tungstenite::Message::text(json!(["AUTH", sign(22242, json!([["relay", relay], ["challenge", f[1]]]), "")]).to_string())).unwrap(); }
            Some("OK") if f[3].as_str().is_none_or(|m| m.is_empty()) && !sent && f[2] == true && f[1] != req["id"] => { ws.send(tungstenite::Message::text(json!(["REQ", "probe", {"kinds": [24133], "#p": [cpub], "since": unix_now() - 5}]).to_string())).unwrap(); }
            Some("OK") if f[1] == req["id"] => { if f[2] != true { println!("RELAY REFUSED REQUEST {}", f[3]); return; } }
            Some("EOSE") if !sent => { ws.send(tungstenite::Message::text(json!(["EVENT", req]).to_string())).unwrap(); sent = true; }
            Some("EVENT") if f[2]["pubkey"] == spub.as_str() => {
                let r: Value = serde_json::from_str(&nip44::decrypt(f[2]["content"].as_str().unwrap(), &ck).unwrap()).unwrap();
                println!("REPLY {}", r); return;
            }
            _ => {}
        }
    }
    println!("NO REPLY in 15 s"); std::process::exit(1);
}
