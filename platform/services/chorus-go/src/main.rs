//! chorus-go — #4710. `serve` runs the go page behind the tunnel
//! (lightlifeurbangardens.com/go); `register-link` makes the one-time link that
//! registers Jeff's phone; `check <card> <round>` is what the land asks.
use chorus_go::*;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;

const PAGE: &str = include_str!("page.html");

#[allow(dead_code)]
mod service_lifecycle {
    include!("../../shared/service_lifecycle.rs");
}

/// launchd's label; the prod name when run by hand.
fn service_label() -> String {
    service_lifecycle::launchd_label().unwrap_or_else(|| "com.chorus.go".to_string())
}

/// #4446 — every service logs its lifecycle, through chorus-log like athena-make.
fn emit_lifecycle((event, fields): service_lifecycle::LifecycleEvent) {
    let mut args = vec![format!("{}/platform/scripts/chorus-log", chorus_home()), event.to_string(), "system".to_string()];
    args.extend(fields.into_iter().map(|(k, v)| format!("{k}={v}")));
    if event == "service.failed" {
        args.push("--level=error".to_string());
    }
    let _ = Command::new("bash").args(&args).output();
}

fn env_or(k: &str, d: &str) -> String {
    std::env::var(k).ok().filter(|s| !s.is_empty()).unwrap_or_else(|| d.to_string())
}
fn rp_id() -> String { env_or("CHORUS_GO_RP_ID", "lightlifeurbangardens.com") }
fn origin() -> String { env_or("CHORUS_GO_ORIGIN", "https://lightlifeurbangardens.com") }
fn chorus_home() -> String { env_or("CHORUS_HOME", &format!("{}/CascadeProjects/chorus", env_or("HOME", ""))) }
fn werk_base() -> String { env_or("CHORUS_WERK_BASE", &format!("{}/CascadeProjects/chorus-werk", env_or("HOME", ""))) }

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let store = match Store::open(&Store::default_dir()) {
        Ok(s) => s,
        Err(e) => { eprintln!("chorus-go: {e}"); std::process::exit(1) }
    };
    let code = match args.first().map(String::as_str) {
        Some("serve") => { serve(&store); 0 }
        Some("register-link") => match store.mint_register_token() {
            Ok(t) => { println!("{}/go/register?token={t}", origin()); 0 }
            Err(e) => { eprintln!("chorus-go: {e}"); 1 }
        },
        Some("check") => {
            let card = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            let round = args.get(2).cloned().unwrap_or_default();
            match land_check(&store, card, &round) {
                Ok(LandGo::Signed) => { println!("chorus-go: #{card} round {} — go signed by Jeff's passkey", short(&round)); 0 }
                Ok(LandGo::NoPasskey) => { println!("chorus-go: no passkey registered — the go is not signed yet (#4710)"); 0 }
                Err(e) => { eprintln!("chorus-go: refused: {e}"); 1 }
            }
        }
        _ => { eprintln!("usage: chorus-go serve | register-link | check <card> <round>"); 2 }
    };
    std::process::exit(code);
}

fn serve(store: &Store) {
    let addr = env_or("CHORUS_GO_ADDR", "127.0.0.1:3490");
    let listener = TcpListener::bind(&addr).unwrap_or_else(|e| {
        let why = format!("bind {addr}: {e}");
        eprintln!("chorus-go: {why}");
        emit_lifecycle(service_lifecycle::failed_event(&service_label(), std::process::id(), &why, 1));
        std::process::exit(1)
    });
    let label = service_label();
    let previous = service_lifecycle::previous_run(&label);
    for e in service_lifecycle::start_events(&label, std::process::id(), &service_lifecycle::binary_version(), previous.as_ref()) {
        emit_lifecycle(e);
    }
    eprintln!("chorus-go: serving {addr} for {}", origin());
    // one thread per connection: a phone waiting on Face ID never holds up the next request
    std::thread::scope(|scope| {
        for stream in listener.incoming().flatten() {
            scope.spawn(move || if let Err(e) = handle(store, stream) { eprintln!("chorus-go: {e}"); });
        }
    });
}

struct Req { method: String, path: String, query: String, body: Vec<u8> }

fn read_req(s: &mut TcpStream) -> Result<Req, String> {
    s.set_read_timeout(Some(std::time::Duration::from_secs(10))).ok();
    let mut r = BufReader::new(s.try_clone().map_err(|e| e.to_string())?);
    let mut line = String::new();
    r.read_line(&mut line).map_err(|e| e.to_string())?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("/").to_string();
    let mut len = 0usize;
    loop {
        let mut h = String::new();
        if r.read_line(&mut h).map_err(|e| e.to_string())? == 0 || h == "\r\n" || h == "\n" { break; }
        if let Some((k, v)) = h.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") { len = v.trim().parse().unwrap_or(0); }
        }
    }
    if len > 64 * 1024 { return Err("body too large".into()); }
    let mut body = vec![0; len];
    r.read_exact(&mut body).map_err(|e| e.to_string())?;
    let (path, query) = target.split_once('?').map(|(a, b)| (a.to_string(), b.to_string())).unwrap_or((target, String::new()));
    Ok(Req { method, path, query, body })
}

fn param(q: &str, k: &str) -> String {
    q.split('&').filter_map(|kv| kv.split_once('=')).find(|(a, _)| *a == k).map(|(_, v)| v.to_string()).unwrap_or_default()
}

fn reply(s: &mut TcpStream, code: u16, ctype: &str, body: &str) -> Result<(), String> {
    let reason = match code { 200 => "OK", 400 => "Bad Request", 403 => "Forbidden", 404 => "Not Found", _ => "Error" };
    write!(s, "HTTP/1.1 {code} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}", body.len())
        .map_err(|e| e.to_string())
}

