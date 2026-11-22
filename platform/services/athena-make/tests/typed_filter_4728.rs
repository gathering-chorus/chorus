// @test-type: integration — boots a scratch Fuseki (the live jar, in memory, a free port); never touches :3030.
// @domain: persistence
// @card: #4728
// @owner: silas
// wipe-guard: exempt — every write goes to the scratch Fuseki this file boots on a free port, never :3030
//! #4728 — src/lib.rs `collection_filter`: since #4717 a field's literal carries
//! its shape's datatype, so `?readingDay=2026-10-24` must find
//! "2026-10-24"^^xsd:date as well as a plain "2026-10-24". Run against real Jena.
//! NEGATIVE: the old exact plain-literal pattern finds none of the typed rows.
use athena_make::{collection_filter, collection_page_query_filtered};
use std::process::Command;

const C: &str = "https://jeffbridwell.com/chorus#";
const G: &str = "urn:chorus:domains:heralds";

/// #4925 (Silas) — the boot says WHY it failed. It returned None for every failure and
/// the panic named the prerequisites, so the 08:21 nightly red ("no scratch Fuseki") read
/// as a missing jar or java when both were present (the test passes by hand in 3 s).
/// Fuseki's stderr now goes to <dir>/fuseki.err and a timeout quotes its tail; the wait is
/// 120 s (240 x 500 ms), not 30, for a JVM cold start beside the nightly's parallel suites.
fn boot() -> Result<(std::process::Child, u16, std::path::PathBuf), String> {
    let home = std::path::PathBuf::from(std::env::var("HOME").map_err(|_| "HOME unset".to_string())?).join(".gathering/data/fuseki-5.1.0");
    boot_with(home, std::env::var("JAVA").unwrap_or_else(|_| "/opt/homebrew/opt/openjdk/bin/java".into()))
}

