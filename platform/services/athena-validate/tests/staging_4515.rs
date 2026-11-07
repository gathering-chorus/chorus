// @test-type: unit
// @domain: domains
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
        // #4536 — a test brings its own baseline home; the machine's must never decide it
        .env("ATHENA_VALIDATE_BASELINE", std::env::var("TEST_BASELINE_4536").unwrap_or_else(|_| "/nonexistent/baseline-4536".into()))
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

/// #4536 — under --staging with no baseline, the sweep still runs and the
/// verdict is UNMEASURED "no baseline", never a sweep-wide red.
#[test]
fn staging_with_no_baseline_is_unmeasured_never_red_4536() {
    let (_, stg, out) = sweep(&["--staging"]);
    assert!(stg.iter().any(|p| p == "/staging-4515/query"), "the sweep must still run: {out}");
    assert!(out.contains("graph-summary|UNMEASURED|no baseline at /nonexistent/baseline-4536"), "{out}");
    assert!(!out.contains("|dirty"), "no baseline must never read as dirty: {out}");
}

/// #4536 — the "before" step: a sweep that could not measure writes no
/// baseline and never reads as "recorded" (an unmeasured baseline would hide
/// gaps from the "after" step). NEGATIVE PROOF for the clean exit a written
/// baseline gets: here nothing is written, so the step is UNMEASURED.
#[test]
fn a_before_step_that_cannot_measure_writes_no_baseline_4536() {
    let dir = std::env::temp_dir().join(format!("athena-validate-4536-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("run.baseline");
    let p = path.to_str().unwrap().to_string();
    let (_, _, out) = sweep(&["--staging", "--write-baseline", &p]);
    assert!(!path.exists(), "an unmeasured sweep wrote a baseline: {out}");
    assert!(out.contains("graph-baseline|NOT WRITTEN"), "{out}");
    assert!(!out.contains("recorded, not judged"), "{out}");
    let _ = std::fs::remove_dir_all(&dir);
}
