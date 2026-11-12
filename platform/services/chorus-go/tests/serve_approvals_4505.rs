// @test-type: unit — hermetic: serve::handle on a loopback port, a scratch go store, a software P-256 key for the phone, and one loopback stand-in for the door, dagu's API, the identity service and the MCP door that only records; no real row or card is written.
// @domain: security
// @card: #4505
// @owner: wren
//! #4505 — the approvals routes end to end: Jeff's one list, a challenge per ask,
//! his passkey decision written as his own row, and an approved `cards add`
//! replayed as data. Each refusal is shown failing with nothing filed.
use chorus_go::*;
use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use p256::pkcs8::EncodePublicKey;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;

const RP: &str = "go.example.test";
const ORIGIN: &str = "https://go.example.test";
/// The lander's webid, and the login the stand-in identity service mints for it (#4710: the
/// decision row is written by the lander as principal-lander; the door's POST must carry it).
const LANDER_WEBID: &str = "https://id.example.test/lander/profile/card#me";
fn minted() -> String {
    use base64::Engine;
    format!("h.{}.s", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json!({"webid": LANDER_WEBID}).to_string()))
}

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

const ACTION: &str = r#"["add","Retire the X","--owner","kade","--priority","P3"]"#;

/// A scratch world. One loopback stand-in (#4710: the lander runs only /usr/bin/curl, so every
/// outside service is reached over HTTP) plays the door (GETs read approvals.json /
/// decisions.json, POSTs append "headers url body" to posts.log), dagu's API (status from the
/// landed / aborted / dagu-unreadable files, human-task answers appended to dagu.log), the
/// identity service (mint-fails refuses) and the MCP door (card filings appended to cards.log).
/// The flag files and logs are the ones the fake programs used before.
struct World { root: PathBuf, addr: String }