fn boot_with(home: std::path::PathBuf, java: String) -> Result<(std::process::Child, u16, std::path::PathBuf), String> {
    let jar = home.join("fuseki-server.jar");
    if !jar.exists() { return Err(format!("jar missing at {}", jar.display())); }
    if !std::path::Path::new(&java).exists() { return Err(format!("java missing at {java}")); }
    let port = std::net::TcpListener::bind("127.0.0.1:0").and_then(|l| l.local_addr()).map_err(|e| format!("no free port: {e}"))?.port();
    let dir = std::env::temp_dir().join(format!("athena-make-4728-{}-{port}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot make {}: {e}", dir.display()))?;
    std::fs::write(dir.join("config.ttl"), "@prefix fuseki: <http://jena.apache.org/fuseki#> .\n@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n@prefix ja: <http://jena.hpl.hp.com/2005/11/Assembler#> .\n[] rdf:type fuseki:Server ; fuseki:services ( <#s> ) .\n<#s> a fuseki:Service ; fuseki:name \"t\" ; fuseki:endpoint [ fuseki:operation fuseki:query ; fuseki:name \"query\" ] ; fuseki:endpoint [ fuseki:operation fuseki:update ; fuseki:name \"update\" ] ; fuseki:dataset [ a ja:MemoryDataset ] .\n")
        .map_err(|e| format!("cannot write config: {e}"))?;
    let err_path = dir.join("fuseki.err");
    let err_file = std::fs::File::create(&err_path).map_err(|e| format!("cannot open {}: {e}", err_path.display()))?;
    let jar_s = jar.to_str().ok_or("jar path is not utf-8")?.to_string();
    let mut child = Command::new(&java).current_dir(&dir).env("FUSEKI_BASE", &dir).env("FUSEKI_HOME", &home)
        .args(["-Xmx256m", "-cp", &jar_s, "org.apache.jena.fuseki.cmd.FusekiCmd", "--config=config.ttl", &format!("--port={port}")])
        .stdout(std::process::Stdio::null()).stderr(err_file).spawn().map_err(|e| format!("spawn error {e}"))?;
    // #4925 round 2 (review) — a real 120 s deadline (each ping may take its own second),
    // and a Fuseki that never answered is stopped, not left running after the test
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    while std::time::Instant::now() < deadline {
        if Command::new("curl").args(["-sf", "-o", "/dev/null", "--max-time", "1", &format!("http://127.0.0.1:{port}/$/ping")]).status().is_ok_and(|s| s.success()) {
            return Ok((child, port, dir));
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    let _ = child.kill();
    let _ = child.wait();
    let tail: String = std::fs::read_to_string(&err_path).unwrap_or_default().lines().rev().take(5).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" | ");
    Err(format!("no ping on :{port} after 120 s; last fuseki stderr: {}", if tail.is_empty() { "(empty)" } else { &tail }))
}

/// #4925 NEGATIVE PROOF (Silas): a missing java names java, never "no scratch Fuseki".
#[test]
fn negative_proof_a_boot_failure_names_its_real_reason() {
    let home = std::env::temp_dir().join(format!("athena-make-4728-neg-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("fuseki-server.jar"), b"").unwrap();
    let why = boot_with(home.clone(), "/nonexistent/java".into()).err().expect("a boot with no java succeeded");
    assert_eq!(why, "java missing at /nonexistent/java");
    let why = boot_with(home.join("nowhere"), "/nonexistent/java".into()).err().unwrap();
    assert!(why.starts_with("jar missing at "), "{why}");
    let _ = std::fs::remove_dir_all(&home);
}

fn rows(port: u16, q: &str) -> Vec<String> {
    let out = Command::new("curl").args(["-sf", "--max-time", "10", "-H", "Accept: text/tab-separated-values", "--data-urlencode", &format!("query={q}"), &format!("http://127.0.0.1:{port}/t/query")]).output().unwrap();
    String::from_utf8_lossy(&out.stdout).lines().skip(1).filter(|l| !l.is_empty()).map(|l| l.trim_matches('"').to_string()).collect()
}

#[test]
fn a_day_filter_finds_typed_and_plain_readings_and_the_old_pattern_does_not() {
    // #3734: a proof that cannot run is a failure, never a pass (Abby/Kade 10-24)
    let (mut child, port, dir) = boot().unwrap_or_else(|why| panic!("scratch Fuseki did not start: {why}"));
    let xsd = "http://www.w3.org/2001/XMLSchema#";
    let up = format!("INSERT DATA {{ GRAPH <{G}> {{ \
        <{C}r-typed> a <{C}SensorReading> ; <{C}readingDay> \"2026-10-24\"^^<{xsd}date> ; <{C}readingValue> \"4\"^^<{xsd}decimal> . \
        <{C}r-plain> a <{C}SensorReading> ; <{C}readingDay> \"2026-10-24\" . \
        <{C}r-other> a <{C}SensorReading> ; <{C}readingDay> \"2026-10-23\"^^<{xsd}date> }} }}");
    Command::new("curl").args(["-sf", "--max-time", "10", "--data-urlencode", &format!("update={up}"), &format!("http://127.0.0.1:{port}/t/update")]).status().unwrap();
    let fields = vec![("readingDay".to_string(), false), ("readingValue".to_string(), false)];
    let (f, _) = collection_filter("readingDay=2026-10-24", &fields).unwrap();
    let got = rows(port, &collection_page_query_filtered(G, &format!("{C}SensorReading"), &f, 100, 0));
    let (fv, _) = collection_filter("readingValue=4", &fields).unwrap();
    let got_v = rows(port, &collection_page_query_filtered(G, &format!("{C}SensorReading"), &fv, 100, 0));
    // the pattern the door used before #4728
    let old = format!(" . ?s <{C}readingDay> \"2026-10-24\"");
    let got_old = rows(port, &collection_page_query_filtered(G, &format!("{C}SensorReading"), &old, 100, 0));
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(got, vec![format!("{C}r-plain"), format!("{C}r-typed")], "the day filter");
    assert_eq!(got_v, vec![format!("{C}r-typed")], "a typed decimal by its text");
    assert_eq!(got_old, vec![format!("{C}r-plain")], "NEGATIVE: the old pattern misses the typed row");
}
