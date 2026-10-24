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

fn boot() -> Option<(std::process::Child, u16, std::path::PathBuf)> {
    let home = std::path::PathBuf::from(std::env::var("HOME").ok()?).join(".gathering/data/fuseki-5.1.0");
    let jar = home.join("fuseki-server.jar");
    if !jar.exists() { return None; }
    let port = std::net::TcpListener::bind("127.0.0.1:0").ok()?.local_addr().ok()?.port();
    let dir = std::env::temp_dir().join(format!("athena-make-4728-{}-{port}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    std::fs::write(dir.join("config.ttl"), "@prefix fuseki: <http://jena.apache.org/fuseki#> .\n@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n@prefix ja: <http://jena.hpl.hp.com/2005/11/Assembler#> .\n[] rdf:type fuseki:Server ; fuseki:services ( <#s> ) .\n<#s> a fuseki:Service ; fuseki:name \"t\" ; fuseki:endpoint [ fuseki:operation fuseki:query ; fuseki:name \"query\" ] ; fuseki:endpoint [ fuseki:operation fuseki:update ; fuseki:name \"update\" ] ; fuseki:dataset [ a ja:MemoryDataset ] .\n").ok()?;
    let java = std::env::var("JAVA").unwrap_or_else(|_| "/opt/homebrew/opt/openjdk/bin/java".into());
    let child = Command::new(java).current_dir(&dir).env("FUSEKI_BASE", &dir).env("FUSEKI_HOME", &home)
        .args(["-Xmx256m", "-cp", jar.to_str()?, "org.apache.jena.fuseki.cmd.FusekiCmd", "--config=config.ttl", &format!("--port={port}")])
        .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().ok()?;
    for _ in 0..60 {
        if Command::new("curl").args(["-sf", "-o", "/dev/null", "--max-time", "1", &format!("http://127.0.0.1:{port}/$/ping")]).status().is_ok_and(|s| s.success()) {
            return Some((child, port, dir));
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    None
}

fn rows(port: u16, q: &str) -> Vec<String> {
    let out = Command::new("curl").args(["-sf", "--max-time", "10", "-H", "Accept: text/tab-separated-values", "--data-urlencode", &format!("query={q}"), &format!("http://127.0.0.1:{port}/t/query")]).output().unwrap();
    String::from_utf8_lossy(&out.stdout).lines().skip(1).filter(|l| !l.is_empty()).map(|l| l.trim_matches('"').to_string()).collect()
}

#[test]
fn a_day_filter_finds_typed_and_plain_readings_and_the_old_pattern_does_not() {
    // #3734: a proof that cannot run is a failure, never a pass (Abby/Kade 10-24)
    let (mut child, port, dir) = boot().expect("no scratch Fuseki: ~/.gathering/data/fuseki-5.1.0 and java are needed for this proof");
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
