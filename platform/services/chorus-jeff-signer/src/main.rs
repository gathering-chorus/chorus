//! #4652 — chorus-jeff-signer: the process. Reads Jeff's key from a file only
//! its own user can read, keeps one connection to our relay, and answers the
//! NIP-46 requests the library says it may.
//!
//! Configuration comes from the LaunchDaemon's environment (names only here):
//!   CHORUS_SIGNER_KEY_FILE  file holding Jeff's key, hex or nsec, mode 0600
//!   CHORUS_SIGNER_RELAY     our relay's own address (what each AUTH names)
//!   CHORUS_SIGNER_DIAL      optional: where to connect when that differs (#4657)
//!   CHORUS_SIGNER_CLIENTS   comma-separated client pubkeys (Clearing v2's)
//!   CHORUS_SIGNER_CHANNEL_H the #clearing channel id (the only channel it signs for)
use chorus_jeff_signer::{dial_address, handshake, nip19_secret, unix_now, Policy, Signer};
use serde_json::{json, Value};
use std::os::unix::fs::MetadataExt;
use std::time::Duration;
use tungstenite::Message;

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| fail(&format!("{name} is not set")))
}

fn fail(why: &str) -> ! {
    eprintln!("chorus-jeff-signer: {why}");
    std::process::exit(2)
}

/// The key file must belong to this user and be unreadable by anyone else; a
/// looser file means the wall is already down, so refuse to start.
fn read_key(path: &str) -> String {
    let meta = std::fs::metadata(path).unwrap_or_else(|e| fail(&format!("key file {path}: {e}")));
    if meta.mode() & 0o077 != 0 {
        fail(&format!("key file {path} is mode {:o}; it must be 0600", meta.mode() & 0o777));
    }
    if meta.uid() != unsafe_getuid() {
        fail(&format!("key file {path} belongs to uid {}, not this user", meta.uid()));
    }
    let raw = std::fs::read_to_string(path).unwrap_or_else(|e| fail(&format!("key file {path}: {e}")));
    nip19_secret(&raw).unwrap_or_else(|e| fail(&format!("key file {path}: {e}")))
}

fn unsafe_getuid() -> u32 {
    extern "C" {
        fn getuid() -> u32;
    }
    unsafe { getuid() }
}

fn serve(signer: &Signer, dial: &str) -> Result<(), String> {
    // #4737: connect to the dial address, but name the relay in the handshake (its Host)
    let (addr, url) = handshake(&signer.policy.relay, Some(dial));
    let stream = std::net::TcpStream::connect(&addr).map_err(|e| format!("connect {addr}: {e}"))?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let (mut ws, _) = tungstenite::client(url.as_str(), stream).map_err(|e| format!("handshake: {e}"))?;
    let sub = || {
        Message::text(json!(["REQ", "jeff-signer", {"kinds": [24133], "#p": [signer.pubkey], "since": unix_now() - 10}]).to_string())
    };
    // The relay closes a REQ sent before AUTH, so subscribe once AUTH is
    // accepted — or after a quiet read, for a relay that never asks.
    eprintln!("chorus-jeff-signer: connected to {dial} (AUTH names {}) as {}", signer.policy.relay, &signer.pubkey[..8]);
    let mut auth_id = String::new();
    let mut subscribed = false;
    loop {
        let msg = match ws.read() {
            Ok(m) => m,
            Err(tungstenite::Error::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
                ws.send(Message::Ping(Vec::new())).map_err(|e| e.to_string())?;
                if !subscribed {
                    ws.send(sub()).map_err(|e| e.to_string())?;
                    subscribed = true;
                }
                continue;
            }
            Err(e) => return Err(format!("read: {e}")),
        };
        let Message::Text(text) = msg else { continue };
        let Ok(frame) = serde_json::from_str::<Value>(&text) else { continue };
        match frame[0].as_str() {
            Some("AUTH") => {
                let ev = signer.auth_event(frame[1].as_str().unwrap_or(""), unix_now());
                auth_id = ev["id"].as_str().unwrap_or("").to_string();
                ws.send(Message::text(json!(["AUTH", ev]).to_string())).map_err(|e| e.to_string())?;
            }
            Some("OK") if frame[1] == auth_id.as_str() => {
                if frame[2] != true {
                    return Err(format!("relay refused AUTH: {}", frame[3]));
                }
                ws.send(sub()).map_err(|e| e.to_string())?;
                subscribed = true;
                eprintln!("chorus-jeff-signer: authenticated, listening for requests");
            }
            Some("EVENT") => {
                if let Some(reply) = signer.handle(&frame[2]) {
                    ws.send(Message::text(json!(["EVENT", reply]).to_string())).map_err(|e| e.to_string())?;
                }
            }
            Some("CLOSED") | Some("NOTICE") => eprintln!("chorus-jeff-signer: relay says {text}"),
            _ => {}
        }
    }
}

fn main() {
    let key = read_key(&env("CHORUS_SIGNER_KEY_FILE"));
    let clients: Vec<String> = env("CHORUS_SIGNER_CLIENTS").split(',').map(str::to_string).collect();
    let policy = Policy { channel_h: env("CHORUS_SIGNER_CHANNEL_H"), relay: env("CHORUS_SIGNER_RELAY") };
    let dial = dial_address(&policy.relay, std::env::var("CHORUS_SIGNER_DIAL").ok().as_deref());
    let signer = Signer::new(&key, clients, policy).unwrap_or_else(|e| fail(&e));
    drop(key);
    let mut wait = 1;
    loop {
        let start = std::time::Instant::now();
        if let Err(e) = serve(&signer, &dial) {
            eprintln!("chorus-jeff-signer: {e}; reconnecting in {wait}s");
        }
        // A connection that held for a minute was a good one: start the backoff over.
        wait = if start.elapsed() > Duration::from_secs(60) { 1 } else { wait };
        std::thread::sleep(Duration::from_secs(wait));
        wait = (wait * 2).min(30);
    }
}
