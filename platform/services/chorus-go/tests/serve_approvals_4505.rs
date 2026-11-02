// @test-type: unit — hermetic: serve::handle on a loopback port, a scratch go store, a software P-256 key for the phone, and a fake door (curl), identity mint and cards that only record; no real row or card is written.
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
use std::path::{Path, PathBuf};

const RP: &str = "go.example.test";
const ORIGIN: &str = "https://go.example.test";
/// What the fake identity mint prints for jeff; the door's POST must carry it.
const MINTED: &str = "minted-for-jeff";

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

fn sh(p: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

const ACTION: &str = r#"["add","Retire the X","--owner","kade","--priority","P3"]"#;

/// A scratch world. The door is a fake curl: GETs read approvals.json / decisions.json,
/// POSTs are appended to posts.log with their header and body.
struct World { root: PathBuf }
fn world(tag: &str) -> World {
    let root = std::env::temp_dir().join(format!("chorus-go-ap-4505-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let r = root.display();
    sh(&root.join("bin/curl"), &format!(r#"#!/bin/sh
echo "$@" >> {r}/curl-argv.log
url=""; data=""; hdr=""; post=0
while [ $# -gt 0 ]; do case "$1" in
  -X) [ "$2" = POST ] && post=1; shift 2;;
  --data-binary) data="$2"; shift 2;;
  -H) if [ "$2" = "@-" ]; then hdr="$hdr|$(cat)"; else hdr="$hdr|$2"; fi; shift 2;;
  --max-time) shift 2;;
  -*) shift;;
  *) url="$1"; shift;;
esac; done
if [ $post = 1 ]; then [ -f {r}/door-refuses ] && exit 22
  name=$(printf '%s' "$data" | sed -n 's/.*"name":"\([^"]*\)".*/\1/p'); grep -q "\"name\":\"$name\"" {r}/posts.log 2>/dev/null && exit 22
  printf '%s %s %s\n' "$hdr" "$url" "$data" >> {r}/posts.log; echo '{{}}'; exit 0; fi
case "$url" in
  */decisions/approvaldecisions*) cat {r}/decisions.json;;
  */decisions/approvals*) cat {r}/approvals.json;;
  *) exit 22;;
esac
"#));
    // #4642: a dagu whose go step waits; every human-task answer is logged
    // #4771: a run id listed in `aborted` reads Aborted (cicd-supersede stopped it)
    sh(&root.join("bin/dagu"), &format!("#!/bin/sh\ncase \"$*\" in *status*) id=$(echo \"$*\" | sed 's/.*--run-id //'); if grep -qxF -- \"$id\" {r}/aborted 2>/dev/null; then echo 'Aborted - 2026-11-01 19:05:00'; else echo 'Waiting - 2026-11-01 19:00:00'; fi;; *human-task*) echo \"$*\" >> {r}/dagu.log;; esac\nexit 0\n"));
    sh(&root.join("bin/mint"), &format!("#!/bin/sh\n[ -f {r}/mint-fails ] && exit 1\n[ \"$1\" = jeff ] && echo {MINTED}\n"));
    sh(&root.join("bin/cards"), &format!("#!/bin/sh\necho \"DEPLOY_ROLE=$DEPLOY_ROLE $*\" >> {r}/cards.log\necho 'Created #9999: filed'\n"));
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
    World { root }
}
impl World {
    fn read(&self, f: &str) -> String { std::fs::read_to_string(self.root.join(f)).unwrap_or_default() }
}

static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());
struct Server { addr: String, _env: std::sync::MutexGuard<'static, ()> }

fn serve(w: &World, phone: &Phone) -> Server {
    let guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
    for (k, v) in [("CHORUS_GO_CURL", "bin/curl"), ("CHORUS_GO_IDENTITY_TOKEN_BIN", "bin/mint"), ("CHORUS_GO_CARDS", "bin/cards")] {
        std::env::set_var(k, w.root.join(v));
    }
    std::env::set_var("CHORUS_GO_DOOR", "http://door.invalid");
    std::env::set_var("CHORUS_GO_DAGU", w.root.join("bin/dagu"));
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
    assert!(posts.contains(MINTED), "the decision row is written with jeff's identity: {posts}");
    // NEGATIVE PROOF (Silas): the identity never rides curl's argv, where `ps` shows it.
    assert!(!w.read("curl-argv.log").contains(MINTED), "token on argv: {}", w.read("curl-argv.log"));
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
    for (tag, owner, forge, why) in [("resend-kade", "principal-kade", false, "not owned by principal-jeff"),
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
const OLD_RUN: &str = "034qOLDrun0000000000aa";
const NEW_RUN: &str = "034qNEWrun0000000000bb";
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
        go("approval-go-4771-old", OLD_RUN, "2026-11-02T07:00:00Z"),
        go("approval-go-4771-new", NEW_RUN, "2026-11-02T07:30:00Z")]}).to_string()).unwrap();
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
    // Jeff opens the old go while it still waits…
    let (c, ch) = http(&srv.addr, "POST", "/go/api/approvals/challenge", &json!({"row": "approval-go-4771-old"}).to_string());
    assert_eq!(c, 200, "{ch}");
    // …a new cicd run of the card supersedes it before he signs.
    supersede(&w, OLD_RUN);
    let (ad, cd, sig) = p.sign(&unb64(ch["challengeApprove"].as_str().unwrap()).unwrap());
    let (c, b) = http(&srv.addr, "POST", "/go/api/approvals/decide", &json!({"row": "approval-go-4771-old",
        "nonce": ch["nonce"], "decision": "approve", "authData": ad, "clientDataJSON": cd, "signature": sig}).to_string());
    assert_ne!(c, 200, "a stale go is not taken: {b}");
    assert!(b["text"].as_str().unwrap_or("").contains("approval-go-4771-new"), "{b}");
    assert_eq!(w.read("dagu.log"), "", "no run's go step was answered");
    assert_eq!(w.read("posts.log"), "", "no decision row was written");
}
