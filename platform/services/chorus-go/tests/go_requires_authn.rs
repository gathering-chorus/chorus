// @test-type: unit — hermetic: chorus-go's handle on a loopback port, a scratch go store and a stub sign-in door answering /_auth/whoami; no tunnel, no real guard.
// @domain: security
// @card: #5039
//! #5039 AC1 — src/serve.rs: over the public tunnel (a request carrying Cf-Connecting-Ip, which
//! Cloudflare sets on every tunnelled request) every /go route needs a signed-in, allowed person
//! before it answers. The one sign-in door (chorus-share-guard) decides: chorus-go asks its
//! /_auth/whoami with the caller's cookie. Pages go to sign-in (302), /go/api/* says 401.
//! Localhost and the Wi-Fi (no Cf header) serve as before.
//! NEGATIVE PROOF: anonymous, signed-in-but-unlisted (403, no loop), a bad cookie, a non-200 door
//! answer saying allowed, and an unreachable door are
//! each refused on every route the card names, where the code before #5039 answered 200.
//! The way back that needs no go: the property row go.tunnel_signin = off serves the tunnel as
//! before; on, a different key, or no row keeps the check on.
use chorus_go::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Mutex;

static ENV: Mutex<()> = Mutex::new(());
const GOOD: &str = "chorus_share_session=good";
const UNLISTED: &str = "chorus_share_session=unlisted";
const LIAR: &str = "chorus_share_session=liar";

