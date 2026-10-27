// @test-type: unit — hermetic: the real chorus-go binary on a loopback port, a scratch go store, a software P-256 key for the phone and a fake dagu that only records its arguments; no real run is ever answered.
// @domain: security
// @card: #4755
//! #4755 — src/main.rs, the process half of #4710: the subcommands, and the go
//! page's HTTP routes end to end — register a phone, open a go for a waiting
//! run, sign it, and see the go step answered as Jeff (on the fake dagu) and
//! `check` read the signed go. Each refusal on the way is shown failing.
use chorus_go::*;
use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use p256::pkcs8::EncodePublicKey;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

const RP: &str = "go.example.test";
const ORIGIN: &str = "https://go.example.test";
const RUN: &str = "034testRun4755";

struct Phone { key: SigningKey, counter: u32 }
impl Phone {
    fn new() -> Phone { Phone { key: SigningKey::random(&mut p256::elliptic_curve::rand_core::OsRng), counter: 0 } }
    fn spki(&self) -> String { b64url(self.key.verifying_key().to_public_key_der().unwrap().as_bytes()) }
    fn client_data(kind: &str, challenge: &[u8]) -> Vec<u8> {
        format!(r#"{{"type":"{kind}","challenge":"{}","origin":"{ORIGIN}","crossOrigin":false}}"#, b64url(challenge)).into_bytes()
    }
    fn sign(&mut self, challenge: &[u8]) -> (String, String, String) {
        self.counter += 1;
        let mut ad: Vec<u8> = Sha256::digest(RP.as_bytes()).to_vec();
        ad.push(0x05);
        ad.extend_from_slice(&self.counter.to_be_bytes());
        let cd = Phone::client_data("webauthn.get", challenge);
        let mut msg = ad.clone();
        msg.extend_from_slice(&Sha256::digest(&cd));
        let sig: Signature = self.key.sign(&msg);
        (b64url(&ad), b64url(&cd), b64url(sig.to_der().as_bytes()))
    }
}

/// A scratch world: go store, werk base with one werk, chorus home with a
/// `cards` that names the card, and a dagu that records and answers from files.
struct World { root: PathBuf, head: String }

fn sh(p: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn world(tag: &str) -> World {
    let root = std::env::temp_dir().join(format!("chorus-go-serve-4755-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let werk = root.join("werks/kade-4755");
    std::fs::create_dir_all(&werk).unwrap();
    std::fs::write(werk.join("f"), "x").unwrap();
    let git = |args: &[&str]| {
        let o = Command::new("git").arg("-C").arg(&werk)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid", "-c", "commit.gpgsign=false"])
            .args(args).output().unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    };
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "w"]);
    let head = git(&["rev-parse", "HEAD"]);
    sh(&root.join("home/platform/scripts/cards"), "#!/bin/sh\necho \"#4755 Coverage to 80\"\n");
    // status lines come from files the test writes; every call is logged
    sh(&root.join("bin/dagu"), &format!("#!/bin/sh\necho \"$@\" >> {r}/dagu.log\n\
        case \"$*\" in\n\
        *\"status cicd-go\"*) cat {r}/go-half 2>/dev/null ;;\n\
        *\"status cicd \"*) cat {r}/run-status 2>/dev/null ;;\n\
        *human-task*) [ -f {r}/complete-fails ] && {{ echo 'step is not waiting' >&2; exit 1; }} ;;\n\
        esac\nexit 0\n", r = root.display()));
    std::fs::write(root.join("run-status"), "Waiting - 2026-10-27 00:00:00\n").unwrap();
    World { root, head }
}

impl World {
    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_chorus-go"));
        c.env_clear()
            .args(args)
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("HOME", &self.root)
            .env("CHORUS_GO_DIR", self.root.join("go"))
            .env("CHORUS_HOME", self.root.join("home"))
            .env("CHORUS_WERK_BASE", self.root.join("werks"))
            .env("CHORUS_GO_DAGU", self.root.join("bin/dagu"))
            .env("CHORUS_GO_RP_ID", RP)
            .env("CHORUS_GO_ORIGIN", ORIGIN);
        if let Ok(p) = std::env::var("LLVM_PROFILE_FILE") {
            c.env("LLVM_PROFILE_FILE", p);
        }
        c
    }
    fn run(&self, args: &[&str]) -> (i32, String, String) {
        let o = self.cmd(args).output().unwrap();
        (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).into(), String::from_utf8_lossy(&o.stderr).into())
    }
    fn dagu_log(&self) -> String { std::fs::read_to_string(self.root.join("dagu.log")).unwrap_or_default() }
}

