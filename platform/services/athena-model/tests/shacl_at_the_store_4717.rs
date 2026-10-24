// @test-type: integration — boots a scratch Fuseki (the live jar, an in-memory dataset with a SHACL service, a free port); never touches :3030 or the store.
// @domain: persistence
// @card: #4717
// @owner: silas
// wipe-guard: exempt — every write goes to a scratch in-memory Fuseki this file boots on a free port (FusekiStore endpoint 127.0.0.1:<port>/t), never :3030
//! #4717 AC1 — src/shacl.rs `gate` against real Jena SHACL. What Jeff sees: a
//! write that breaks the model is refused with every violation in one answer,
//! and nothing is written. Kade's and Wren's checks (10-23, 10-24): a violation
//! on an edge target (sh:node) is caught, a subclass target is not a false
//! violation, the scratch graph is gone after a refusal, the sweeper drops a
//! stale one, and loading the shapes leaves the shared blank nodes as they were.
use athena_model::shacl::{gate, sweep, SCRATCH_PREFIX, SCRATCH_TTL_SECS};
use athena_model::{FusekiStore, Store};
use std::process::{Child, Command};
use std::sync::{Mutex, OnceLock};

const C: &str = "https://jeffbridwell.com/chorus#";
const ALERTS: &str = "urn:chorus:instances:alerts";

struct Server { _child: Child, port: u16, _dir: std::path::PathBuf }
impl Drop for Server {
    fn drop(&mut self) { let _ = self._child.kill(); }
}

fn jar() -> Option<std::path::PathBuf> {
    let home = std::env::var("FUSEKI_HOME").map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from(std::env::var("HOME").unwrap()).join(".gathering").join("data/fuseki-5.1.0"));
    let j = home.join("fuseki-server.jar");
    j.exists().then_some(j)
}

