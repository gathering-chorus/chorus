// @test-type: unit — hermetic: the real athena-deploy binary against a loopback stub store; no Fuseki, no athena-make.
// @domain: knowledge
// @card: #4755
//! #4755 — the deploy's process half: `run_athena_deploy_plan` end to end, every
//! retirement form, and each refusal it makes before or after a write.
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

const C: &str = "https://jeffbridwell.com/chorus#";

/// Answers a healthy store gives: every count 0, every write 200, the route
/// list names one served collection, and each retirement's target is present
/// until its delete has been seen.
fn healthy(req: &Req, earlier: &[Req]) -> (u16, String) {
    let b = &req.body;
    let deleted = |needle: &str| earlier.iter().any(|e| e.method == "POST" && !e.body.starts_with("query=") && e.body.contains(needle));
    if req.target.contains("__model_deploy_probe__") {
        return (200, r#"{"served":["/v1/things/gadgets"]}"#.into());
    }
    if b.starts_with("query=ASK") {
        let tail = b.trim_start_matches("query=ASK").to_string();
        let gone = earlier.iter().any(|e| e.body.contains("DELETE DATA") && e.body.contains(tail.trim()));
        return (200, format!(r#"{{"head":{{}},"boolean":{}}}"#, !gone));
    }
    if b.starts_with("query=SELECT DISTINCT ?s WHERE { VALUES") {
        return (200, format!(r#"{{"results":{{"bindings":[{{"s":{{"type":"uri","value":"{C}RetireMe"}}}}]}}}}"#));
    }
    if b.starts_with("query=CONSTRUCT") && b.contains("RetireMe") {
        return (200, "<urn:a> <urn:b> <urn:c> .\n<urn:d> <urn:e> <urn:f> .\n".into());
    }
    if b.starts_with("query=") && b.contains("RetireMe") && b.contains("COUNT") {
        return (200, "n\r\n2\r\n".into());
    }
    if b.starts_with("query=SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE { GRAPH <") && b.contains(&format!("?s a <{C}Widget> }}")) {
        let n = if deleted(&format!("?s a <{C}Widget> ; ?p ?o")) { 0 } else { 3 };
        return (200, format!("n\r\n{n}\r\n"));
    }
    if b.starts_with("query=") {
        return (200, "n\r\n0\r\n".into());
    }
    (200, String::new())
}

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("deploy-run-4755-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn write(p: &Path, text: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

const MODEL: &str = "@prefix chorus: <https://jeffbridwell.com/chorus#> .\n\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
@prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
chorus:Widget a owl:Class ; rdfs:label \"Widget\" .\n";

/// env_clear drops the coverage run's profile path; carry it so the child's
/// lines are counted.
fn keep_profile(c: &mut Command) {
    if let Ok(p) = std::env::var("LLVM_PROFILE_FILE") {
        c.env("LLVM_PROFILE_FILE", p);
    }
}

/// The binary with a cleared environment: only the stub, the scratch tree and PATH.
fn deploy(stub: &Stub, root: &Path, args: &[&str], env: &[(&str, &str)]) -> (i32, String, String) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_athena-deploy"));
    c.env_clear()
        .args(args)
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", root)
        .env("CHORUS_ROOT", root)
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
    keep_profile(&mut c);
    for (k, v) in env {
        c.env(k, v);
    }
    let o = c.output().unwrap();
    (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).into(), String::from_utf8_lossy(&o.stderr).into())
}

/// One TTL file, deployed partially (TTL=), with nothing staged for retirement.
fn partial(tag: &str, answer: Answer, extra: &[(&str, &str)], model: &str, retirements: &str) -> (i32, String, String, Stub) {
    let root = scratch(tag);
    let ttl = root.join("model.ttl");
    write(&ttl, model);
    write(&root.join("retirements.jsonl"), retirements);
    let stub = Stub::start(answer);
    let ttl_s = ttl.display().to_string();
    let mut env = vec![("TTL", ttl_s.as_str())];
    env.extend_from_slice(extra);
    let (code, out, err) = deploy(&stub, &root, &[], &env);
    (code, out, err, stub)
}

#[test]
fn a_partial_deploy_stages_merges_verifies_and_stamps() {
    let (code, out, err, stub) = partial("ok", Arc::new(healthy), &[], MODEL, "");
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains("deployed 1 model file(s) -> <urn:test:ontology>"), "{out}");
    assert!(out.contains("stamped deployedFromCommit=unknown"), "{out}");
    assert!(out.contains("0 empty property nodes"), "{out}");
    let log = stub.bodies();
    assert!(log.iter().any(|l| l.starts_with("POST /test/data?graph=urn:test:ontology-staging-deploy")), "{log:?}");
    assert!(log.iter().any(|l| l.contains("deployedFromCommit") && l.contains("INSERT DATA")), "no stamp write");
}

/// Every retirement form, executed against a store that holds each target.
#[test]
fn every_staged_retirement_form_is_executed_and_verified() {
    let lines = [
        r#"{"retire_subject": "https://jeffbridwell.com/chorus#Old", "status": "retired"}"#.to_string(),
        r#"{"subject_domain": "awareness", "object_class": "Widget", "graph": "urn:chorus:domains:domains", "status": "staged"}"#.into(),
        r#"{"triple_subject": "https://jeffbridwell.com/chorus#pulse", "triple_predicate": "https://jeffbridwell.com/chorus#servesOperation", "triple_object": "https://jeffbridwell.com/chorus#tree", "status": "staged"}"#.into(),
        r#"{"retire_subject": "https://jeffbridwell.com/chorus#RetireMe", "status": "staged"}"#.into(),
        r#"{"retire_subject": "https://jeffbridwell.com/chorus#LongGone", "status": "staged"}"#.into(),
        r#"{"merge_subject": "https://jeffbridwell.com/chorus#RetireMeDup", "into": "https://jeffbridwell.com/chorus#Kept", "status": "staged"}"#.into(),
        r#"{"retire_class": "https://jeffbridwell.com/chorus#Widget", "graph": "urn:chorus:domains:awareness", "status": "staged"}"#.into(),
        r#"{"retire_graph": "urn:test:RetireMe-graph", "status": "staged"}"#.into(),
        r#"{"retire_graph": "urn:test:empty-graph", "status": "staged"}"#.into(),
        String::new(),
    ];
    let (code, out, err, stub) = partial("retire", Arc::new(healthy), &[], MODEL, &lines.join("\n"));
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains("retirement line 1 is 'retired', not staged"), "{out}");
    assert!(out.contains("claim awareness->Widget removed"), "{out}");
    assert!(out.contains("triple <https://jeffbridwell.com/chorus#pulse>"), "{out}");
    assert!(out.contains("subject <https://jeffbridwell.com/chorus#LongGone> already absent"), "{out}");
    assert!(out.contains("retirement executed — <urn:test:ontology> subject <https://jeffbridwell.com/chorus#RetireMe> (2 triple(s)"), "{out}");
    assert!(out.contains("refs <https://jeffbridwell.com/chorus#RetireMeDup>"), "{out}");
    assert!(out.contains("class retirement executed — Widget removed from <urn:chorus:domains:awareness> (3 rows)"), "{out}");
    assert!(out.contains("graph retirement executed — graph <urn:test:RetireMe-graph> dropped (2 triple(s)"), "{out}");
    assert!(out.contains("graph <urn:test:empty-graph> already empty"), "{out}");
    assert!(stub.bodies().iter().any(|l| l.contains("DROP GRAPH <urn:test:RetireMe-graph>")));
    let backups = std::fs::read_dir(scratch_path("retire").join("backups")).unwrap().count();
    assert!(backups >= 3, "every guarded delete writes its backup first ({backups})");
}

fn scratch_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("deploy-run-4755-{tag}-{}", std::process::id()))
}

