// @test-type: unit
// @domain: services
//! #4747 swarm (Silas) — `chorus-runner call`, end to end in-process and
//! hermetic: the store is a local stub serving three Skill rows, the door is a
//! closed port, and the identity script is a fake. Covers
//! platform/services/chorus-runner/src/client.rs (run, args_from_pairs): the
//! call's exit code is the verb's, a row's timeout stops it, and each refusal
//! exits with its own code.
use chorus_runner::client::{args_from_pairs, run};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::sync::Mutex;

/// run() reads process env (ROLE, the store, the door), so calls take turns.
static ENV: Mutex<()> = Mutex::new(());

fn skill(name: &str, binary: &str, arguments: &str, timeout: u64) -> Vec<Value> {
    let s = format!("https://jeffbridwell.com/chorus#skill-{name}");
    let c = |p: &str| format!("https://jeffbridwell.com/chorus#{p}");
    let lit = |v: &str| json!({"type": "literal", "value": v});
    let uri = |v: &str| json!({"type": "uri", "value": c(v)});
    [
        ("executor", lit("deterministic")),
        ("inputShape", uri("WerkCardInput")),
        ("implementedBy", lit(binary)),
        ("arguments", lit(arguments)),
        ("ownedBy", uri("principal-kade")),
        ("runsAs", lit("caller")),
        ("timeoutSeconds", lit(&timeout.to_string())),
    ]
    .into_iter()
    .map(|(p, o)| json!({"s": {"type": "uri", "value": s}, "p": {"type": "uri", "value": c(p)}, "o": o}))
    .collect()
}

/// A store that answers every query with the same three Skill rows.
fn stub_store() -> String {
    let mut b = skill("zz-true-4747", "true", "{card}", 5);
    b.extend(skill("zz-false-4747", "false", "{card}", 5));
    b.extend(skill("zz-sleep-4747", "sleep", "5", 1));
    let body = json!({"head": {"vars": ["s", "p", "o"]}, "results": {"bindings": b}}).to_string();
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            let mut s = s;
            let mut buf = [0u8; 8192];
            let _ = s.read(&mut buf);
            let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/sparql-results+json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
        }
    });
    format!("http://127.0.0.1:{port}/pods")
}

fn setup() -> std::sync::MutexGuard<'static, ()> {
    let g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    static STORE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    std::env::set_var("CHORUS_FUSEKI", STORE.get_or_init(stub_store));
    std::env::set_var("CHORUS_DOOR", "http://127.0.0.1:9");
    let home = std::env::temp_dir().join(format!("client-run-4747-{}", std::process::id()));
    let scripts = home.join("platform/scripts");
    std::fs::create_dir_all(&scripts).unwrap();
    let mint = scripts.join("chorus-identity-token");
    std::fs::write(&mint, "#!/bin/sh\necho test-credential\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&mint, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::env::set_var("CHORUS_HOME", &home);
    std::env::remove_var("CHORUS_WERK_BASE");
    std::env::set_var("ROLE", "wren");
    g
}

#[test]
fn the_calls_exit_code_is_the_verbs() {
    let _g = setup();
    assert_eq!(run("zz-true-4747", &json!({"card": "1"})), 0);
}

/// NEGATIVE PROOF: a verb that fails makes the call fail; a green exit is not
/// what run() returns regardless.
#[test]
fn negative_proof_a_failing_verb_fails_the_call() {
    let _g = setup();
    assert_eq!(run("zz-false-4747", &json!({"card": "1"})), 1);
}

#[test]
fn a_verb_past_its_rows_timeout_is_stopped() {
    let _g = setup();
    let t = std::time::Instant::now();
    assert_eq!(run("zz-sleep-4747", &json!({"card": "1"})), 124);
    assert!(t.elapsed() < std::time::Duration::from_secs(4), "it waited for the verb, not the row's 1s");
}

#[test]
fn each_refusal_exits_with_its_own_code() {
    let _g = setup();
    assert_eq!(run("zz-no-such-tool-4747", &json!({})), 2, "no such tool");
    assert_eq!(run("zz-true-4747", &json!({})), 1, "missing required card");
    assert_eq!(run("athena-list", &json!({"collection": "/cards/cards"})), 1, "door down");
    std::env::remove_var("ROLE");
    std::env::remove_var("CHORUS_ROLE");
    assert_eq!(run("zz-true-4747", &json!({"card": "1"})), 2, "no ROLE to run as");
}

#[test]
fn a_key_given_three_times_is_a_list_of_three_in_order() {
    let v = args_from_pairs(&["k=a".into(), "k=b".into(), "k=c".into()]).unwrap();
    assert_eq!(v["k"], json!(["a", "b", "c"]));
}