fn boot(shacl: bool) -> Option<Server> {
    let jar = jar()?;
    let port = std::net::TcpListener::bind("127.0.0.1:0").ok()?.local_addr().ok()?.port();
    let dir = std::env::temp_dir().join(format!("athena-4717-{}-{port}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    let svc = if shacl { "fuseki:endpoint [ fuseki:operation fuseki:shacl ; fuseki:name \"shacl\" ] ;" } else { "" };
    std::fs::write(dir.join("config.ttl"), format!(
        "@prefix fuseki: <http://jena.apache.org/fuseki#> .\n@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n@prefix ja: <http://jena.hpl.hp.com/2005/11/Assembler#> .\n\
         [] rdf:type fuseki:Server ; fuseki:services ( <#svc> ) .\n\
         <#svc> a fuseki:Service ; fuseki:name \"t\" ;\n fuseki:endpoint [ fuseki:operation fuseki:query ; fuseki:name \"query\" ] ;\n\
         fuseki:endpoint [ fuseki:operation fuseki:update ; fuseki:name \"update\" ] ;\n {svc}\n fuseki:dataset [ a ja:MemoryDataset ] .\n")).ok()?;
    let java = std::env::var("JAVA").unwrap_or_else(|_| "/opt/homebrew/opt/openjdk/bin/java".into());
    let child = Command::new(java).current_dir(&dir).env("FUSEKI_BASE", &dir).env("FUSEKI_HOME", jar.parent()?)
        .args(["-Xmx256m", "-cp", jar.to_str()?, "org.apache.jena.fuseki.cmd.FusekiCmd", "--config=config.ttl", &format!("--port={port}")])
        .stdout(std::fs::File::create(dir.join("server.log")).ok()?).stderr(std::process::Stdio::null()).spawn().ok()?;
    for _ in 0..60 {
        if Command::new("curl").args(["-sf", "-o", "/dev/null", "--max-time", "1", &format!("http://127.0.0.1:{port}/$/ping")]).status().is_ok_and(|s| s.success()) {
            return Some(Server { _child: child, port, _dir: dir });
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    None
}

/// One server for the file; tests take the lock so the scratch-graph counts are theirs.
fn server() -> Option<&'static Server> {
    static S: OnceLock<Option<Server>> = OnceLock::new();
    S.get_or_init(|| {
        let s = boot(true)?;
        let st = FusekiStore { endpoint: format!("http://127.0.0.1:{}/t", s.port) };
        st.update(&format!("INSERT DATA {{ GRAPH <urn:chorus:ontology> {{ {SHAPES} }} GRAPH <{ALERTS}> {{ {ROWS} }} GRAPH <urn:chorus:instances:platforms> {{ {TARGETS} }} }}")).ok()?;
        Some(s)
    }).as_ref()
}
fn lock() -> std::sync::MutexGuard<'static, ()> {
    static L: Mutex<()> = Mutex::new(());
    L.lock().unwrap_or_else(|e| e.into_inner())
}

const SHAPES: &str = "<https://jeffbridwell.com/chorus#AlertShape> a <http://www.w3.org/ns/shacl#NodeShape> ; <http://www.w3.org/ns/shacl#targetClass> <https://jeffbridwell.com/chorus#Alert> ;
  <http://www.w3.org/ns/shacl#property> [ <http://www.w3.org/ns/shacl#path> <https://jeffbridwell.com/chorus#owner> ; <http://www.w3.org/ns/shacl#minCount> 1 ; <http://www.w3.org/ns/shacl#message> \"an alert needs an owner\" ] ;
  <http://www.w3.org/ns/shacl#property> [ <http://www.w3.org/ns/shacl#path> <https://jeffbridwell.com/chorus#muted> ; <http://www.w3.org/ns/shacl#datatype> <http://www.w3.org/2001/XMLSchema#boolean> ; <http://www.w3.org/ns/shacl#message> \"muted is a boolean\" ] ;
  <http://www.w3.org/ns/shacl#property> [ <http://www.w3.org/ns/shacl#path> <https://jeffbridwell.com/chorus#platform> ; <http://www.w3.org/ns/shacl#class> <https://jeffbridwell.com/chorus#Platform> ; <http://www.w3.org/ns/shacl#message> \"platform is a Platform\" ] ;
  <http://www.w3.org/ns/shacl#property> [ <http://www.w3.org/ns/shacl#path> <https://jeffbridwell.com/chorus#runbook> ; <http://www.w3.org/ns/shacl#node> <https://jeffbridwell.com/chorus#RunbookShape> ; <http://www.w3.org/ns/shacl#message> \"the runbook needs a name\" ] .
<https://jeffbridwell.com/chorus#RunbookShape> a <http://www.w3.org/ns/shacl#NodeShape> ; <http://www.w3.org/ns/shacl#property> [ <http://www.w3.org/ns/shacl#path> <https://jeffbridwell.com/chorus#name> ; <http://www.w3.org/ns/shacl#minCount> 1 ] .
<https://jeffbridwell.com/chorus#CloudPlatform> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <https://jeffbridwell.com/chorus#Platform> .";
const ROWS: &str = "<https://jeffbridwell.com/chorus#alert-a> a <https://jeffbridwell.com/chorus#Alert> ; <https://jeffbridwell.com/chorus#owner> \"silas\" .
<https://jeffbridwell.com/chorus#alert-b> a <https://jeffbridwell.com/chorus#Alert> ; <https://jeffbridwell.com/chorus#owner> \"kade\" .";
const TARGETS: &str = "<https://jeffbridwell.com/chorus#platform-p1> a <https://jeffbridwell.com/chorus#CloudPlatform> .
<https://jeffbridwell.com/chorus#runbook-r1> a <https://jeffbridwell.com/chorus#Runbook> .
<https://jeffbridwell.com/chorus#runbook-r2> a <https://jeffbridwell.com/chorus#Runbook> ; <https://jeffbridwell.com/chorus#name> \"restart fuseki\" .";

fn store(s: &Server) -> FusekiStore {
    std::env::set_var("ATHENA_SHACL", "1");
    FusekiStore { endpoint: format!("http://127.0.0.1:{}/t", s.port) }
}
fn scratch_graphs(st: &FusekiStore) -> usize {
    st.select_v(&format!("SELECT DISTINCT ?v WHERE {{ GRAPH ?v {{ }} FILTER(STRSTARTS(STR(?v), \"{SCRATCH_PREFIX}\")) }}")).unwrap().len()
}
fn row(st: &FusekiStore, s: &str) -> Vec<String> {
    let mut v = st.select_v(&format!("SELECT ?v WHERE {{ GRAPH <{ALERTS}> {{ <{C}{s}> ?p ?o BIND(CONCAT(STR(?p), \"=\", STR(?o)) AS ?v) }} }}")).unwrap();
    v.sort();
    v
}
macro_rules! need_server {
    () => {{
        let Some(s) = server() else { eprintln!("skip: no Fuseki jar"); return; };
        s
    }};
}

#[test]
fn a_write_with_three_violations_is_refused_with_all_three_and_writes_nothing() {
    let _l = lock();
    let st = store(need_server!());
    let before = row(&st, "alert-a");
    let w = format!("DELETE WHERE {{ GRAPH <{ALERTS}> {{ <{C}alert-a> <{C}owner> ?o }} }} ;\n\
                     INSERT DATA {{ GRAPH <{ALERTS}> {{ <{C}alert-a> <{C}muted> \"yes\" ; <{C}platform> <{C}platform-nope> . }} }}");
    let e = gate(&st, &w, &[format!("{C}alert-a")]).unwrap_err();
    assert!(e.starts_with("shape-violation: 3 violation(s)"), "{e}");
    for m in ["owner: an alert needs an owner", "muted: muted is a boolean", "platform: platform is a Platform"] {
        assert!(e.contains(m), "missing {m:?} in {e}");
    }
    assert_eq!(row(&st, "alert-a"), before, "the real row changed");
    assert_eq!(scratch_graphs(&st), 0, "a scratch graph was left behind");
}

/// NEGATIVE (of the copy): a target whose type is a subclass of Platform is a
/// Platform; without the class chain copied this read as a false violation.
#[test]
fn a_valid_write_passes_and_a_subclass_target_is_not_a_false_violation() {
    let _l = lock();
    let st = store(need_server!());
    let w = format!("INSERT DATA {{ GRAPH <{ALERTS}> {{ <{C}alert-a> <{C}muted> \"true\"^^<http://www.w3.org/2001/XMLSchema#boolean> ; <{C}platform> <{C}platform-p1> . }} }}");
    gate(&st, &w, &[format!("{C}alert-a")]).unwrap();
    assert_eq!(scratch_graphs(&st), 0);
}

/// Wren (1): sh:node needs the target's own row, not only its type.
#[test]
fn a_violation_on_an_edge_target_is_caught_and_a_good_target_passes() {
    let _l = lock();
    let st = store(need_server!());
    let link = |r: &str| format!("INSERT DATA {{ GRAPH <{ALERTS}> {{ <{C}alert-b> <{C}runbook> <{C}{r}> }} }}");
    let e = gate(&st, &link("runbook-r1"), &[format!("{C}alert-b")]).unwrap_err();
    assert!(e.contains("runbook: the runbook needs a name"), "{e}");
    gate(&st, &link("runbook-r2"), &[format!("{C}alert-b")]).unwrap();
    assert_eq!(scratch_graphs(&st), 0);
}

/// Kade (1): a scratch graph a killed write left behind is swept; a fresh one stays.
#[test]
fn the_sweeper_drops_a_planted_stale_scratch_and_keeps_a_fresh_one() {
    let _l = lock();
    let st = store(need_server!());
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let (stale, fresh) = (format!("{SCRATCH_PREFIX}{}-1-1", now - SCRATCH_TTL_SECS - 60), format!("{SCRATCH_PREFIX}{now}-1-2"));
    st.update(&format!("INSERT DATA {{ GRAPH <{stale}> {{ <urn:a> <urn:b> 1 }} GRAPH <{fresh}> {{ <urn:a> <urn:b> 2 }} }}")).unwrap();
    assert_eq!(sweep(&st, now).unwrap(), 1);
    let left = st.select_v(&format!("SELECT DISTINCT ?v WHERE {{ GRAPH ?v {{ }} FILTER(STRSTARTS(STR(?v), \"{SCRATCH_PREFIX}\")) }}")).unwrap();
    assert_eq!(left, vec![fresh.clone()]);
    st.update(&format!("DROP SILENT GRAPH <{fresh}>")).unwrap();
}

/// Kade (2): the shapes are read by CONSTRUCT, never re-flattened, so the
/// ontology's shared blank nodes are exactly as they were.
#[test]
fn loading_the_shapes_leaves_the_shared_blank_nodes_unchanged() {
    let _l = lock();
    let st = store(need_server!());
    let count = || st.select_v("SELECT ?v WHERE { { SELECT (COUNT(DISTINCT ?b) AS ?v) WHERE { GRAPH <urn:chorus:ontology> { ?x ?p ?b FILTER(isBlank(?b)) } } } }").unwrap();
    let before = count();
    let shapes = st.shapes_turtle().unwrap();
    assert!(shapes.contains("an alert needs an owner"), "{shapes}");
    let _ = gate(&st, &format!("INSERT DATA {{ GRAPH <{ALERTS}> {{ <{C}alert-a> <{C}muted> \"x\" }} }}"), &[format!("{C}alert-a")]);
    assert_eq!(count(), before);
    assert_eq!(before, vec!["5".to_string()], "the fixture has 5 property blank nodes");
}

/// NEGATIVE: SHACL switched on against a store with no SHACL service refuses
/// the write (fail closed), never lets it through unchecked.
#[test]
fn negative_proof_no_shacl_service_refuses_rather_than_passing() {
    let _l = lock();
    let Some(s) = boot(false) else { eprintln!("skip: no Fuseki jar"); return; };
    let st = store(&s);
    let e = gate(&st, &format!("INSERT DATA {{ GRAPH <{ALERTS}> {{ <{C}alert-z> <{C}owner> \"x\" }} }}"), &[format!("{C}alert-z")]).unwrap_err();
    assert!(e.contains("shacl") && e.contains("HTTP failure"), "{e}");
    assert_eq!(scratch_graphs(&st), 0);
}