/// A stub that answers like `healthy`, except where `over` says otherwise.
fn with(over: impl Fn(&Req, &[Req]) -> Option<(u16, String)> + Send + Sync + 'static) -> Answer {
    Arc::new(move |r: &Req, e: &[Req]| over(r, e).unwrap_or_else(|| healthy(r, e)))
}

fn refused(tag: &str, answer: Answer, extra: &[(&str, &str)], model: &str, retirements: &str, want: &str) -> (String, Stub) {
    let (code, out, err, stub) = partial(tag, answer, extra, model, retirements);
    assert_ne!(code, 0, "{tag}: a refusal exited 0\n{out}\n{err}");
    assert!(err.contains(want) || out.contains(want), "{tag}: wanted {want:?}\n{out}\n{err}");
    (err, stub)
}

fn is_update(r: &Req, needle: &str) -> bool {
    r.method == "POST" && r.target.contains("/update") && r.body.contains(needle)
}

#[test]
fn a_store_that_refuses_the_merge_fails_the_deploy_and_drops_staging() {
    let (_, stub) = refused("merge", with(|r, _| is_update(r, "INSERT").then(|| (500, String::new()))), &[], MODEL, "", "merge-http-500");
    assert!(stub.bodies().iter().any(|l| l.starts_with("DELETE /test/data?graph=urn:test:ontology-staging-deploy")));
}

