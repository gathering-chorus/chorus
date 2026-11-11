// @test-type: unit — hermetic: the real athena-deploy binary against a loopback stub store; no Fuseki, no athena-make.
// @domain: knowledge
// @card: #4942
//! #4942 — the deploy refuses, before any store call, a land that would set an agent
//! principal's hostAccount to anything but chorus-<name>, or drop it. On #4773 a land
//! set three agents to "jeffbridwell" and locked every role out with no deploy-time
//! error. The rule is the shipped PrincipalShape rule (AgentRunsAsItsOwnAccount),
//! read from the werk's own TTL.
//!
//! Every run clears the environment and points FUSEKI_GSP/QUERY/UPDATE and
//! OWL_API_URL at a stub this test opens on a loopback port, with HOME and
//! CHORUS_ROOT in a scratch directory, so nothing here can reach a real store.
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
struct Req {
    method: String,
    target: String,
    body: String,
}

type Answer = Arc<dyn Fn(&Req, &[Req]) -> (u16, String) + Send + Sync>;

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                let h = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok());
                match h {
                    Some(v) => { out.push(v); i += 2; }
                    None => out.push(b'%'),
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A store that answers each request with `answer(req, earlier requests)`.
struct Stub {
    url: String,
    log: Arc<Mutex<Vec<Req>>>,
}

impl Stub {
    fn start(answer: Answer) -> Stub {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}", l.local_addr().unwrap().port());
        let log: Arc<Mutex<Vec<Req>>> = Arc::new(Mutex::new(Vec::new()));
        let seen = log.clone();
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                let mut r = BufReader::new(s.try_clone().unwrap());
                let mut first = String::new();
                if r.read_line(&mut first).is_err() { continue; }
                let mut parts = first.split_whitespace();
                let method = parts.next().unwrap_or("").to_string();
                let target = decode(parts.next().unwrap_or(""));
                let (mut len, mut form, mut expect) = (0usize, false, false);
                loop {
                    let mut h = String::new();
                    if r.read_line(&mut h).is_err() || h == "\r\n" || h.is_empty() { break; }
                    let lh = h.to_ascii_lowercase();
                    if let Some(v) = lh.strip_prefix("content-length:") { len = v.trim().parse().unwrap_or(0); }
                    if lh.starts_with("content-type:") && lh.contains("x-www-form-urlencoded") { form = true; }
                    if lh.starts_with("expect:") && lh.contains("100-continue") { expect = true; }
                }
                let mut w = s;
                if expect { let _ = w.write_all(b"HTTP/1.1 100 Continue\r\n\r\n"); }
                let mut raw = vec![0u8; len];
                let _ = r.read_exact(&mut raw);
                let raw = String::from_utf8_lossy(&raw).into_owned();
                let req = Req { method, target, body: if form { decode(&raw) } else { raw } };
                let earlier = seen.lock().unwrap().clone();
                let (code, text) = answer(&req, &earlier);
                seen.lock().unwrap().push(req);
                let _ = write!(w, "HTTP/1.1 {code} X\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len());
            }
        });
        Stub { url, log }
    }
    fn bodies(&self) -> Vec<String> {
        self.log.lock().unwrap().iter().map(|r| format!("{} {} {}", r.method, r.target, r.body)).collect()
    }
}



fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("identity-gate-4942-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn write(p: &Path, text: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

/// The werk's own security model, read at run time (never env!(), #4245): the gate's
/// rule is the shipped PrincipalShape rule, not a copy in this test.
fn shipped_security_model() -> String {
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    std::fs::read_to_string(format!("{crate_dir}/../../../roles/silas/ontology/security-model-3618.ttl")).unwrap()
}

const AGENT_OK: &str = "@prefix chorus: <https://jeffbridwell.com/chorus#> .\n\
chorus:principal-silas a chorus:Principal ; chorus:principalKind \"agent\" ; chorus:hostAccount \"chorus-silas\" .\n\
chorus:principal-pulse a chorus:Principal ; chorus:principalKind \"service\" ; chorus:hostAccount \"jeffbridwell\" .\n";

/// The binary with a cleared environment against a stub that records every call.
fn deploy(tag: &str, model: &str, with_rule: bool) -> (i32, String, Vec<String>) {
    deploy_in(tag, model, with_rule, None)
}

/// As deploy, with another file already in the tree that this land does not touch.
fn deploy_in(tag: &str, model: &str, with_rule: bool, untouched: Option<(&str, &str)>) -> (i32, String, Vec<String>) {
    let root = scratch(tag);
    if let Some((rel, text)) = untouched { write(&root.join(rel), text); }
    let ttl = root.join("principals.ttl");
    write(&ttl, model);
    if with_rule {
        write(&root.join("roles/silas/ontology/security-model-3618.ttl"), &shipped_security_model());
    }
    let stub = Stub::start(Arc::new(|_r: &Req, _e: &[Req]| (200, "n\r\n0\r\n".to_string())));
    let mut c = Command::new(env!("CARGO_BIN_EXE_athena-deploy"));
    c.env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", &root)
        .env("CHORUS_ROOT", &root)
        .env("TTL", &ttl)
        .env("FUSEKI_GSP", format!("{}/test/data", stub.url))
        .env("FUSEKI_QUERY", format!("{}/test/query", stub.url))
        .env("FUSEKI_UPDATE", format!("{}/test/update", stub.url))
        .env("OWL_API_URL", &stub.url)
        .env("FUSEKI_WRITE_ENV", root.join("no-credential.env"))
        .env("ONTOLOGY_GRAPH", "urn:test:ontology")
        .env("CHORUS_LOG", "true")
        .env("RETIREMENTS_FILE", root.join("retirements.jsonl"))
        .env("GRAPH_BACKUP_DIR", root.join("backups"))
        .env("FUSEKI_WRITE_TIMEOUT", "10");
    if let Ok(p) = std::env::var("LLVM_PROFILE_FILE") { c.env("LLVM_PROFILE_FILE", p); }
    let o = c.output().unwrap();
    let calls = stub.bodies();
    (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stderr).into_owned(), calls)
}