/// A stub of the door's /_auth/whoami: 200 allowed for GOOD, 200 not allowed for UNLISTED,
/// 401 for anything else. It records every cookie it was asked about.
fn door() -> (String, std::sync::Arc<Mutex<Vec<String>>>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap().to_string();
    let seen = std::sync::Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        for mut s in l.incoming().flatten() {
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut cookie = String::new();
            loop {
                let mut h = String::new();
                if r.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" { break; }
                if let Some(v) = h.strip_prefix("Cookie: ") { cookie = v.trim().to_string(); }
            }
            log.lock().unwrap().push(cookie.clone());
            let (code, body) = match cookie.as_str() {
                GOOD => (200, r#"{"webid": "https://id.example/jeff", "allowed": true}"#),
                UNLISTED => (200, r#"{"webid": "https://id.example/other", "allowed": false}"#),
                // a door that refuses but says allowed: only a 200 counts
                LIAR => (401, r#"{"webid": "https://id.example/jeff", "allowed": true}"#),
                _ => (401, r#"{"webid": null, "allowed": false}"#),
            };
            let _ = write!(s, "HTTP/1.1 {code} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        }
    });
    (addr, seen)
}

struct Server { addr: String, root: PathBuf, _env: std::sync::MutexGuard<'static, ()> }

fn serve(tag: &str, authn: &str) -> Server { serve_with(tag, authn, "http://127.0.0.1:9") }

fn serve_with(tag: &str, authn: &str, model_door: &str) -> Server {
    let guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let root: PathBuf = std::env::temp_dir().join(format!("chorus-go-5039-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::env::set_var("HOME", &root);
    // a chorus-log that records its arguments, so a test can see what was logged
    let log = root.join("home/platform/scripts/chorus-log");
    std::fs::create_dir_all(log.parent().unwrap()).unwrap();
    std::fs::write(&log, format!("#!/bin/sh\necho \"$@\" >> {}/spine.log\n", root.display())).unwrap();
    { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(&log, std::fs::Permissions::from_mode(0o755)).unwrap(); }
    std::env::set_var("CHORUS_HOME", root.join("home"));
    std::env::set_var("CHORUS_GO_RP_ID", "go.example.test");
    std::env::set_var("CHORUS_GO_ORIGIN", "https://go.example.test");
    std::env::set_var("CHORUS_GO_AUTHN", authn);
    std::env::set_var("CHORUS_GO_LOGIN", "https://door.example.test/_auth/login");
    // the model door: nowhere by default (the way-back row is unreadable, so the check stays on)
    std::env::set_var("CHORUS_GO_DOOR", model_door);
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap().to_string();
    let dir = root.join("go");
    std::thread::spawn(move || {
        let store = Store::open(&dir).unwrap();
        for s in l.incoming().flatten() {
            let _ = chorus_go::serve::handle(&store, s);
        }
    });
    Server { addr, root, _env: guard }
}

/// One request; `tunnel` adds the header Cloudflare sets. Returns (status, Location header).
fn ask(addr: &str, method: &str, path: &str, tunnel: bool, cookie: Option<&str>) -> (u16, String) {
    let mut s = TcpStream::connect(addr).unwrap();
    let mut head = format!("{method} {path} HTTP/1.1\r\nHost: x\r\n");
    if tunnel { head.push_str("Cf-Connecting-Ip: 203.0.113.7\r\n"); }
    if let Some(c) = cookie { head.push_str(&format!("Cookie: {c}\r\n")); }
    write!(s, "{head}Content-Length: 2\r\nConnection: close\r\n\r\n{{}}").unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let code = out.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    let loc = out.lines().find_map(|l| l.strip_prefix("Location: ")).unwrap_or("").to_string();
    (code, loc)
}

const PAGES: &[&str] = &["/go/", "/go/approvals", "/go/register"];
const API: &[(&str, &str)] = &[
    ("GET", "/go/api/approvals"),
    ("GET", "/go/api/register-options"),
    ("POST", "/go/api/approvals/challenge"),
    ("POST", "/go/api/approvals/resend"),
    ("POST", "/go/api/approvals/decide"),
    ("POST", "/go/api/register"),
];

#[test]
fn over_the_tunnel_an_anonymous_caller_is_sent_to_sign_in_and_the_api_says_401() {
    let (door_at, _) = door();
    let srv = serve("anon", &door_at);
    for p in PAGES {
        let (c, loc) = ask(&srv.addr, "GET", p, true, None);
        assert_eq!(c, 302, "GET {p}");
        assert!(loc.starts_with("https://door.example.test/_auth/login?next=https%3A%2F%2Fgo.example.test%2Fgo"), "GET {p}: {loc}");
    }
    for (m, p) in API {
        assert_eq!(ask(&srv.addr, m, p, true, None).0, 401, "{m} {p}");
    }
}

#[test]
fn negative_proof_unlisted_bad_cookie_and_an_unreachable_door_are_all_refused() {
    let (door_at, seen) = door();
    let srv = serve("neg", &door_at);
    for cookie in [LIAR, "chorus_share_session=forged"] {
        assert_eq!(ask(&srv.addr, "GET", "/go/", true, Some(cookie)).0, 302, "{cookie}");
        for (m, p) in API {
            assert_eq!(ask(&srv.addr, m, p, true, Some(cookie)).0, 401, "{cookie} {m} {p}");
        }
    }
    // signed in but unlisted: refused with 403, never sent round to sign in again (a loop)
    assert_eq!(ask(&srv.addr, "GET", "/go/", true, Some(UNLISTED)).0, 403);
    for (m, p) in API {
        assert_eq!(ask(&srv.addr, m, p, true, Some(UNLISTED)).0, 403, "unlisted {m} {p}");
    }
    // the door was really asked, with the caller's own cookie: the refusal is its answer
    assert!(seen.lock().unwrap().iter().any(|c| c == UNLISTED), "the door was never asked");
    drop(srv);
    // the door down: fail closed, even with the good cookie
    let dead = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().to_string();
    let srv = serve("dead", &dead);
    assert_eq!(ask(&srv.addr, "GET", "/go/", true, Some(GOOD)).0, 302);
    assert_eq!(ask(&srv.addr, "GET", "/go/api/register-options", true, Some(GOOD)).0, 401);
}

#[test]
fn over_the_tunnel_a_signed_in_allowed_person_is_served() {
    let (door_at, _) = door();
    let srv = serve("good", &door_at);
    for p in PAGES {
        assert_eq!(ask(&srv.addr, "GET", p, true, Some(GOOD)).0, 200, "GET {p}");
    }
    assert_eq!(ask(&srv.addr, "GET", "/go/api/register-options", true, Some(GOOD)).0, 200);
    // decide still needs the passkey: signed in alone is not a go
    assert_ne!(ask(&srv.addr, "POST", "/go/api/approvals/decide", true, Some(GOOD)).0, 200);
}

#[test]
fn localhost_and_the_wifi_serve_as_before() {
    // no Cf header: the door is never consulted (an unreachable one changes nothing)
    let dead = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().to_string();
    let srv = serve("local", &dead);
    for p in PAGES {
        assert_eq!(ask(&srv.addr, "GET", p, false, None).0, 200, "GET {p}");
    }
    assert_eq!(ask(&srv.addr, "GET", "/go/api/register-options", false, None).0, 200);
}

/// A stub model door answering one property row: (key, value), or 404 when None.
fn model_door(row: Option<(&'static str, &'static str)>) -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = format!("http://{}", l.local_addr().unwrap());
    std::thread::spawn(move || {
        for mut s in l.incoming().flatten() {
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut line = String::new();
            let _ = r.read_line(&mut line);
            loop { let mut h = String::new(); if r.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" { break; } }
            let hit = line.contains("/v1/properties/properties/prop-go-tunnel-signin");
            let (code, body) = match (row, hit) {
                (Some((k, v)), true) => (200, format!(r#"{{"data":{{"name":"prop-go-tunnel-signin","propertyKey":"{k}","propertyValue":"{v}"}}}}"#)),
                _ => (404, "{}".to_string()),
            };
            let _ = write!(s, "HTTP/1.1 {code} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        }
    });
    addr
}

#[test]
fn the_way_back_without_a_go_the_off_row_serves_the_tunnel_as_before() {
    let dead = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().to_string();
    let srv = serve_with("off", &dead, &model_door(Some(("go.tunnel_signin", "off"))));
    assert_eq!(ask(&srv.addr, "GET", "/go/", true, None).0, 200);
    assert_eq!(ask(&srv.addr, "GET", "/go/api/register-options", true, None).0, 200);
    // and it is said out loud on every such request
    let spine = std::fs::read_to_string(srv.root.join("spine.log")).unwrap_or_default();
    assert_eq!(spine.matches("go.tunnel_signin.off system").count(), 2, "{spine}");
}

#[test]
fn negative_proof_on_another_key_or_no_row_keeps_the_check_on() {
    let (door_at, _) = door();
    for (tag, row) in [("on", Some(("go.tunnel_signin", "on"))), ("key", Some(("go.other", "off"))), ("none", None)] {
        let srv = serve_with(tag, &door_at, &model_door(row));
        assert_eq!(ask(&srv.addr, "GET", "/go/", true, None).0, 302, "{tag}");
        assert_eq!(ask(&srv.addr, "GET", "/go/api/register-options", true, None).0, 401, "{tag}");
        drop(srv);
    }
}
