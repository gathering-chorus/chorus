// @domain: domains
// @test-type: integration — spawns athena-make serve and the generated principles-api against an in-test SPARQL stub; no Fuseki
//! #4995 (AC3) — the generated principles API and athena-make's door, served over the
//! same store, return the same principles field by field: same set, same values, same
//! owner links. NEGATIVE PROOF: the generated API reads a store with one row changed and
//! the comparison fails, naming that row.
//! The store is an in-memory oxigraph behind a std TcpListener on port 0 that speaks the
//! SPARQL 1.1 protocol both clients use (POST {endpoint}/query, raw or form body).
//! Covers shaping/domains/principles/api (generated) against platform/services/athena-make.
use serde_json::Value;
use std::net::TcpListener;
use std::path::Path;

mod stub_4995;
use stub_4995::{repo, serve, store};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Killer(Child);
impl Drop for Killer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// GET a URL until it answers 200 (the server is starting), at most 60 s.
fn get(url: &str) -> Value {
    let start = Instant::now();
    loop {
        let out = Command::new("curl").args(["-s", "-w", "\n%{http_code}", "--max-time", "10", url]).output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        if let Some((body, code)) = text.rsplit_once('\n') {
            if code == "200" {
                return serde_json::from_str(body).unwrap_or_else(|e| panic!("{url}: not JSON ({e}): {body}"));
            }
            assert!(start.elapsed() < Duration::from_secs(60), "{url}: last answer {code}: {body}");
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

/// athena-make: the binary under test is the one serving :3360 today unless ATHENA_MAKE_BIN
/// names another. A missing binary fails loudly, never skips.
fn athena_make(endpoint: &str) -> (Killer, String) {
    let bin = std::env::var("ATHENA_MAKE_BIN").unwrap_or_else(|_| "/Users/jeffbridwell/.chorus/bin/athena-make".into());
    assert!(Path::new(&bin).is_file(), "athena-make binary not found at {bin} (set ATHENA_MAKE_BIN)");
    let port = free_port();
    let child = Command::new(&bin).args(["serve", "--port", &port.to_string()])
        .env("CHORUS_FUSEKI", endpoint).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    (Killer(child), format!("http://127.0.0.1:{port}/v1/principles/principles"))
}

/// The generated principles API, built by werk-build in its crate home.
fn principles_api(endpoint: &str) -> (Killer, String) {
    let dir = repo().join("shaping/domains/principles/api");
    // #4555: one workspace, one target folder (CARGO_TARGET_DIR, else <repo>/target)
    let target = std::env::var("CARGO_TARGET_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| repo().join("target"));
    let bin = target.join("release/principles-api");
    // Borg Queen 18:29: until werk-build knows domains/<d>/api (this card), nothing else builds it
    if !bin.is_file() {
        let st = Command::new("cargo").args(["build", "--release", "--locked"]).current_dir(&dir).status().unwrap();
        assert!(st.success(), "cargo build of principles-api failed");
    }
    assert!(bin.is_file(), "principles-api not built at {}", bin.display());
    let port = free_port();
    let child = Command::new(&bin)
        .env("PRINCIPLES_STORE_ENDPOINT", endpoint).env("PRINCIPLES_SERVER_BIND", format!("127.0.0.1:{port}"))
        .env("PRINCIPLES_STORE_KIND", "sparql").env("PRINCIPLES_EVENTS_COMMAND", "")
        .stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    (Killer(child), format!("http://127.0.0.1:{port}/principles/principles"))
}

/// Every row as name → its fields, envelope metadata left out. Values compared as JSON.
fn rows(v: &Value) -> std::collections::BTreeMap<String, Value> {
    v["data"].as_array().unwrap_or_else(|| panic!("no data array: {v}")).iter()
        .map(|r| (r["name"].as_str().unwrap_or_else(|| panic!("row without a name: {r}")).to_string(), r.clone()))
        .collect()
}

/// The differences, each naming the row (and field): empty when equal.
fn differences(make: &Value, gen: &Value) -> Vec<String> {
    let (a, b) = (rows(make), rows(gen));
    let mut out = Vec::new();
    for name in a.keys().chain(b.keys()).collect::<std::collections::BTreeSet<_>>() {
        match (a.get(name), b.get(name)) {
            (Some(x), Some(y)) => {
                let keys: std::collections::BTreeSet<&String> = x.as_object().unwrap().keys().chain(y.as_object().unwrap().keys()).collect();
                for k in keys {
                    if x.get(k) != y.get(k) {
                        out.push(format!("{name}.{k}: athena-make {:?} vs generated {:?}", x.get(k), y.get(k)));
                    }
                }
            }
            (Some(_), None) => out.push(format!("{name}: only athena-make serves it")),
            (None, Some(_)) => out.push(format!("{name}: only the generated API serves it")),
            (None, None) => {}
        }
    }
    out
}

/// One test function: the servers are spawned once and the stub's threads are shared.
#[test]
fn the_generated_principles_api_equals_athena_makes_door() {
    let same = serve(store(None));
    let (_m, make_url) = athena_make(&same);
    let (_g, gen_url) = principles_api(&same);
    let (make, gen) = (get(&make_url), get(&gen_url));
    assert!(rows(&make).len() >= 10, "the door served too few principles to compare: {}", rows(&make).len());
    // Borg Queen 18:31: "same owner links" must not pass on rows that carry none
    let owned = rows(&gen).values().filter(|r| r["ownedBy"].as_str().is_some_and(|o| !o.is_empty())).count();
    assert!(owned == rows(&gen).len(), "every principle carries an owner; {owned} of {} do", rows(&gen).len());
    let d = differences(&make, &gen);
    assert!(d.is_empty(), "{} difference(s):\n{}", d.len(), d.join("\n"));

    // NEGATIVE PROOF: the generated API reads a store where one principle's statement differs
    let first = rows(&make).keys().next().unwrap().clone();
    let changed = serve(store(Some(&first)));
    let (_g2, gen2_url) = principles_api(&changed);
    let d = differences(&make, &get(&gen2_url));
    assert!(!d.is_empty() && d.iter().all(|x| x.starts_with(&format!("{first}."))), "one changed row must fail, naming {first}: {d:?}");
}
