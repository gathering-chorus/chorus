// @test-type: integration — boots a scratch Fuseki on disk (the live jar, TDB2 like prod, a free port); never touches :3030 or the store.
// @domain: persistence
// @card: #4717
// @owner: silas
// wipe-guard: exempt — every write goes to a scratch TDB2 Fuseki this file boots on a free port (FusekiStore endpoint 127.0.0.1:<port>/t), never :3030
//! #4717 reopen — src/lib.rs `field_literal` / `canonical_decimal` against real
//! TDB2. What Jeff sees: a sensor reading set twice holds one value. On 10-24 a
//! reading held 7.0 and 8.0 (the crawler-out-of-sync check crashed on the list):
//! TDB2 keeps a decimal written "8" under that spelling and reads it back as
//! 8.0, so set_field's `DELETE WHERE { s p ?o }` cannot delete it. An in-memory
//! dataset does not show the bug, so this file boots TDB2.
use athena_model::{set_field, verify_identity, FusekiStore, Store};
use std::process::{Child, Command};

const C: &str = "https://jeffbridwell.com/chorus#";
const HERALDS: &str = "urn:chorus:domains:heralds";
const D: &str = "http://www.w3.org/2001/XMLSchema#decimal";

struct Server { _child: Child, port: u16, dir: std::path::PathBuf }
impl Drop for Server {
    // removing its directory is the watcher's signal to stop the server
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.dir); let _ = self._child.wait(); }
}

fn boot() -> Option<Server> {
    let home = std::env::var("FUSEKI_HOME").map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from(std::env::var("HOME").unwrap()).join(".gathering/data/fuseki-5.1.0"));
    let jar = home.join("fuseki-server.jar");
    if !jar.exists() { return None; }
    let port = std::net::TcpListener::bind("127.0.0.1:0").ok()?.local_addr().ok()?.port();
    let dir = std::env::temp_dir().join(format!("athena-4717-dec-{}-{port}", std::process::id()));
    std::fs::create_dir_all(dir.join("db")).ok()?;
    std::fs::write(dir.join("config.ttl"), format!(
        "@prefix fuseki: <http://jena.apache.org/fuseki#> .\n@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n@prefix tdb2: <http://jena.apache.org/2016/tdb#> .\n\
         [] rdf:type fuseki:Server ; fuseki:services ( <#svc> ) .\n\
         <#svc> a fuseki:Service ; fuseki:name \"t\" ;\n fuseki:endpoint [ fuseki:operation fuseki:query ; fuseki:name \"query\" ] ;\n\
         fuseki:endpoint [ fuseki:operation fuseki:update ; fuseki:name \"update\" ] ;\n fuseki:dataset [ a tdb2:DatasetTDB2 ; tdb2:location \"{}\" ] .\n",
        dir.join("db").display())).ok()?;
    let java = std::env::var("JAVA").unwrap_or_else(|_| "/opt/homebrew/opt/openjdk/bin/java".into());
    // the server ends with the test process: a watcher stops it once this pid is gone
    let watch = format!("\"$0\" -Xmx256m -cp \"$1\" org.apache.jena.fuseki.cmd.FusekiCmd --config=config.ttl --port={port} & J=$!; \
        while kill -0 {me} 2>/dev/null && kill -0 $J 2>/dev/null && [ -e \"$2\" ]; do sleep 1; done; kill $J 2>/dev/null; wait $J", me = std::process::id());
    let child = Command::new("sh").args(["-c", &watch, &java, jar.to_str()?, dir.join("config.ttl").to_str()?]).current_dir(&dir).env("FUSEKI_BASE", &dir).env("FUSEKI_HOME", &home)
        .stdout(std::fs::File::create(dir.join("server.log")).ok()?).stderr(std::process::Stdio::null()).spawn().ok()?;
    for _ in 0..60 {
        if Command::new("curl").args(["-sf", "-o", "/dev/null", "--max-time", "1", &format!("http://127.0.0.1:{port}/$/ping")]).status().is_ok_and(|s| s.success()) {
            return Some(Server { _child: child, port, dir });
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    None
}

fn values(st: &FusekiStore, s: &str) -> Vec<String> {
    let mut v = st.select_v(&format!("SELECT ?v WHERE {{ GRAPH <{HERALDS}> {{ <{C}{s}> <{C}readingValue> ?o }} BIND(STR(?o) AS ?v) }}")).unwrap();
    v.sort();
    v
}

#[test]
fn a_decimal_set_twice_holds_one_value_on_tdb2() {
    let Some(srv) = boot() else { eprintln!("skip: no Fuseki jar"); return; };
    std::env::set_var("CHORUS_LOG_FILE", srv.dir.join("chorus.log"));
    let st = FusekiStore { endpoint: format!("http://127.0.0.1:{}/t", srv.port) };
    let sec = std::env::var("CHORUS_SECURITY_GRAPH").unwrap_or_else(|_| athena_model::SECURITY_GRAPH_DEFAULT.to_string());
    st.update(&format!("INSERT DATA {{ GRAPH <{sec}> {{ <{C}principal-kade> a <{C}Principal> }} \
        GRAPH <urn:chorus:ontology> {{ <{C}SensorReadingShape> a <http://www.w3.org/ns/shacl#NodeShape> ; <http://www.w3.org/ns/shacl#targetClass> <{C}SensorReading> ; \
          <http://www.w3.org/ns/shacl#property> [ <http://www.w3.org/ns/shacl#path> <{C}readingValue> ; <http://www.w3.org/ns/shacl#datatype> <{D}> ] }} \
        GRAPH <{HERALDS}> {{ <{C}sensor-reading-r> a <{C}SensorReading> . <{C}sensor-reading-stuck> a <{C}SensorReading> ; <{C}readingValue> \"7\"^^<{D}> }} }}")).unwrap();
    let id = verify_identity(Some("kade"), &st).unwrap();

    // negative proof: the store under test has the bug — a decimal spelled "7"
    // survives set_field's replace. If Jena ever fixes it, this goes red and
    // canonical_decimal can be retired.
    set_field(&st, "sensor-reading", "stuck", "readingValue", "8", Some(HERALDS), &id).unwrap();
    assert_eq!(values(&st, "sensor-reading-stuck"), ["7.0", "8.0"], "TDB2 no longer keeps a non-canonical decimal");

    for v in ["7", "8", "7.50", "8.00", "0"] {
        set_field(&st, "sensor-reading", "r", "readingValue", v, Some(HERALDS), &id).unwrap();
    }
    assert_eq!(values(&st, "sensor-reading-r"), ["0.0"], "each set replaces the last");
}