fn stand_in(root: PathBuf) -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap().to_string();
    std::thread::spawn(move || {
        for mut s in l.incoming().flatten() {
            let mut got = Vec::new(); let mut buf = [0u8; 65536];
            loop {
                let n = s.read(&mut buf).unwrap_or(0); got.extend_from_slice(&buf[..n]);
                let t = String::from_utf8_lossy(&got).to_string();
                if let Some((h, b)) = t.split_once("\r\n\r\n") {
                    let len = h.lines().find_map(|l| l.to_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0))).unwrap_or(0);
                    if b.len() >= len { break; }
                }
                if n == 0 { break; }
            }
            let t = String::from_utf8_lossy(&got).to_string();
            let (head, body) = t.split_once("\r\n\r\n").unwrap_or((t.as_str(), ""));
            let line = head.lines().next().unwrap_or("");
            let method = line.split_whitespace().next().unwrap_or("");
            let path = line.split_whitespace().nth(1).unwrap_or("").to_string();
            let has = |f: &str| root.join(f).exists();
            let read = |f: &str| std::fs::read_to_string(root.join(f)).unwrap_or_default();
            let append = |f: &str, l: String| { let mut h = std::fs::OpenOptions::new().create(true).append(true).open(root.join(f)).unwrap(); writeln!(h, "{l}").unwrap(); };
            let hdrs: String = head.lines().skip(1).filter(|l| l.starts_with("Authorization")).collect::<Vec<_>>().join("|");
            let (code, extra, out): (u16, String, String) = if path.starts_with("/decisions/") && method == "GET" {
                (200, String::new(), if path.contains("approvaldecisions") { read("decisions.json") } else { read("approvals.json") })
            } else if path.starts_with("/decisions/approvaldecisions") {
                let name = serde_json::from_str::<Value>(body).ok().and_then(|v| v["name"].as_str().map(String::from)).unwrap_or_default();
                if has("door-refuses") { (422, String::new(), r#"{"error":"refused"}"#.into()) }
                else if read("posts.log").contains(&format!("\"name\":\"{name}\"")) { (409, String::new(), r#"{"error":"exists"}"#.into()) }
                else { append("posts.log", format!("{hdrs} {path} {body}")); (201, String::new(), "{}".into()) }
            } else if path.starts_with("/api/v1/dag-runs/") && path.contains("/human-tasks/") {
                let p: Vec<&str> = path.trim_start_matches("/api/v1/dag-runs/").split('/').collect();
                let inputs = serde_json::from_str::<Value>(body).unwrap_or(Value::Null);
                let ins: String = inputs.as_object().into_iter().flatten().map(|(k, v)| format!(" --input {k}={}", v.as_str().unwrap_or(""))).collect();
                append("dagu.log", format!("human-task complete {} --run-id {} --step {}{ins}", p[0], p[1], p[3]));
                (200, String::new(), "{}".into())
            } else if path.starts_with("/api/v1/dag-runs/") {
                let id = path.rsplit('/').next().unwrap_or("").to_string();
                let listed = |f: &str| read(f).lines().any(|l| l == id);
                if has("dagu-unreadable") { (500, String::new(), String::new()) }
                else if listed("landed") { (200, String::new(), json!({"statusLabel": "succeeded", "startedAt": "2026-11-02 16:39:52", "nodes": [{"step": {"name": "cicd-land"}, "statusLabel": "succeeded"}]}).to_string()) }
                else if listed("aborted") { (200, String::new(), json!({"statusLabel": "succeeded", "startedAt": "2026-11-01 19:05:00", "nodes": [{"step": {"name": "cicd-land"}, "statusLabel": "skipped"}]}).to_string()) }
                else { (200, String::new(), json!({"statusLabel": "waiting", "startedAt": "2026-11-01 19:00:00", "nodes": []}).to_string()) }
            } else if path == "/.oidc/token" {
                if has("mint-fails") { (401, String::new(), r#"{"error":"invalid_client"}"#.into()) }
                else { (200, String::new(), json!({"access_token": minted()}).to_string()) }
            } else if path == "/mcp" {
                if body.contains("\"initialize\"") { (200, "Mcp-Session-Id: s-4505\r\n".into(), r#"{"jsonrpc":"2.0","id":1,"result":{}}"#.into()) }
                else if body.contains("notifications/initialized") { (202, String::new(), String::new()) }
                else {
                    let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
                    let a = &v["params"]["arguments"];
                    let mut line = format!("DEPLOY_ROLE=jeff add {}", a["title"].as_str().unwrap_or(""));
                    for (k, val) in a.as_object().into_iter().flatten().filter(|(k, _)| k.as_str() != "title") { line.push_str(&format!(" --{k} {}", val.as_str().unwrap_or(""))); }
                    append("cards.log", line);
                    (200, String::new(), r#"{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"Created #9999: filed"}]}}"#.into())
                }
            } else if path == "/api/chorus/spine-event" { (200, String::new(), "{}".into()) }
            // #4710 — GitHub as the lander sees it: an installation login, and no open PR (the
            // merge already happened, so a resent go goes straight to the run's go step)
            else if path.ends_with("/access_tokens") { (201, String::new(), r#"{"token":"ghs_STANDIN"}"#.into()) }
            else if path.contains("/pulls?") { (200, String::new(), "[]".into()) }
            // the round's PRs: merged at the signed round, unless the world says it never merged
            else if path.contains("/commits/") && path.ends_with("/pulls") {
                let sha = path.split("/commits/").nth(1).unwrap_or("").trim_end_matches("/pulls");
                let merged = if has("unmerged") { Value::Null } else { json!("2026-11-01T19:10:00Z") };
                (200, String::new(), json!([{"number": 77, "merged_at": merged,
                    "head": {"ref": "wren/4642", "sha": sha, "repo": {"full_name": "gathering-chorus/chorus"}}}]).to_string())
            }
            else { (404, String::new(), String::new()) };
            let _ = write!(s, "HTTP/1.1 {code} X\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{out}", out.len());
        }
    });
    addr
}

fn world(tag: &str) -> World {
    let root = std::env::temp_dir().join(format!("chorus-go-ap-4505-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    // the lander's own login: cred.json 0600 in the world's HOME, bound to this account
    let cred = chorus_go::identity::cred_path(&root);
    std::fs::create_dir_all(cred.parent().unwrap()).unwrap();
    std::fs::write(&cred, json!({"id": "lander", "secret": "s", "issuer": "https://id.example.test/", "webId": LANDER_WEBID,
        "hostAccount": chorus_go::identity::me().1}).to_string()).unwrap();
    { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(&cred, std::fs::Permissions::from_mode(0o600)).unwrap(); }
    let addr = stand_in(root.clone());
    // the lander's GitHub App key (a throwaway), 0600 in the world's HOME
    let app = root.join(".chorus/secrets/github-app");
    std::fs::create_dir_all(&app).unwrap();
    assert!(std::process::Command::new("/usr/bin/openssl").args(["genrsa", "-out"]).arg(app.join("key.pem")).arg("2048")
        .stderr(std::process::Stdio::null()).status().unwrap().success());
    std::fs::write(app.join("app-id"), "123\n").unwrap();
    std::fs::write(app.join("installation-id"), "456\n").unwrap();
    for f in ["key.pem", "app-id", "installation-id"] {
        use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(app.join(f), std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let a = |name: &str, owner: &str, at: &str, title: &str| json!({"name": name, "ownedBy": format!("https://jeffbridwell.com/chorus#principal-{owner}"),
        "approvalKind": "card", "approvalSubject": title, "approvalAction": ACTION, "createdAt": at});
    std::fs::write(root.join("approvals.json"), json!({"data": [
        a("approval-silas-2", "silas", "2026-11-01T13:10:00Z", "Second ask"),
        a("approval-wren-1", "wren", "2026-11-01T13:00:00Z", "First ask"),
        a("approval-kade-3", "kade", "2026-11-01T12:00:00Z", "Already decided"),
    ]}).to_string()).unwrap();
    std::fs::write(root.join("decisions.json"), json!({"data": [
        {"name": "decision-approval-kade-3", "ownedBy": "chorus:principal-jeff", "decides": "chorus:approval-kade-3"},
        // a "decision" a role wrote decides nothing
        {"name": "decision-approval-wren-1", "ownedBy": "chorus:principal-kade", "decides": "chorus:approval-wren-1"},
    ]}).to_string()).unwrap();
    World { root, addr }
}
impl World {
    fn read(&self, f: &str) -> String { std::fs::read_to_string(self.root.join(f)).unwrap_or_default() }
}

static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());
struct Server { addr: String, _env: std::sync::MutexGuard<'static, ()> }

fn serve(w: &World, phone: &Phone) -> Server {
    let guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let a = &w.addr;
    std::env::set_var("HOME", &w.root);
    std::env::set_var("CHORUS_GO_DOOR", format!("http://{a}"));
    std::env::set_var("CHORUS_GO_DAGU_API", format!("http://{a}/api/v1"));
    std::env::set_var("CHORUS_CSS_LOCAL", format!("http://{a}"));
    std::env::set_var("CHORUS_GO_MCP", format!("http://{a}/mcp"));
    std::env::set_var("CHORUS_GO_SPINE", format!("http://{a}/api/chorus/spine-event"));
    std::env::set_var("WERK_GITHUB_API", format!("http://{a}"));
    std::env::remove_var("WERK_GITHUB_APP_DIR");
    std::env::set_var("CHORUS_GO_RP_ID", RP);
    std::env::set_var("CHORUS_GO_ORIGIN", ORIGIN);
    let store = Store::open(&w.root.join("go")).unwrap();
    let reg = store.mint_register_token().unwrap();
    let cd = Phone::client_data("webauthn.create", &register_challenge(&reg));
    register(&store, &reg, "cred-1", &phone.spki(), &cd, RP, ORIGIN).unwrap();
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap().to_string();
    std::thread::spawn(move || { for s in l.incoming().flatten() { let _ = chorus_go::serve::handle(&store, s); } });
    Server { addr, _env: guard }
}

fn http(addr: &str, method: &str, path: &str, body: &str) -> (u16, Value) {
    let mut s = TcpStream::connect(addr).unwrap();
    write!(s, "{method} {path} HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let code = out.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    (code, out.split_once("\r\n\r\n").and_then(|(_, b)| serde_json::from_str(b).ok()).unwrap_or(Value::Null))
}

fn decide(srv: &Server, phone: &mut Phone, row: &str, decision: &str) -> (u16, Value) {
    let (c, ch) = http(&srv.addr, "POST", "/go/api/approvals/challenge", &json!({"row": row}).to_string());
    assert_eq!(c, 200, "{ch}");
    let key = if decision == "approve" { "challengeApprove" } else { "challengeDecline" };
    let (ad, cd, sig) = phone.sign(&unb64(ch[key].as_str().unwrap()).unwrap());
    http(&srv.addr, "POST", "/go/api/approvals/decide", &json!({"row": row, "nonce": ch["nonce"], "decision": decision,
        "authData": ad, "clientDataJSON": cd, "signature": sig}).to_string())
}

#[test]
fn the_list_is_pending_asks_oldest_first_and_a_roles_decision_decides_nothing() {
    let w = world("list"); let p = Phone::new(); let srv = serve(&w, &p);
    let (c, b) = http(&srv.addr, "GET", "/go/api/approvals", "");
    assert_eq!(c, 200, "{b}");
    let rows: Vec<&str> = b["approvals"].as_array().unwrap().iter().map(|r| r["row"].as_str().unwrap()).collect();
    assert_eq!(rows, vec!["approval-wren-1", "approval-silas-2"]);
    assert_eq!(b["approvals"][0]["asker"], "principal-wren");
    assert_eq!(b["approvals"][0]["title"], "First ask");
}

#[test]
fn jeffs_approve_writes_his_decision_row_and_replays_the_argv_as_jeff() {
    let w = world("approve"); let mut p = Phone::new(); let srv = serve(&w, &p);
    let (c, b) = decide(&srv, &mut p, "approval-wren-1", "approve");
    assert_eq!(c, 200, "{b}");
    let posts = w.read("posts.log");
    assert!(posts.contains(&format!("Authorization: Bearer {}", minted())), "the decision row is written with the lander's login: {posts}");
    // NEGATIVE PROOF (Silas): the identity never rides curl's argv, where `ps` shows it.
    assert!(posts.contains("/decisions/approvaldecisions"), "{posts}");
    assert!(posts.contains(r#""decides":"approval-wren-1""#) && posts.contains(r#""approvalVerdict":"approve""#), "{posts}");
    assert_eq!(w.read("cards.log").trim(), "DEPLOY_ROLE=jeff add Retire the X --owner kade --priority P3");
}

#[test]
fn a_decline_records_and_files_nothing() {
    let w = world("decline"); let mut p = Phone::new(); let srv = serve(&w, &p);
    let (c, b) = decide(&srv, &mut p, "approval-silas-2", "decline");
    assert_eq!(c, 200, "{b}");
    assert!(w.read("posts.log").contains(r#""approvalVerdict":"decline""#));
    assert_eq!(w.read("cards.log"), "");
}

#[test]
fn negative_proof_a_decision_no_passkey_signed_writes_nothing_and_files_nothing() {
    let w = world("unsigned"); let p = Phone::new(); let srv = serve(&w, &p);
    let mut other = Phone::new();
    let (c, _) = decide(&srv, &mut other, "approval-wren-1", "approve");
    assert_eq!(c, 403);
    assert_eq!(w.read("posts.log"), "");
    assert_eq!(w.read("cards.log"), "");
}

#[test]
fn negative_proof_a_decided_ask_cannot_be_asked_for_again() {
    let w = world("decided"); let p = Phone::new(); let srv = serve(&w, &p);
    let (c, b) = http(&srv.addr, "POST", "/go/api/approvals/challenge", &json!({"row": "approval-kade-3"}).to_string());
    assert_eq!(c, 403, "{b}");
    assert!(b["text"].as_str().unwrap().contains("already decided"));
}

#[test]
fn negative_proof_with_no_jeff_identity_nothing_is_filed() {
    let w = world("nomint"); let mut p = Phone::new(); let srv = serve(&w, &p);
    std::fs::write(w.root.join("mint-fails"), "").unwrap();
    let (c, b) = decide(&srv, &mut p, "approval-wren-1", "approve");
    assert_eq!(c, 403, "{b}");
    assert_eq!(w.read("cards.log"), "");
}

#[test]
fn negative_proof_when_the_door_refuses_the_decision_nothing_is_filed() {
    let w = world("refused"); let mut p = Phone::new(); let srv = serve(&w, &p);
    std::fs::write(w.root.join("door-refuses"), "").unwrap();
    let (c, _) = decide(&srv, &mut p, "approval-wren-1", "approve");
    assert_eq!(c, 403);
    assert_eq!(w.read("cards.log"), "");
}

#[test]
fn negative_proof_two_racing_decides_file_once() {
    let w = world("race"); let mut p = Phone::new(); let srv = serve(&w, &p);
    let ch = |_: ()| http(&srv.addr, "POST", "/go/api/approvals/challenge", &json!({"row": "approval-wren-1"}).to_string()).1;
    let (a, b) = (ch(()), ch(()));
    let send = |p: &mut Phone, c: &Value| {
        let (ad, cd, sig) = p.sign(&unb64(c["challengeApprove"].as_str().unwrap()).unwrap());
        http(&srv.addr, "POST", "/go/api/approvals/decide", &json!({"row": "approval-wren-1", "nonce": c["nonce"], "decision": "approve",
            "authData": ad, "clientDataJSON": cd, "signature": sig}).to_string())
    };
    assert_eq!(send(&mut p, &a).0, 200);
    let (c2, b2) = send(&mut p, &b);
    assert_eq!(c2, 403, "{b2}");
    assert_eq!(w.read("cards.log").lines().count(), 1, "filed once");
}

// #4642 — the stuck-go recovery (Silas, Kade 19:17): Jeff's go is recorded but the run never took
// it. It stays on his list as signed-not-sent; resend re-verifies the STORED signature (no new
// Face ID) and answers the run's go step. Only a jeff-owned, validly signed decision is re-sent.
const GO_RUN: &str = "034qFPOewc9praHJbg6SYH";
fn go_world(tag: &str, phone: &mut Phone, owner: &str, forge: bool) -> World {
    let w = world(tag);
    let action = format!(r#"{{"card":4642,"run":"{GO_RUN}","round":"5174a0592261bbd5b452ec1287cda4e5e2fda45a"}}"#);
    let row = "approval-go-4642";
    let nonce = "n-recorded";
    let (ad, cd, sig) = phone.sign(&approval_challenge(row, &action_sha(&action), "approve", nonce));
    let proof = json!({"nonce": if forge { "n-other" } else { nonce }, "authData": ad, "clientDataJSON": cd, "signature": sig});
    std::fs::write(w.root.join("approvals.json"), json!({"data": [
        {"name": format!("chorus:{row}"), "ownedBy": "chorus:principal-wren", "approvalKind": "go",
         "approvalSubject": "#4642 Approvals round 2", "approvalAction": action, "createdAt": "2026-11-01T19:00:00Z"}]}).to_string()).unwrap();
    std::fs::write(w.root.join("decisions.json"), json!({"data": [
        {"name": format!("decision-{row}"), "ownedBy": format!("chorus:{owner}"), "decides": format!("chorus:{row}"),
         "approvalVerdict": "approve", "passkeyProof": proof.to_string()}]}).to_string()).unwrap();
    w
}

#[test]
fn a_signed_go_its_run_never_took_is_listed_and_resent_on_the_recorded_signature() {
    let mut p = Phone::new();
    let w = go_world("resend", &mut p, "principal-jeff", false);
    let srv = serve(&w, &p);
    let (c, b) = http(&srv.addr, "GET", "/go/api/approvals", "");
    assert_eq!(c, 200, "{b}");
    let rows = b["approvals"].as_array().unwrap();
    assert!(rows.iter().any(|r| r["row"] == "approval-go-4642" && r["state"] == "signed-not-sent"), "{b}");
    let (c, b) = http(&srv.addr, "POST", "/go/api/approvals/resend", &json!({"row": "approval-go-4642"}).to_string());
    assert_eq!(c, 200, "{b}");
    let log = w.read("dagu.log");
    assert!(log.contains("human-task complete cicd-go") && log.contains(&format!("go-{GO_RUN}")) && log.contains("decision=go"), "{log}");
}

#[test]
fn negative_proof_resend_refuses_a_decision_that_is_not_jeffs_signed_go() {
    for (tag, owner, forge, why) in [("resend-kade", "principal-kade", false, "not principal-jeff or principal-lander"),
                                     ("resend-forged", "principal-jeff", true, "not signed by Jeff's passkey")] {
        let mut p = Phone::new();
        let w = go_world(tag, &mut p, owner, forge);
        let srv = serve(&w, &p);
        let (c, b) = http(&srv.addr, "POST", "/go/api/approvals/resend", &json!({"row": "approval-go-4642"}).to_string());
        assert_eq!(c, 403, "{tag}: {b}");
        assert!(b["text"].as_str().unwrap_or("").contains(why), "{tag}: {b}");
        assert_eq!(w.read("dagu.log"), "", "{tag}: nothing answered the run");
        drop(srv);
    }
}

// #4771 AC2 — one go per card. A card's new cicd run supersedes (aborts) its
// older go; that older go row is not offered, cannot be asked for, and a
// decide that races the supersede writes nothing and answers no run.
const OLD_RUN: &str = "034qq6XgiAeVGOv2S6DEhR"; // run 11, started 11-02 17:28:41 (UUIDv7)
const NEW_RUN: &str = "034qq9S3EbXzSGzpLWcIXj"; // run 12, started 17:30:36
fn two_go_world(tag: &str) -> World {
    let w = world(tag);
    // the asker's werk, whose HEAD is the round the go was asked at
    let werk = w.root.join("werk/wren-4771");
    std::fs::create_dir_all(&werk).unwrap();
    let git = |args: &[&str]| assert!(std::process::Command::new("git").arg("-C").arg(&werk).args(args)
        .env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t").env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t")
        .status().unwrap().success());
    git(&["init", "-q"]);
    git(&["commit", "-q", "--allow-empty", "-m", "round"]);
    let o = std::process::Command::new("git").arg("-C").arg(&werk).args(["rev-parse", "HEAD"]).output().unwrap();
    let round = String::from_utf8_lossy(&o.stdout).trim().to_string();
    let go = |name: &str, run: &str, at: &str| json!({"name": name, "ownedBy": "chorus:principal-wren",
        "approvalKind": "go", "approvalSubject": "#4771 go lands hands-free",
        "approvalAction": format!(r#"{{"card":4771,"run":"{run}","round":"{round}"}}"#), "createdAt": at});
    std::fs::write(w.root.join("approvals.json"), json!({"data": [
        // the older run's row posted LAST (17:32, the slow run reached its go late; Silas 17:36)
        go("approval-go-4771-old", OLD_RUN, "2026-11-02T17:32:00-05:00"),
        go("approval-go-4771-new", NEW_RUN, "2026-11-02T22:31:00Z")]}).to_string()).unwrap();
    std::fs::write(w.root.join("decisions.json"), json!({"data": []}).to_string()).unwrap();
    w
}
fn supersede(w: &World, run: &str) {
    // go_run asks cicd-go for go-<run> first; both ids read Aborted
    std::fs::write(w.root.join("aborted"), format!("{run}\ngo-{run}\n")).unwrap();
}

#[test]
fn a_superseded_go_is_not_offered_and_asking_for_it_points_to_the_newest_go() {
    let p = Phone::new();
    let w = two_go_world("stale-list");
    supersede(&w, OLD_RUN);
    let srv = serve(&w, &p);
    std::env::set_var("CHORUS_WERK_BASE", w.root.join("werk"));
    let (c, b) = http(&srv.addr, "GET", "/go/api/approvals", "");
    assert_eq!(c, 200, "{b}");
    let rows: Vec<&str> = b["approvals"].as_array().unwrap().iter().filter_map(|r| r["row"].as_str()).collect();
    assert!(rows.iter().any(|r| *r == "approval-go-4771-new"), "the newest go is offered: {b}");
    assert!(!rows.iter().any(|r| *r == "approval-go-4771-old"), "the superseded go is not offered: {b}");
    let (c, b) = http(&srv.addr, "POST", "/go/api/approvals/challenge", &json!({"row": "approval-go-4771-old"}).to_string());
    assert_ne!(c, 200, "no challenge for a superseded go: {b}");
    let t = b["text"].as_str().unwrap_or("");
    assert!(t.contains("superseded") && t.contains("approval-go-4771-new"), "points to the newest go: {b}");
}

#[test]
fn negative_proof_a_decide_racing_the_supersede_writes_nothing_and_answers_no_run() {
    let mut p = Phone::new();
    let w = two_go_world("stale-decide");
    let srv = serve(&w, &p);
    std::env::set_var("CHORUS_WERK_BASE", w.root.join("werk"));
    // Jeff opens the old go while it is the card's only one…
    let both = std::fs::read_to_string(w.root.join("approvals.json")).unwrap();
    let only_old: Value = serde_json::from_str(&both).unwrap();
    let only_old = json!({"data": [only_old["data"][0].clone()]});
    std::fs::write(w.root.join("approvals.json"), only_old.to_string()).unwrap();
    let (c, ch) = http(&srv.addr, "POST", "/go/api/approvals/challenge", &json!({"row": "approval-go-4771-old"}).to_string());
    assert_eq!(c, 200, "{ch}");
    // …a new cicd run of the card posts its go and supersedes the old one before he signs.
    std::fs::write(w.root.join("approvals.json"), both).unwrap();
    supersede(&w, OLD_RUN);
    let (ad, cd, sig) = p.sign(&unb64(ch["challengeApprove"].as_str().unwrap()).unwrap());
    let (c, b) = http(&srv.addr, "POST", "/go/api/approvals/decide", &json!({"row": "approval-go-4771-old",
        "nonce": ch["nonce"], "decision": "approve", "authData": ad, "clientDataJSON": cd, "signature": sig}).to_string());
    assert_ne!(c, 200, "a stale go is not taken: {b}");
    assert!(b["text"].as_str().unwrap_or("").contains("approval-go-4771-new"), "{b}");
    assert_eq!(w.read("dagu.log"), "", "no run's go step was answered");
    assert_eq!(w.read("posts.log"), "", "no decision row was written");
}

#[test]
fn negative_proof_an_unreadable_dagu_keeps_the_go_rows_listed_with_the_reason() {
    let p = Phone::new();
    let w = two_go_world("stale-unreadable");
    std::fs::write(w.root.join("dagu-unreadable"), "").unwrap();
    let srv = serve(&w, &p);
    std::env::set_var("CHORUS_WERK_BASE", w.root.join("werk"));
    let (c, b) = http(&srv.addr, "GET", "/go/api/approvals", "");
    assert_eq!(c, 200, "{b}");
    let rows = b["approvals"].as_array().unwrap();
    // the card's newest go stays listed with the reason (the older one is the card's
    // superseded go by the one-go-per-card rule, which needs no dagu)
    for name in ["approval-go-4771-new"] {
        let r = rows.iter().find(|r| r["row"] == name).unwrap_or_else(|| panic!("{name} hidden on an unreadable dagu: {b}"));
        assert!(r["state"].as_str().unwrap_or("").contains("cannot read"), "{name} carries the reason: {b}");
    }
}

/// #4771 — timing-proof one go per card (Silas 17:33): a run that reached its go AFTER the
/// newer run started was never superseded in dagu — both wait. The list offers only the newest
/// and the older is refused, pointing at it.
/// NEGATIVE PROOF: without newest_go_per_card both rows were offered (run 11 + run 12, 11-02).
#[test]
fn two_waiting_gos_of_a_card_offer_only_the_newest_and_refuse_the_older() {
    let p = Phone::new();
    let w = two_go_world("both-waiting");
    let srv = serve(&w, &p);
    std::env::set_var("CHORUS_WERK_BASE", w.root.join("werk"));
    let (c, b) = http(&srv.addr, "GET", "/go/api/approvals", "");
    assert_eq!(c, 200, "{b}");
    let rows: Vec<&str> = b["approvals"].as_array().unwrap().iter().filter_map(|r| r["row"].as_str()).collect();
    assert!(rows.contains(&"approval-go-4771-new"), "{b}");
    assert!(!rows.contains(&"approval-go-4771-old"), "the older waiting go is not offered: {b}");
    let (c, b) = http(&srv.addr, "POST", "/go/api/approvals/challenge", &json!({"row": "approval-go-4771-old"}).to_string());
    assert_ne!(c, 200, "{b}");
    let t = b["text"].as_str().unwrap_or("");
    assert!(t.contains("superseded") && t.contains("approval-go-4771-new"), "{b}");
    let (c, b) = http(&srv.addr, "POST", "/go/api/approvals/challenge", &json!({"row": "approval-go-4771-new"}).to_string());
    assert_eq!(c, 200, "the newest go still opens: {b}");
}

/// #4771 — a go whose run already landed (Jeff's go went through) leaves the list: it cannot
/// be signed again (11-02 17:51: run 10's landed go still showed beside two others).
/// NEGATIVE PROOF: the same row with its run still waiting is offered (it is the card's newest).
#[test]
fn a_landed_go_leaves_the_list_and_a_waiting_one_stays() {
    let p = Phone::new();
    let w = two_go_world("landed");
    let srv = serve(&w, &p);
    std::env::set_var("CHORUS_WERK_BASE", w.root.join("werk"));
    let rows = |b: &Value| b["approvals"].as_array().unwrap().iter().filter_map(|r| r["row"].as_str().map(str::to_string)).collect::<Vec<_>>();
    let (_, b) = http(&srv.addr, "GET", "/go/api/approvals", "");
    assert!(rows(&b).contains(&"approval-go-4771-new".to_string()), "waiting: offered {b}");
    std::fs::write(w.root.join("landed"), format!("{NEW_RUN}\ngo-{NEW_RUN}\n")).unwrap();
    let (_, b) = http(&srv.addr, "GET", "/go/api/approvals", "");
    assert!(!rows(&b).iter().any(|r| r.starts_with("approval-go-4771")), "a landed card offers no go: {b}");
}

/// #4771 — the page's hidden panels stay hidden: `.row`/`.card` set display, which beats the
/// browser's [hidden] rule, so the dead Go/No panel showed "Loading…" above the real rows.
/// NEGATIVE PROOF: the rule is checked by its exact text; without it this fails.
#[test]
fn the_page_keeps_hidden_panels_hidden() {
    let page = include_str!("../src/page.html");
    assert!(page.contains("[hidden] { display: none !important; }"), "no [hidden] override in page.html");
    assert!(page.contains("id=\"regrow\" hidden"));
    // the dead Go/No panel is gone, not hidden (Silas 17:56): Approve on a row is the only go
    assert!(!page.contains("id=\"buttons\"") && !page.contains(">Go</button>"), "the old Go/No panel is back");
}

// #4710 review of 113c9fe51 (E) — "no open PR" is not taken as merged: a resent go whose PR GitHub
// shows unmerged at the signed round is refused, and the run's go step is never answered.
#[test]
fn negative_proof_a_resent_go_whose_pr_never_merged_is_refused() {
    let mut p = Phone::new();
    let w = go_world("resend-unmerged", &mut p, "principal-jeff", false);
    std::fs::write(w.root.join("unmerged"), "").unwrap();
    let srv = serve(&w, &p);
    let (c, b) = http(&srv.addr, "POST", "/go/api/approvals/resend", &json!({"row": "approval-go-4642"}).to_string());
    assert_ne!(c, 200, "{b}");
    assert!(b["text"].as_str().unwrap_or("").contains("no PR for #4642 merged"), "{b}");
    assert!(!w.read("dagu.log").contains("human-task complete cicd-go"), "the go step must not be answered");
}
