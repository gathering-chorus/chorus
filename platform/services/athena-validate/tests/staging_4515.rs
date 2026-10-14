// @test-type: unit
// @domain: athena
//! #4515 — `athena-validate --staging`, the athena pipeline's Validate step: it
//! sweeps the staging dataset the Deploy step loaded, never prod. Two fake
//! servers on loopback stand in for prod and staging; each records the paths it
//! was asked for. Nothing reaches a real store or athena-make.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};

fn bin() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let b = exe.parent().unwrap().parent().unwrap().join("athena-validate");
    assert!(b.is_file(), "athena-validate not built at {}", b.display());
    b
}

/// A loopback server answering 500 to everything, recording each request path.
fn recorder() -> (String, Arc<Mutex<Vec<String>>>) {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let s2 = seen.clone();
    std::thread::spawn(move || {
        for st in l.incoming().flatten() {
            let mut st = st;
            let mut buf = [0u8; 8192];
            let n = st.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let path = req.split_whitespace().nth(1).unwrap_or("").split('?').next().unwrap_or("").to_string();
            s2.lock().unwrap().push(path);
            let _ = write!(st, "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        }
    });
    (format!("http://{addr}"), seen)
}

/// Run a sweep with prod at one recorder and staging at the other.
fn sweep(args: &[&str]) -> (Vec<String>, Vec<String>, String) {
    let (prod, prod_seen) = recorder();
    let (stg, stg_seen) = recorder();
    let o = Command::new(bin())
        .args(args)
        .env("FUSEKI_QUERY", format!("{prod}/pods/query"))
        .env("CHORUS_OWL_API", &prod)
        .env("CHORUS_FUSEKI_BASE", &stg)
        .env("STAGING_DATASET", "staging-4515")
        .env("ATHENA_MAKE_STAGING_URL", &stg)
        .env("ATHENA_VALIDATE_NUDGE", "0")
        .env("ATHENA_VALIDATE_TIMEOUT", "5")
        .env_remove("ATHENA_VALIDATE_REPORT")
        .env_remove("ATHENA_VALIDATE_GRAPHS")
        .env_remove("XPC_SERVICE_NAME")
        .output()
        .unwrap();
    let out = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    let p = prod_seen.lock().unwrap().clone();
    let s = stg_seen.lock().unwrap().clone();
    (p, s, out)
}

#[test]
fn staging_sweeps_the_staging_dataset_and_door_and_never_prod() {
    let (prod, stg, out) = sweep(&["--staging"]);
    assert!(prod.is_empty(), "prod was asked {prod:?}\n{out}");
    assert!(stg.iter().any(|p| p == "/staging-4515/query"), "staging store not asked: {stg:?}\n{out}");
    assert!(out.lines().next().unwrap_or("").starts_with("graph-target|staging|"), "{out}");
}

#[test]
fn negative_proof_without_staging_the_sweep_reads_prod() {
    // the same harness can tell the two apart: a plain sweep goes to prod only
    let (prod, stg, out) = sweep(&[]);
    assert!(stg.is_empty(), "staging was asked {stg:?}\n{out}");
    assert!(prod.iter().any(|p| p == "/pods/query"), "prod store not asked: {prod:?}\n{out}");
}