fn handle(store: &Store, mut s: TcpStream) -> Result<(), String> {
    let req = read_req(&mut s)?;
    let path = req.path.trim_end_matches('/');
    let json_body = || serde_json::from_slice::<Value>(&req.body).unwrap_or(Value::Null);
    let field = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let out = match (req.method.as_str(), path) {
        ("GET", "/go") | ("GET", "/go/register") => return reply(&mut s, 200, "text/html; charset=utf-8", PAGE),
        ("GET", "/go/api/info") => info(store, &param(&req.query, "card"), &param(&req.query, "run"), &param(&req.query, "role")),
        ("GET", "/go/api/register-options") => Ok(json!({
            "challenge": b64url(&register_challenge(&param(&req.query, "token"))), "rpId": rp_id(), "registered": store.passkey().ok().flatten().is_some()})),
        ("POST", "/go/api/register") => {
            let v = json_body();
            register(store, &field(&v, "token"), &field(&v, "credId"), &field(&v, "publicKey"),
                &unb64(&field(&v, "clientDataJSON")).unwrap_or_default(), &rp_id(), &origin())
                .map(|_| json!({"ok": true, "text": "Your phone is registered. From now on only a go it signs lands a card."}))
        }
        ("POST", "/go/api/decide") => {
            let v = json_body();
            accept(store, &field(&v, "nonce"), &field(&v, "decision"), &field(&v, "authData"), &field(&v, "clientDataJSON"), &field(&v, "signature"))
                .and_then(|rec| complete_dagu(&rec).map(|t| json!({"ok": true, "text": t})))
        }
        _ => return reply(&mut s, 404, "text/plain", "not found"),
    };
    match out {
        Ok(v) => reply(&mut s, 200, "application/json", &v.to_string()),
        Err(e) => reply(&mut s, 403, "application/json", &json!({"ok": false, "text": e}).to_string()),
    }
}

/// The round is the werk's HEAD: the commit the demo showed.
fn round_of(role: &str, card: u64) -> Result<String, String> {
    let werk = format!("{}/{role}-{card}", werk_base());
    chorus_go::werk_head(&werk).map_err(|_| format!("no werk for #{card} ({role})"))
}

fn title_of(card: u64) -> String {
    Command::new(format!("{}/platform/scripts/cards", chorus_home())).args(["view", &card.to_string()]).output().ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).lines().next().map(str::to_string))
        .unwrap_or_else(|| format!("#{card}"))
}

fn info(store: &Store, card: &str, run: &str, role: &str) -> Result<Value, String> {
    let card: u64 = card.parse().map_err(|_| "no card in the link".to_string())?;
    if !role.bytes().all(|b| b.is_ascii_lowercase() || b == b'-') || role.is_empty() { return Err("no role in the link".into()); }
    let pk = store.passkey()?.ok_or("no passkey registered yet: run `chorus-go register-link` on the Library")?;
    // #4753 — one go per card: a superseded or answered run is refused before Jeff signs
    if run.is_empty() || !run.bytes().all(|b| b.is_ascii_alphanumeric()) { return Err("no run in the link".into()); }
    let (dag, id) = go_run(run);
    go_run_open(&run_status(dag, &id))?;
    let round = round_of(role, card)?;
    let nonce = store.mint_challenge(card, run, &round)?;
    Ok(json!({
        "card": card, "run": run, "round": round, "title": title_of(card), "nonce": nonce, "rpId": pk.rp_id, "credId": pk.cred_id,
        "challengeGo": b64url(&go_challenge(card, run, &format!("{round}|go"), &nonce)),
        "challengeNo": b64url(&go_challenge(card, run, &format!("{round}|no"), &nonce)),
    }))
}

/// #4758 — the dag and run that hold the go of the link's run (lib go_target).
fn go_run(run: &str) -> (&'static str, String) {
    go_target(run, &run_status("cicd-go", &format!("go-{run}")))
}

/// The first line of `dagu status <dag> --run-id <run>`; empty when dagu cannot say.
fn run_status(dag: &str, run: &str) -> String {
    let dagu = env_or("CHORUS_GO_DAGU", "/opt/homebrew/bin/dagu");
    let cfg = format!("{}/platform/config/dagu.yaml", chorus_home());
    Command::new(&dagu)
        .args(["--config", &cfg, "status", dag, "--run-id", run])
        .env_remove("CHORUS_ROLE").env_remove("ROLE")
        .output()
        .ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).lines().next().map(str::to_string))
        .unwrap_or_default()
}

/// Answer the run's go step with what Jeff's phone signed.
fn complete_dagu(rec: &GoRecord) -> Result<String, String> {
    let dagu = env_or("CHORUS_GO_DAGU", "/opt/homebrew/bin/dagu");
    let cfg = format!("{}/platform/config/dagu.yaml", chorus_home());
    let (dag, id) = go_run(&rec.run);
    let o = Command::new(&dagu)
        .args(["--config", &cfg, "human-task", "complete", dag, "--run-id", &id, "--step", "skill_werk_go",
               "--input", "accepter=jeff", "--input", &format!("decision={}", rec.decision)])
        .env_remove("CHORUS_ROLE").env_remove("ROLE")
        .output().map_err(|e| format!("dagu: {e}"))?;
    if !o.status.success() {
        return Err(format!("signed, but the run did not take it: {}", String::from_utf8_lossy(&o.stderr).trim()));
    }
    Ok(if rec.decision == "go" { format!("Go signed for #{}. It's landing.", rec.card) } else { format!("No signed for #{}. It's held.", rec.card) })
}