/// NEGATIVE PROOF: a verify the store does not answer is a refusal, never a pass.
#[test]
fn an_unanswered_verify_is_refused() {
    refused("verify-none", with(|r, _| r.body.contains("FILTER NOT EXISTS { { GRAPH").then(|| (500, "boom".into()))), &[], MODEL, "", "verify-unanswered");
}

#[test]
fn staged_subjects_missing_after_the_merge_are_refused() {
    refused("verify-missing", with(|r, _| r.body.contains("FILTER NOT EXISTS { { GRAPH").then(|| (200, "n\r\n4\r\n".into()))), &[], MODEL, "", "verify-missing-4");
}

#[test]
fn a_failed_staging_load_names_the_store_answer() {
    refused("stage", with(|r, _| (r.method == "POST" && r.target.contains("/data?graph=")).then(|| (503, String::new()))), &[], MODEL, "", "staging-load-http-503");
}

/// A 401 with no credential configured names the missing credential, not the store.
#[test]
fn a_401_with_no_credential_names_the_credential() {
    refused("stage401", with(|r, _| (r.method == "POST" && r.target.contains("/data?graph=")).then(|| (401, String::new()))), &[], MODEL, "", "staging-load");
}

#[test]
fn a_stamp_the_store_will_not_take_fails_loud() {
    refused("stamp", with(|r, _| is_update(r, "deployedFromCommit").then(|| (500, String::new()))), &[], MODEL, "", "stamp-write-failed-http-500");
}

#[test]
fn a_failed_sh_node_flatten_fails_the_deploy() {
    refused("flatten", with(|r, _| is_update(r, "rdfs:subClassOf* ?anc").then(|| (500, String::new()))), &[], MODEL, "", "sh-node-flatten-failed-http-500");
}

#[test]
fn empty_property_nodes_after_the_write_are_refused() {
    refused("empties", with(|r, _| (r.body.starts_with("query=") && r.body.contains("FILTER NOT EXISTS { ?b ?p ?o }")).then(|| (200, "n\r\n2\r\n".into()))), &[], MODEL, "", "empty-property-nodes");
}

#[test]
fn a_malformed_retirement_line_refuses_the_deploy() {
    refused("malformed", Arc::new(healthy), &[], MODEL, "not json\n", "retirement-staging-malformed:line-1");
}

#[test]
fn a_claim_on_a_class_served_right_now_is_refused() {
    let line = r#"{"subject_domain": "things", "object_class": "Gadget", "status": "staged"}"#;
    refused("served", Arc::new(healthy), &[], MODEL, line, "retirement-claim-served-at-execute");
}