/// The server, stopped when the test ends (the child is this test's own).
struct Server { child: Child, addr: String }
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn serve(w: &World) -> Server {
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let addr = format!("127.0.0.1:{port}");
    // serve never returns and is killed here, so its lines are not in the
    // coverage profile (continuous mode is refused on macOS: counters section
    // not page-aligned); these are behaviour proofs, counted or not
    let child = w.cmd(&["serve"]).env("CHORUS_GO_ADDR", &addr).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    for _ in 0..100 {
        if TcpStream::connect(&addr).is_ok() { break; }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    Server { child, addr }
}

fn http(addr: &str, method: &str, path: &str, body: &str) -> (u16, String) {
    let mut s = TcpStream::connect(addr).unwrap();
    write!(s, "{method} {path} HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let code = out.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    (code, out.split_once("\r\n\r\n").map(|(_, b)| b.to_string()).unwrap_or_default())
}

fn j(body: &str) -> Value { serde_json::from_str(body).unwrap_or(Value::Null) }

/// Register the phone through the page's two calls.
fn register_phone(w: &World, srv: &Server, phone: &Phone) {
    let (code, link, err) = w.run(&["register-link"]);
    assert_eq!(code, 0, "{err}");
    let token = link.trim().rsplit("token=").next().unwrap().to_string();
    let (c, b) = http(&srv.addr, "GET", &format!("/go/api/register-options?token={token}"), "");
    assert_eq!(c, 200);
    assert_eq!(j(&b)["registered"], json!(false));
    let cd = Phone::client_data("webauthn.create", &register_challenge(&token));
    let reg = json!({"token": token, "credId": "cred-1", "publicKey": phone.spki(), "clientDataJSON": b64url(&cd)});
    let (c, b) = http(&srv.addr, "POST", "/go/api/register", &reg.to_string());
    assert_eq!(c, 200, "{b}");
    // NEGATIVE PROOF: the token is spent; a second registration is refused.
    let (c, _) = http(&srv.addr, "POST", "/go/api/register", &reg.to_string());
    assert_eq!(c, 403);
}

fn open_go(srv: &Server) -> (u16, Value) {
    let (c, b) = http(&srv.addr, "GET", &format!("/go/api/info?card=4755&run={RUN}&role=kade"), "");
    (c, j(&b))
}

fn sign_decision(phone: &mut Phone, info: &Value, decision: &str) -> String {
    let key = if decision == "go" { "challengeGo" } else { "challengeNo" };
    let ch = unb64(info[key].as_str().unwrap()).unwrap();
    let (ad, cd, sig) = phone.sign(&ch);
    json!({"nonce": info["nonce"], "decision": decision, "authData": ad, "clientDataJSON": cd, "signature": sig}).to_string()
}

#[test]
fn a_go_signed_on_the_page_answers_the_run_as_jeff_and_check_reads_it() {
    let w = world("go");
    let srv = serve(&w);
    let mut phone = Phone::new();
    let (c, page) = http(&srv.addr, "GET", "/go", "");
    assert_eq!(c, 200);
    assert!(!page.is_empty());
    assert_eq!(http(&srv.addr, "GET", "/go/nowhere", "").0, 404);

    // no passkey yet: the page cannot open a go, and check says so
    let (c, v) = open_go(&srv);
    assert_eq!(c, 403);
    assert!(v["text"].as_str().unwrap().contains("no passkey registered"), "{v}");
    let (code, out, _) = w.run(&["check", "4755", &w.head]);
    assert_eq!(code, 0);
    assert!(out.contains("no passkey registered"), "{out}");

    register_phone(&w, &srv, &phone);
    let (c, info) = open_go(&srv);
    assert_eq!(c, 200, "{info}");
    assert_eq!(info["round"], json!(w.head));
    assert_eq!(info["title"], json!("#4755 Coverage to 80"));

    // NEGATIVE PROOF: before the go, check refuses the land
    let (code, _, err) = w.run(&["check", "4755", &w.head]);
    assert_eq!(code, 1);
    assert!(err.contains("no go signed by Jeff's passkey for #4755"), "{err}");

    let (c, b) = http(&srv.addr, "POST", "/go/api/decide", &sign_decision(&mut phone, &info, "go"));
    assert_eq!(c, 200, "{b}");
    assert!(j(&b)["text"].as_str().unwrap().contains("Go signed for #4755"), "{b}");
    let log = w.dagu_log();
    assert!(log.contains(&format!("human-task complete cicd --run-id {RUN} --step skill_werk_go --input accepter=jeff --input decision=go")), "{log}");

    let (code, out, err) = w.run(&["check", "4755", &w.head]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("go signed by Jeff's passkey"), "{out}");
    // NEGATIVE PROOF: the same go does not land another round
    let (code, _, _) = w.run(&["check", "4755", "0000000000000000000000000000000000000000"]);
    assert_eq!(code, 1);
}

#[test]
fn a_no_is_answered_and_a_go_the_run_will_not_take_is_named() {
    let w = world("no");
    let srv = serve(&w);
    let mut phone = Phone::new();
    register_phone(&w, &srv, &phone);
    // #4758 — a go half exists: the go goes to cicd-go, run go-<run>
    std::fs::write(w.root.join("go-half"), "Waiting - 2026-10-27\n").unwrap();
    let (_, info) = open_go(&srv);
    let (c, b) = http(&srv.addr, "POST", "/go/api/decide", &sign_decision(&mut phone, &info, "no"));
    assert_eq!(c, 200, "{b}");
    assert!(j(&b)["text"].as_str().unwrap().contains("No signed for #4755"), "{b}");
    assert!(w.dagu_log().contains(&format!("complete cicd-go --run-id go-{RUN}")), "{}", w.dagu_log());

    std::fs::write(w.root.join("complete-fails"), "").unwrap();
    let (_, info) = open_go(&srv);
    let (c, b) = http(&srv.addr, "POST", "/go/api/decide", &sign_decision(&mut phone, &info, "go"));
    assert_eq!(c, 403);
    assert!(b.contains("signed, but the run did not take it"), "{b}");
}

#[test]
fn the_page_refuses_a_go_for_a_run_not_waiting_or_a_link_missing_its_parts() {
    let w = world("refuse");
    let srv = serve(&w);
    let phone = Phone::new();
    register_phone(&w, &srv, &phone);
    for (q, want) in [
        ("card=x&run=r&role=kade", "no card in the link"),
        ("card=4755&run=r&role=", "no role in the link"),
        ("card=4755&run=&role=kade", "no run in the link"),
        ("card=4755&run=r1&role=wren", "no werk for #4755 (wren)"),
    ] {
        let (c, b) = http(&srv.addr, "GET", &format!("/go/api/info?{q}"), "");
        assert_eq!(c, 403, "{q}");
        assert!(b.contains(want), "{q}: {b}");
    }
    for (status, want) in [("Aborted - t\n", "superseded"), ("Succeeded - t\n", "succeeded"), ("", "cannot read this run")] {
        std::fs::write(w.root.join("run-status"), status).unwrap();
        let (c, v) = open_go(&srv);
        assert_eq!(c, 403);
        assert!(v["text"].as_str().unwrap().contains(want), "{status}: {v}");
    }
    // a forged decision is refused and answers nothing
    let (c, _) = http(&srv.addr, "POST", "/go/api/decide", r#"{"nonce":"n","decision":"go","authData":"","clientDataJSON":"","signature":""}"#);
    assert_eq!(c, 403);
    assert!(!w.dagu_log().contains("human-task"), "a refused go reached dagu");
}

#[test]
fn subcommands_refuse_what_they_cannot_do() {
    let w = world("cli");
    let (code, _, err) = w.run(&[]);
    assert_eq!(code, 2);
    assert!(err.contains("usage: chorus-go"), "{err}");
    let (code, out, _) = w.run(&["register-link"]);
    assert_eq!(code, 0);
    assert!(out.starts_with(&format!("{ORIGIN}/go/register?token=")), "{out}");
    // a port already held: serve fails loud instead of hanging
    let held = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = held.local_addr().unwrap().to_string();
    let o = w.cmd(&["serve"]).env("CHORUS_GO_ADDR", &addr).output().unwrap();
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("bind"), "{}", String::from_utf8_lossy(&o.stderr));
    // a go store that cannot be opened
    std::fs::write(w.root.join("not-a-dir"), "").unwrap();
    let o = w.cmd(&["check", "1", "r"]).env("CHORUS_GO_DIR", w.root.join("not-a-dir")).output().unwrap();
    assert_eq!(o.status.code(), Some(1));
}