/// NEGATIVE PROOF: the #4773 shape — an agent set to jeffbridwell — is refused with
/// exit 2, names the principal, and the store sees no request at all.
#[test]
fn ac2_the_4773_replay_is_refused_before_any_store_call() {
    let bad = AGENT_OK.replace("\"chorus-silas\"", "\"jeffbridwell\"");
    let (code, err, calls) = deploy("wrong", &bad, true);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("principal-silas") && err.contains("jeffbridwell"), "names who and what: {err}");
    assert!(calls.is_empty(), "the store was called: {calls:?}");
}

/// NEGATIVE PROOF: a land that drops an agent's hostAccount is refused the same way.
#[test]
fn ac2_a_dropped_host_account_is_refused_before_any_store_call() {
    let bad = AGENT_OK.replace(" ; chorus:hostAccount \"chorus-silas\"", "");
    assert!(!bad.contains("chorus-silas"), "the fixture still carries the account");
    let (code, err, calls) = deploy("dropped", &bad, true);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("principal-silas"), "{err}");
    assert!(calls.is_empty(), "the store was called: {calls:?}");
}

/// A legitimate land (agent on its own account, a service on jeffbridwell) passes
/// the gate and goes on to the store.
#[test]
fn ac2_a_legitimate_land_passes_the_gate() {
    let (code, err, calls) = deploy("ok", AGENT_OK, true);
    assert!(!err.contains("identity gate"), "refused a legitimate land: {err}");
    assert!(!calls.is_empty(), "a legitimate land never reached the store (exit {code}): {err}");
}

/// A land of principals with no rule to check them by is refused, never waved
/// through: a deleted or renamed rule fails loudly (#3734).
#[test]
fn ac2_negative_principals_with_no_rule_are_refused() {
    let (code, err, calls) = deploy("norule", AGENT_OK, false);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("AgentRunsAsItsOwnAccount"), "{err}");
    assert!(calls.is_empty(), "the store was called: {calls:?}");
}

/// The shipped model passes its own gate today: a gate that refused the real tree
/// would stop every land (Wren, pair 4942).
#[test]
fn ac2_the_shipped_model_passes_the_gate() {
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let root = format!("{crate_dir}/../../..");
    let mut files = Vec::new();
    for role in ["silas", "kade", "wren", "borg-queen", "abby-normal"] {
        let Ok(entries) = std::fs::read_dir(format!("{root}/roles/{role}/ontology")) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("ttl") {
                files.push((p.display().to_string(), std::fs::read_to_string(&p).unwrap_or_default()));
            }
        }
    }
    assert!(files.iter().any(|(_, t)| t.contains("principalKind")), "the walk found no principals");
    assert_eq!(athena_deploy::identity_gate(&root, &files), Ok(()));
}

/// NEGATIVE PROOF (Wren, pair 4942): a card land carries only its changed files. A
/// land that sets only hostAccount, while the principal's principalKind lives in an
/// untouched file in the tree, is still checked and refused.
#[test]
fn ac2_a_host_account_change_with_kind_in_an_untouched_file_is_refused() {
    let land = "@prefix chorus: <https://jeffbridwell.com/chorus#> .\n\
chorus:principal-silas chorus:hostAccount \"jeffbridwell\" .\n";
    let tree = ("roles/silas/ontology/identity-principals-3613.ttl",
        "@prefix chorus: <https://jeffbridwell.com/chorus#> .\n\
chorus:principal-silas a chorus:Principal ; chorus:principalKind \"agent\" .\n");
    let (code, err, calls) = deploy_in("split", land, true, Some(tree));
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("principal-silas") && err.contains("jeffbridwell"), "{err}");
    assert!(calls.is_empty(), "the store was called: {calls:?}");
}