#[test]
fn an_unanswered_serve_check_defers_claims_and_classes() {
    let lines = format!("{}\n{}\n",
        r#"{"subject_domain": "things", "object_class": "Widget", "status": "staged"}"#,
        r#"{"retire_class": "https://jeffbridwell.com/chorus#Widget", "status": "staged"}"#);
    let (code, out, err, _) = partial("defer", with(|r, _| r.target.contains("__model_deploy_probe__").then(|| (200, String::new()))), &[], MODEL, &lines);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(err.contains("serve-check UNANSWERED"), "{err}");
    assert!(err.contains("athena-make gave no route list"), "{err}");
}

#[test]
fn a_claimed_class_is_not_deleted() {
    let line = r#"{"retire_class": "https://jeffbridwell.com/chorus#Widget", "status": "staged"}"#;
    refused("claimed", with(|r, _| r.body.contains("definesVocabulary> <https://jeffbridwell.com/chorus#Widget>").then(|| (200, "n\r\n1\r\n".into()))), &[], MODEL, line, "class-retirement-claimed:1");
}

#[test]
fn a_class_with_inbound_edges_is_not_deleted() {
    let line = r#"{"retire_class": "https://jeffbridwell.com/chorus#Widget", "status": "staged"}"#;
    refused("inbound", with(|r, _| r.body.contains("GRAPH ?g2 { ?x ?p ?s }").then(|| (200, "n\r\n5\r\n".into()))), &[], MODEL, line, "class-retirement-has-consumers:5");
}

#[test]
fn a_class_delete_that_did_not_take_names_its_survivors() {
    let line = r#"{"retire_class": "https://jeffbridwell.com/chorus#Widget", "status": "staged"}"#;
    let (err, _) = refused("survive", with(|r, _| {
        if r.body.starts_with("query=SELECT (COUNT(DISTINCT ?s)") && r.body.contains("?s a <https://jeffbridwell.com/chorus#Widget> }") {
            return Some((200, "n\r\n3\r\n".into()));
        }
        r.body.contains("LIMIT 10").then(|| (200, r#"{"results":{"bindings":[{"s":{"value":"https://jeffbridwell.com/chorus#w1"}}]}}"#.into()))
    }), &[], MODEL, line, "class-retirement-did-not-take");
    assert!(err.contains("survivors: https://jeffbridwell.com/chorus#w1"), "{err}");
}

#[test]
fn unanswered_class_checks_defer_rather_than_delete() {
    let line = r#"{"retire_class": "https://jeffbridwell.com/chorus#Widget", "status": "staged"}"#;
    for (tag, needle, want) in [
        ("claimq", "definesVocabulary> <https://jeffbridwell.com/chorus#Widget>", "claim check did not answer"),
        ("inboundq", "GRAPH ?g2 { ?x ?p ?s }", "inbound-edge check did not answer"),
    ] {
        let n = needle.to_string();
        let (code, out, err, stub) = partial(tag, with(move |r, _| r.body.contains(&n).then(|| (500, "x".into()))), &[], MODEL, line);
        assert_eq!(code, 0, "{out}\n{err}");
        assert!(err.contains(want), "{err}");
        assert!(!stub.bodies().iter().any(|l| l.contains("?s a <https://jeffbridwell.com/chorus#Widget> ; ?p ?o")), "{tag}: deleted blind");
    }
}

#[test]
fn a_class_delete_the_store_rejects_is_refused() {
    let line = r#"{"retire_class": "https://jeffbridwell.com/chorus#Widget", "status": "staged"}"#;
    refused("classhttp", with(|r, _| (r.target.contains("/update") && r.body.contains("?s a <https://jeffbridwell.com/chorus#Widget> ; ?p ?o")).then(|| (500, String::new()))), &[], MODEL, line, "class-retirement-http-500");
}

#[test]
fn retirement_asks_that_go_unanswered_or_stay_true_are_refused() {
    let line = r#"{"triple_subject": "https://jeffbridwell.com/chorus#a", "triple_predicate": "https://jeffbridwell.com/chorus#b", "triple_object": "https://jeffbridwell.com/chorus#c", "status": "staged"}"#;
    refused("asknone", with(|r, _| r.body.starts_with("query=ASK").then(|| (200, "?".into()))), &[], MODEL, line, "retirement-ask-unanswered");
    refused("askstays", with(|r, _| r.body.starts_with("query=ASK").then(|| (200, r#"{"boolean": true}"#.into()))), &[], MODEL, line, "retirement-still-present");
    refused("askdel", with(|r, _| is_update(r, "DELETE DATA").then(|| (500, String::new()))), &[], MODEL, line, "retirement-delete-http-500");
    let verify_blind = with(|r, e| (r.body.starts_with("query=ASK") && e.iter().any(|x| x.body.contains("DELETE DATA"))).then(|| (200, "?".into())));
    refused("askverify", verify_blind, &[], MODEL, line, "retirement-verify-unanswered");
    let bad = r#"{"triple_subject": "not an iri", "triple_predicate": "x:y", "triple_object": "x:z", "status": "staged"}"#;
    refused("askbad", Arc::new(healthy), &[], MODEL, bad, "retirement-entry-empty");
    let same = r#"{"merge_subject": "https://jeffbridwell.com/chorus#A", "into": "https://jeffbridwell.com/chorus#A", "status": "staged"}"#;
    refused("mergesame", Arc::new(healthy), &[], MODEL, same, "merge-entry-needs-from-and-a-different-into");
}

#[test]
fn a_guarded_delete_with_a_short_backup_or_a_rejected_delete_is_refused() {
    let line = r#"{"retire_graph": "urn:test:RetireMe-graph", "status": "staged"}"#;
    refused("short", with(|r, _| r.body.starts_with("query=CONSTRUCT").then(|| (200, "<urn:a> <urn:b> <urn:c> .\n".into()))), &[], MODEL, line, "retire-refused");
    refused("blindcount", with(|r, _| (r.body.starts_with("query=SELECT (COUNT(*)") && r.body.contains("RetireMe")).then(|| (500, "x".into()))), &[], MODEL, line, "could not count");
    refused("gdhttp", with(|r, _| is_update(r, "DROP GRAPH").then(|| (500, String::new()))), &[], MODEL, line, "retire-delete-http-500");
}

#[test]
fn a_model_that_names_a_role_as_owner_is_refused_before_any_write() {
    let bad = format!("{MODEL}chorus:Widget chorus:ownedBy chorus:role-kade .\n");
    let (_, stub) = refused("owner", Arc::new(healthy), &[], &bad, "", "source-authors-role-owner:1");
    assert!(stub.bodies().is_empty(), "wrote before refusing");
}

#[test]
fn a_property_declared_twice_is_refused_before_any_write() {
    let bad = format!("{MODEL}chorus:size a owl:DatatypeProperty ; rdfs:domain chorus:Widget .\nchorus:size a owl:DatatypeProperty ; rdfs:domain chorus:Gadget .\n");
    let (_, stub) = refused("dupe", Arc::new(healthy), &[], &bad, "", "duplicate-property-declaration");
    assert!(stub.bodies().is_empty(), "wrote before refusing");
}

#[test]
fn turtle_riot_rejects_is_refused() {
    refused("riot", Arc::new(healthy), &[], "this is not turtle .\n", "", "riot-invalid");
}

#[test]
fn an_absent_validator_refuses_unless_unvalidated_is_allowed_loudly() {
    refused("noriot", Arc::new(healthy), &[("RIOT_BIN", "no-such-riot-4755")], MODEL, "", "riot-absent");
    let (code, out, err, _) = partial("noriot-ok", Arc::new(healthy), &[("RIOT_BIN", "no-such-riot-4755"), ("ALLOW_UNVALIDATED", "1")], MODEL, "");
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(err.contains("deploying UNVALIDATED TTL"), "{err}");
}

#[test]
fn a_ttl_that_does_not_exist_is_refused() {
    let root = scratch("missing");
    let stub = Stub::start(Arc::new(healthy));
    let gone = root.join("gone.ttl").display().to_string();
    let (code, _, err) = deploy(&stub, &root, &[], &[("TTL", gone.as_str())]);
    assert_eq!(code, 1);
    assert!(err.contains("ttl-not-found"), "{err}");
}

/// NEGATIVE PROOF for the #3536 guard: RETIRE_ABSENT with no Domain in staging
/// would delete every live Domain; it refuses instead.
#[test]
fn retire_absent_with_an_empty_staging_graph_is_refused() {
    refused("absent", Arc::new(healthy), &[("RETIRE_ABSENT", "1")], MODEL, "", "retire-guard-empty-staging");
    let (code, out, err, stub) = partial("absent-ok", with(|r, _| r.body.contains("FILTER(?t = <https://jeffbridwell.com/chorus#Domain>)").then(|| (200, "n\r\n3\r\n".into()))), &[("RETIRE_ABSENT", "1")], MODEL, "");
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(stub.bodies().iter().any(|l| l.contains("a <https://jeffbridwell.com/chorus#Domain> ; ?p ?o")), "retire clause not sent");
}

#[test]
fn the_shacl_report_has_three_distinguishable_states() {
    let (_, _, err, _) = partial("shacl-absent", Arc::new(healthy), &[("SHACL_REPORT", "1"), ("SHACL_BIN", "no-such-shacl-4755")], MODEL, "");
    assert!(err.contains("SHACL report SKIPPED"), "{err}");
    let (_, _, err, _) = partial("shacl-crash", Arc::new(healthy), &[("SHACL_REPORT", "1"), ("SHACL_BIN", "false")], MODEL, "");
    assert!(err.contains("SHACL validator CRASHED"), "{err}");
}

#[test]
fn a_core_stamp_pass_that_makes_no_progress_is_refused() {
    refused("stamps", with(|r, _| (r.body.starts_with("query=") && r.body.contains("SELECT (COUNT(*) AS ?n)") && r.body.contains("dct:creator")).then(|| (200, "n\r\n7\r\n".into()))), &[], MODEL, "", "made no progress");
}

#[test]
fn a_bare_run_inside_a_werk_with_no_store_named_is_refused() {
    // cwd is this crate, inside a werk; no FUSEKI_GSP. The target check exits
    // before any store call either way.
    let root = scratch("werk");
    let o = Command::new(env!("CARGO_BIN_EXE_athena-deploy"))
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", &root)
        .env("ATHENA_DEPLOY_TARGET_CHECK_ONLY", "1")
        .envs(std::env::var("LLVM_PROFILE_FILE").map(|p| ("LLVM_PROFILE_FILE", p)))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    let want = if env!("CARGO_MANIFEST_DIR").contains("/chorus-werk/") { 78 } else { 0 };
    assert_eq!(o.status.code(), Some(want), "{}", String::from_utf8_lossy(&o.stderr));
    let stub = Stub::start(Arc::new(healthy));
    let (code, out, _) = deploy(&stub, &root, &[], &[("ATHENA_DEPLOY_TARGET_CHECK_ONLY", "1")]);
    assert_eq!(code, 0);
    assert!(out.starts_with("target-check: ok gsp="), "{out}");
    assert!(stub.bodies().is_empty());
}

// ---- the subcommands main.rs routes before any deploy ----

/// athena-make's discovery document at "/", the Domain count at the store.
fn door(collections: &'static [&'static str]) -> Answer {
    Arc::new(move |r: &Req, _: &[Req]| {
        if r.method == "GET" && r.target == "/" {
            let items: Vec<String> = collections.iter().map(|c| format!(r#"{{"collection": "{c}"}}"#)).collect();
            return (200, format!(r#"{{"primitives": [{}]}}"#, items.join(", ")));
        }
        (200, "n\r\n5\r\n".into())
    })
}

#[test]
fn served_snapshot_then_compare_refuses_a_dropped_route_unless_allowed() {
    let root = scratch("served");
    let snap = root.join("before.txt");
    let snap_s = snap.display().to_string();
    let two = Stub::start(door(&["/v1/things/widgets", "/v1/things/gadgets"]));
    let (code, out, err) = deploy(&two, &root, &["served-snapshot", &two.url, &snap_s], &[("ATHENA_SERVED_TRIES", "1")]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("served-snapshot: 2 route(s), domains 5"), "{out}");

    let (code, out, err) = deploy(&two, &root, &["served-compare", &snap_s, &two.url], &[("ATHENA_SERVED_TRIES", "1")]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("nothing served before is missing"), "{out}");

    // NEGATIVE PROOF: the door now serves one route fewer.
    let one = Stub::start(door(&["/v1/things/widgets"]));
    let (code, _, err) = deploy(&one, &root, &["served-compare", &snap_s, &one.url], &[("ATHENA_SERVED_TRIES", "1")]);
    assert_eq!(code, 1);
    assert!(err.contains("route gone: /v1/things/gadgets"), "{err}");

    let (code, out, _) = deploy(&one, &root, &["served-compare", &snap_s, &one.url],
        &[("ATHENA_SERVED_TRIES", "1"), ("ATHENA_ALLOW_DROPS", "route gone: /v1/things/gadgets")]);
    assert_eq!(code, 0);
    assert!(out.contains("allowed by ATHENA_ALLOW_DROPS"), "{out}");
}

#[test]
fn a_door_with_no_collections_is_unmeasured_and_a_missing_snapshot_refuses() {
    let root = scratch("served-none");
    let none = Stub::start(door(&[]));
    let out_s = root.join("snap.txt").display().to_string();
    let (code, _, err) = deploy(&none, &root, &["served-snapshot", &none.url, &out_s], &[("ATHENA_SERVED_TRIES", "1")]);
    assert_eq!(code, 1);
    assert!(err.contains("lists no collection"), "{err}");
    let gone = root.join("no-before.txt").display().to_string();
    let (code, _, err) = deploy(&none, &root, &["served-compare", &gone, &none.url], &[]);
    assert_eq!(code, 1);
    assert!(err.contains("no before snapshot"), "{err}");
    let (code, _, err) = deploy(&none, &root, &["served-compare", &out_s, &none.url], &[("ATHENA_SERVED_TRIES", "1")]);
    assert_eq!(code, 1, "{err}");
}

/// An argument the binary does not know is refused, never read as "deploy everything".
#[test]
fn unknown_arguments_and_short_subcommands_refuse_without_a_store_call() {
    let root = scratch("args");
    let stub = Stub::start(Arc::new(healthy));
    for (args, want) in [
        (&["served-snapshot"][..], "usage: athena-deploy served-snapshot"),
        (&["served-compare", "x"][..], "usage: athena-deploy served-compare"),
        (&["scope"][..], "usage: athena-deploy scope"),
        (&["prove-trace"][..], "usage: athena-deploy prove-trace"),
        (&["staging", "sideways"][..], "usage: athena-deploy staging"),
        (&["sideways", "--card", "5"][..], "usage: athena-deploy --card"),
        (&["deploy-everything"][..], "unknown argument 'deploy-everything'"),
    ] {
        let (code, _, err) = deploy(&stub, &root, args, &[]);
        assert_eq!(code, 2, "{args:?}: {err}");
        assert!(err.contains(want), "{args:?}: {err}");
    }
    assert!(stub.bodies().is_empty(), "a refused argument reached the store");
}

#[test]
fn scope_and_prove_trace_run_from_the_binary() {
    let root = scratch("scope");
    let stub = Stub::start(Arc::new(healthy));
    let (code, _, err) = deploy(&stub, &root, &["scope", root.to_str().unwrap(), "HEAD~1..HEAD"], &[]);
    assert_eq!(code, 2, "not a git tree: {err}");
    let spine = root.join("spine.log");
    write(&spine, "");
    let (code, _, err) = deploy(&stub, &root, &["prove-trace", "t-4755", "1", "--spine", spine.to_str().unwrap()], &[]);
    assert_eq!(code, 1, "{err}");
}

// ---- a full run: the whole model set, then the domain sets from the manifest ----

/// Every model-set file as one small class, and a manifest of two sets: a
/// vocabulary that replaces its graph and a set merged into the domains graph.
fn full_tree(tag: &str, domains_ttl: &str) -> PathBuf {
    let root = scratch(tag);
    let r = root.display().to_string();
    for (i, f) in athena_deploy::model_set(&r, None).iter().enumerate() {
        write(Path::new(f), &format!("{MODEL}chorus:Class{i} a owl:Class .\n"));
    }
    write(&root.join("sets/vocab.ttl"), "@prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
        <urn:test:term-a> a skos:Concept ; skos:prefLabel \"card\" ; skos:inScheme <urn:test:scheme> .\n\
        <urn:test:term-b> a skos:Concept ; skos:prefLabel \"card\" ; skos:inScheme <urn:test:scheme> .\n");
    write(&root.join("sets/domains.ttl"), domains_ttl);
    write(&root.join("platform/config/domain-set-manifest.txt"),
        "# test manifest\nvocab|urn:test:vocab|sets/vocab.ttl|replace\ndomains|urn:chorus:domains:domains|sets/domains.ttl\n");
    write(&root.join("retirements.jsonl"), "");
    root
}

const DOMAIN_ROW: &str = "@prefix chorus: <https://jeffbridwell.com/chorus#> .\n<urn:test:row> a chorus:Widget .\n";

/// A validator that accepts every file and yields no triples: the full-run
/// legs below are about the store's answers, not Turtle, and a JVM per file
/// per run is a minute of wall clock. The clobber test uses the real riot.
fn quick_riot(root: &Path) -> String {
    let p = root.join("bin/riot-ok");
    write(&p, "#!/bin/sh\nexit 0\n");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p.display().to_string()
}

#[test]
fn a_full_run_deploys_the_model_set_then_every_domain_set() {
    let root = full_tree("full", DOMAIN_ROW);
    let riot = quick_riot(&root);
    let stub = Stub::start(Arc::new(healthy));
    let (code, out, err) = deploy(&stub, &root, &[], &[("SHACL_BIN", "no-such-shacl-4755"), ("RIOT_BIN", &riot)]);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains("vocab: 1 file(s) -> <urn:test:vocab>"), "{out}");
    assert!(out.contains("domains: 1 file(s) -> <urn:chorus:domains:domains>"), "{out}");
    assert!(err.contains("vocabulary label collisions:"), "{err}");
    let log = stub.bodies();
    assert!(log.iter().any(|l| l.starts_with("DELETE /test/data?graph=urn:test:vocab ")), "replace set did not empty its graph");
    assert!(!log.iter().any(|l| l.starts_with("DELETE /test/data?graph=urn:chorus:domains:domains ")), "a merge set emptied its graph");
}

#[test]
fn a_domain_set_leg_refuses_on_each_bad_store_answer() {
    for (tag, needle, code, body, want) in [
        ("set-stage", "/data?graph=urn:test:vocab-staging-deploy", 500, "", "vocab-staging-http-500"),
        ("set-replace", "/data?graph=urn:test:vocab", 500, "", "vocab-replace-delete-http-500"),
    ] {
        let root = full_tree(tag, DOMAIN_ROW);
        let n = needle.to_string();
        let stub = Stub::start(with(move |r, _| (r.target.ends_with(&n) && r.method != "GET" && !(n.ends_with("vocab") && r.method == "POST")).then(|| (code, body.to_string()))));
        let riot = quick_riot(&root);
        let (c, out, err) = deploy(&stub, &root, &[], &[("SHACL_BIN", "no-such-shacl-4755"), ("RIOT_BIN", &riot)]);
        assert_eq!(c, 1, "{tag}: {out}\n{err}");
        assert!(err.contains(want), "{tag}: {err}");
    }
    let root = full_tree("set-verify", DOMAIN_ROW);
    let riot = quick_riot(&root);
    let stub = Stub::start(with(|r, _| r.body.contains("FILTER NOT EXISTS { GRAPH <urn:test:vocab> {").then(|| (200, "n\r\n2\r\n".into()))));
    let (_, _, err) = deploy(&stub, &root, &[], &[("SHACL_BIN", "no-such-shacl-4755"), ("RIOT_BIN", &riot)]);
    assert!(err.contains("vocab-verify-missing-2"), "{err}");
    let root = full_tree("set-merge", DOMAIN_ROW);
    let riot = quick_riot(&root);
    let stub = Stub::start(with(|r, _| (is_update(r, "urn:test:vocab-staging-deploy")).then(|| (500, String::new()))));
    let (_, _, err) = deploy(&stub, &root, &[], &[("SHACL_BIN", "no-such-shacl-4755"), ("RIOT_BIN", &riot)]);
    assert!(err.contains("vocab-merge-http-500"), "{err}");
}

/// NEGATIVE PROOF for #4338: a domains-graph set naming a Domain the model set
/// declares would wipe that row; refused before any write.
#[test]
fn a_domain_set_that_names_a_model_domain_is_refused() {
    let root = full_tree("clobber", "@prefix chorus: <https://jeffbridwell.com/chorus#> .\nchorus:things chorus:label \"x\" .\n");
    let first = athena_deploy::model_set(&root.display().to_string(), None)[0].clone();
    write(Path::new(&first), &format!("{MODEL}chorus:things a chorus:Domain .\n"));
    let stub = Stub::start(Arc::new(healthy));
    let (code, _, err) = deploy(&stub, &root, &[], &[]);
    assert_eq!(code, 1);
    assert!(err.contains("domain-set-names-domain:1"), "{err}");
    assert!(stub.bodies().is_empty(), "wrote before refusing");
}

#[test]
fn a_missing_manifest_is_refused_not_read_as_no_sets() {
    let root = full_tree("nomanifest", DOMAIN_ROW);
    std::fs::remove_file(root.join("platform/config/domain-set-manifest.txt")).unwrap();
    let riot = quick_riot(&root);
    let stub = Stub::start(Arc::new(healthy));
    let (code, _, err) = deploy(&stub, &root, &[], &[("RIOT_BIN", &riot)]);
    assert_eq!(code, 1);
    assert!(err.contains("domain-set-manifest-unreadable"), "{err}");
}

fn git(root: &Path, args: &[&str]) -> String {
    let o = Command::new("git").arg("-C").arg(root)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid", "-c", "commit.gpgsign=false"])
        .args(args).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}

/// NEGATIVE PROOF for #4125: a subject deleted from source since the store's
/// stamped commit refuses the deploy; staged for retirement, it passes.
#[test]
fn a_subject_deleted_since_the_stamped_commit_is_refused_unless_staged() {
    let root = full_tree("srcdel", DOMAIN_ROW);
    git(&root, &["init", "-q"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "model"]);
    let sha = git(&root, &["rev-parse", "HEAD"]);
    let first = athena_deploy::model_set(&root.display().to_string(), None)[0].clone();
    write(Path::new(&first), MODEL);
    let riot = quick_riot(&root);
    let quick = [("SHACL_BIN", "no-such-shacl-4755"), ("RIOT_BIN", riot.as_str())];
    let stamped = move || -> Answer {
        let sha = sha.clone();
        with(move |r, _| r.body.contains("deployedFromCommit> ?c").then(|| (200, format!("c\r\n{sha}\r\n"))))
    };
    let stub = Stub::start(stamped());
    let (code, _, err) = deploy(&stub, &root, &[], &quick);
    assert_eq!(code, 1);
    assert!(err.contains("source-deleted-subjects:1"), "{err}");
    assert!(err.contains("chorus:Class0"), "{err}");

    write(&root.join("retirements.jsonl"), r#"{"retire_subject": "https://jeffbridwell.com/chorus#Class0", "status": "staged"}"#);
    let stub = Stub::start(stamped());
    let (code, out, err) = deploy(&stub, &root, &[], &quick);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains("stamped deployedFromCommit="), "{out}");

    let stub = Stub::start(with(|r, _| r.body.contains("deployedFromCommit> ?c").then(|| (200, "c\r\nabcdef1234567\r\n".into()))));
    let (_, _, err) = deploy(&stub, &root, &[], &quick);
    assert!(err.contains("is not in this tree"), "{err}");
}
