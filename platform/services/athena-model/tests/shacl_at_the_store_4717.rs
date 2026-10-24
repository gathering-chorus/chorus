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
use athena_model::{field_literal, FusekiStore, Store};
use std::process::{Child, Command};
use std::sync::{Mutex, OnceLock};

const C: &str = "https://jeffbridwell.com/chorus#";
const ALERTS: &str = "urn:chorus:instances:alerts";
const HERALDS: &str = "urn:chorus:domains:heralds";
const SENSOR_TARGETS: &str = "<https://jeffbridwell.com/chorus#sensor-services> a <https://jeffbridwell.com/chorus#Sensor> .
<https://jeffbridwell.com/chorus#principal-silas> a <https://jeffbridwell.com/chorus#Principal> .";

struct Server { _child: Child, port: u16, _dir: std::path::PathBuf }
impl Drop for Server {
    // removing its directory is the watcher's signal to stop the server
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self._dir); let _ = self._child.wait(); }
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
    // The shared server lives in a static that is never dropped, so it must end
    // with the test process itself: a watcher stops it once this pid is gone
    // (10-24: seven test servers outlived their runs before this).
    let watch = format!("\"$0\" -Xmx256m -cp \"$1\" org.apache.jena.fuseki.cmd.FusekiCmd --config=config.ttl --port={port} & J=$!; \
        while kill -0 {me} 2>/dev/null && kill -0 $J 2>/dev/null && [ -e \"$2\" ]; do sleep 1; done; kill $J 2>/dev/null; wait $J", me = std::process::id());
    let child = Command::new("sh").args(["-c", &watch, &java, jar.to_str()?, dir.join("config.ttl").to_str()?]).current_dir(&dir).env("FUSEKI_BASE", &dir).env("FUSEKI_HOME", jar.parent()?)
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
        // the live SensorRunShape (and the CoreShape it names), read 10-24 05:50
        let sensor = std::fs::read_to_string(format!("{}/tests/fixtures/sensorrun-shape-4717.nt", std::env::var("CARGO_MANIFEST_DIR").ok()?)).ok()?;
        st.update(&format!("INSERT DATA {{ GRAPH <urn:chorus:ontology> {{ {sensor} }} GRAPH <{HERALDS}> {{ {SENSOR_TARGETS} }} }}")).ok()?;
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
    std::env::set_var("ATHENA_SHACL_CACHE", s._dir.join("shacl-cache"));
    // the gate's witness events go to this server's own log, never the live spine
    std::env::set_var("CHORUS_LOG_FILE", s._dir.join("chorus.log"));
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
    assert!(before[0].parse::<u32>().unwrap() >= 5, "the fixture's property blank nodes are counted: {before:?}");
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

/// Wren 10-24: replay a real hourly sensor write (services, 06:38 run) with
/// SHACL on. The run starts as a row with no counts; its end write sets the
/// outcome, the end time and the four counts. Typed by the live shape, it
/// passes; the same write with plain literals, as the DAL wrote it before
/// #4717, is refused for every count.
#[test]
fn a_real_hourly_sensor_end_write_passes_typed_and_plain_literals_are_refused() {
    let _l = lock();
    let st = store(need_server!());
    let run = format!("{C}sensor-run-services-20261024t063855z");
    st.update(&format!("DELETE WHERE {{ GRAPH <{HERALDS}> {{ <{run}> ?p ?o }} }} ;\n\
        INSERT DATA {{ GRAPH <{HERALDS}> {{ <{run}> a <{C}SensorRun> ; <{C}label> \"services-20261024t063855z\" ; \
        <{C}requestedBy> <{C}principal-silas> ; <{C}ownedBy> <{C}principal-silas> ; <{C}startedAt> \"2026-10-24T06:38:55Z\" ; \
        <{C}runOfSensor> <{C}sensor-services> ; <{C}runTarget> \"localhost\" ; <{C}sensorOutcome> \"running\" }} }}")).unwrap();
    let dt = |f: &str| st.select_v(&format!("SELECT ?v WHERE {{ GRAPH <urn:chorus:ontology> {{ <{C}SensorRunShape> <http://www.w3.org/ns/shacl#property> ?b . ?b <http://www.w3.org/ns/shacl#path> <{C}{f}> ; <http://www.w3.org/ns/shacl#datatype> ?d BIND(STRAFTER(STR(?d), \"#\") AS ?v) }} }}")).unwrap().pop();
    let end = |typed: bool| {
        let lit = |f: &str, v: &str| if typed { field_literal(v, dt(f).as_deref()) } else { format!("\"{v}\"") };
        format!("DELETE {{ GRAPH <{HERALDS}> {{ <{run}> <{C}sensorOutcome> ?o }} }} INSERT {{ GRAPH <{HERALDS}> {{ <{run}> <{C}sensorOutcome> \"red\" ; \
            <{C}sensorEndedAt> {} ; <{C}plannedCreate> {} ; <{C}plannedUpdate> {} ; <{C}plannedDelete> {} ; <{C}plannedUnchanged> {} }} }} \
            WHERE {{ GRAPH <{HERALDS}> {{ <{run}> <{C}sensorOutcome> ?o }} }}",
            lit("sensorEndedAt", "2026-10-24T06:39:46Z"), lit("plannedCreate", "0"), lit("plannedUpdate", "38"), lit("plannedDelete", "0"), lit("plannedUnchanged", "87"))
    };
    let typed = end(true);
    assert!(typed.contains("\"38\"^^<http://www.w3.org/2001/XMLSchema#integer>"), "the counts come out typed: {typed}");
    gate(&st, &typed, &[run.clone()]).unwrap();
    let e = gate(&st, &end(false), &[run.clone()]).unwrap_err();
    assert!(e.starts_with("shape-violation: 4 violation(s)"), "{e}");
    for f in ["plannedCreate", "plannedUpdate", "plannedDelete", "plannedUnchanged"] {
        assert!(e.contains(&format!("{f}:")), "{f} missing: {e}");
    }
    assert_eq!(scratch_graphs(&st), 0);
}

/// Wren 10-24: an edit to a row that already breaks its shape passes when it
/// breaks nothing new (the old breakage comes back as a warning); what it adds
/// is refused, and only that.
#[test]
fn a_dirty_row_edit_passes_unless_it_adds_a_violation() {
    let _l = lock();
    let st = store(need_server!());
    st.update(&format!("INSERT DATA {{ GRAPH <{ALERTS}> {{ <{C}alert-d> a <{C}Alert> ; <{C}owner> \"wren\" ; <{C}muted> \"maybe\" }} }}")).unwrap();
    gate(&st, &format!("INSERT DATA {{ GRAPH <{ALERTS}> {{ <{C}alert-d> <{C}comment> \"noted\" }} }}"), &[format!("{C}alert-d")]).unwrap();
    let e = gate(&st, &format!("INSERT DATA {{ GRAPH <{ALERTS}> {{ <{C}alert-d> <{C}platform> <{C}platform-nope> }} }}"), &[format!("{C}alert-d")]).unwrap_err();
    assert_eq!(e, "shape-violation: 1 violation(s): platform: platform is a Platform");
}

/// AC3 — a shape change takes effect on the next write, with no restart: the
/// shapes are read at each write, never held past it. NEGATIVE: the same write
/// passed before the shape was tightened.
#[test]
fn a_shape_change_applies_to_the_next_write_with_no_restart() {
    let _l = lock();
    let st = store(need_server!());
    let w = format!("INSERT DATA {{ GRAPH <{ALERTS}> {{ <{C}alert-e> a <{C}Alert> ; <{C}owner> \"kade\" }} }}");
    gate(&st, &w, &[format!("{C}alert-e")]).unwrap();
    let tighten = format!("INSERT DATA {{ GRAPH <urn:chorus:ontology> {{ <{C}AlertShape> <http://www.w3.org/ns/shacl#property> <{C}SeverityProp> . \
        <{C}SeverityProp> <http://www.w3.org/ns/shacl#path> <{C}severity> ; <http://www.w3.org/ns/shacl#minCount> 1 ; <http://www.w3.org/ns/shacl#message> \"an alert needs a severity\" }} }}");
    st.update(&tighten).unwrap();
    let e = gate(&st, &w, &[format!("{C}alert-e")]).unwrap_err();
    assert!(e.contains("severity: an alert needs a severity"), "{e}");
    st.update(&tighten.replace("INSERT DATA", "DELETE DATA")).unwrap();
    gate(&st, &w, &[format!("{C}alert-e")]).unwrap();
}

/// Wren 10-24: an edit that keeps the shapes graph's triple count (minCount
/// 1 → 2) must still reach the next write; the cache is keyed on a hash of
/// every triple, never on a count or a version stamp.
#[test]
fn a_same_count_shape_edit_applies_to_the_next_write() {
    let _l = lock();
    let st = store(need_server!());
    let w = format!("INSERT DATA {{ GRAPH <{ALERTS}> {{ <{C}alert-f> a <{C}Alert> ; <{C}owner> \"kade\" }} }}");
    gate(&st, &w, &[format!("{C}alert-f")]).unwrap();
    let count = || st.select_v("SELECT ?v WHERE { { SELECT (COUNT(*) AS ?v) WHERE { GRAPH <urn:chorus:ontology> { ?s ?p ?o } } } }").unwrap();
    let n = count();
    let set = |from: u32, to: u32| format!("DELETE {{ GRAPH <urn:chorus:ontology> {{ ?b <http://www.w3.org/ns/shacl#minCount> {from} }} }} \
        INSERT {{ GRAPH <urn:chorus:ontology> {{ ?b <http://www.w3.org/ns/shacl#minCount> {to} }} }} \
        WHERE {{ GRAPH <urn:chorus:ontology> {{ <{C}AlertShape> <http://www.w3.org/ns/shacl#property> ?b . ?b <http://www.w3.org/ns/shacl#path> <{C}owner> ; <http://www.w3.org/ns/shacl#minCount> {from} }} }}");
    st.update(&set(1, 2)).unwrap();
    assert_eq!(count(), n, "the edit keeps the triple count");
    let e = gate(&st, &w, &[format!("{C}alert-f")]);
    st.update(&set(2, 1)).unwrap();
    let e = e.unwrap_err();
    assert!(e.contains("owner: an alert needs an owner"), "{e}");
}

/// Wren 10-24: a write holds the single-writer lock for its check, so the
/// shapes must not be re-read on every write. Deterministic, not a stopwatch
/// (cicd run 2, 10-24 09:34: a 250 ms bound failed at 650 ms under load): after
/// the first write, five more send no shapes CONSTRUCT to the store at all.
#[test]
fn a_cached_check_does_not_reread_the_shapes() {
    let _l = lock();
    let s = need_server!();
    let st = store(s);
    let w = format!("INSERT DATA {{ GRAPH <{ALERTS}> {{ <{C}alert-g> a <{C}Alert> ; <{C}owner> \"kade\" }} }}");
    gate(&st, &w, &[format!("{C}alert-g")]).unwrap();
    let reads = || std::fs::read_to_string(s._dir.join("server.log")).unwrap().matches("CONSTRUCT { ?n ?p ?o }").count();
    let before = reads();
    assert!(before >= 1, "the first write read the shapes");
    let t = std::time::Instant::now();
    for _ in 0..5 {
        gate(&st, &w, &[format!("{C}alert-g")]).unwrap();
    }
    assert_eq!(reads(), before, "a cached write re-read the shapes");
    eprintln!("cached gate: {:?} each", t.elapsed() / 5);
}

/// Kade 10-24 (demo review): the shapes hash keys the cache, so the same graph
/// must hash the same every time; a hash that wandered would make every write
/// pay for a fresh shapes read.
#[test]
fn the_shapes_hash_is_the_same_for_the_same_graph() {
    let _l = lock();
    let st = store(need_server!());
    let a = st.shapes_hash().unwrap();
    for _ in 0..5 {
        assert_eq!(st.shapes_hash().unwrap(), a);
    }
    assert_eq!(a.len(), 64);
}

/// Wren 10-24 (#3734): the gate is wired into a set and a link outside `apply`,
/// so each needs its own refusal. Both go through the real DAL verbs against
/// real Jena: a severity above the shape's sh:maxInclusive (the DAL's own field
/// checks pass it) and a runbook link to a row with no name (sh:node; machines stand in as runbooks). Each is
/// refused and the row is unchanged; a good set and a good link land.
#[test]
fn a_bad_set_and_a_bad_link_outside_apply_are_refused_and_write_nothing() {
    use athena_model::{add_edges_keeping, set_field, verify_identity};
    let _l = lock();
    let st = store(need_server!());
    let sec = std::env::var("CHORUS_SECURITY_GRAPH").unwrap_or_else(|_| athena_model::SECURITY_GRAPH_DEFAULT.to_string());
    st.update(&format!("INSERT DATA {{ GRAPH <{sec}> {{ <{C}principal-silas> a <{C}Principal> }} \
        GRAPH <urn:chorus:ontology> {{ <{C}AlertShape> <http://www.w3.org/ns/shacl#property> <{C}SeverityRange> . \
        <{C}SeverityRange> <http://www.w3.org/ns/shacl#path> <{C}severity> ; <http://www.w3.org/ns/shacl#datatype> <http://www.w3.org/2001/XMLSchema#integer> ; \
        <http://www.w3.org/ns/shacl#maxInclusive> 5 ; <http://www.w3.org/ns/shacl#message> \"severity is 1 to 5\" }} \
        GRAPH <{ALERTS}> {{ <{C}alert-h> a <{C}Alert> ; <{C}owner> \"silas\" }} \
        GRAPH <urn:chorus:instances:platforms> {{ <{C}m1> a <{C}Machine> . <{C}m2> a <{C}Machine> ; <{C}name> \"library\" }} }}")).unwrap();
    let id = verify_identity(Some("silas"), &st).unwrap();
    let before = row(&st, "alert-h");
    let e = set_field(&st, "alert", "h", "severity", "9", Some(ALERTS), &id).unwrap_err();
    assert!(e.contains("severity: severity is 1 to 5"), "{e}");
    assert_eq!(row(&st, "alert-h"), before, "a refused set wrote");
    set_field(&st, "alert", "h", "severity", "3", Some(ALERTS), &id).unwrap();
    let e = add_edges_keeping(&st, "alert", "h", &[("runbook".into(), "machine".into(), "m1".into())], Some(ALERTS), &id, None).unwrap_err();
    assert!(e.contains("runbook: the runbook needs a name"), "{e}");
    assert!(!row(&st, "alert-h").iter().any(|v| v.contains("runbook")), "a refused link wrote");
    add_edges_keeping(&st, "alert", "h", &[("runbook".into(), "machine".into(), "m2".into())], Some(ALERTS), &id, None).unwrap();
    assert!(row(&st, "alert-h").iter().any(|v| v.ends_with("#m2")));
    assert_eq!(scratch_graphs(&st), 0);
}

/// Kade 10-24 (review of gate): only edges a shape reads backwards are copied
/// into the scratch, so a row with many inbound links stays cheap to check. An
/// sh:inversePath constraint still sees its inbound edges: an alert that no
/// incident points at is refused once the shape requires one.
#[test]
fn inverse_paths_still_see_their_inbound_edges() {
    let _l = lock();
    let st = store(need_server!());
    let sh = "http://www.w3.org/ns/shacl#";
    let shape = format!("<{C}AlertShape> <{sh}property> <{C}RaisedBy> . <{C}RaisedBy> <{sh}path> [ <{sh}inversePath> <{C}raises> ] ; <{sh}minCount> 1 ; <{sh}message> \"an incident raises it\" .");
    st.update(&format!("INSERT DATA {{ GRAPH <urn:chorus:ontology> {{ {shape} }} GRAPH <urn:chorus:instances:incidents> {{ <{C}incident-1> <{C}raises> <{C}alert-i> }} }}")).unwrap();
    let w = |a: &str| format!("INSERT DATA {{ GRAPH <{ALERTS}> {{ <{C}{a}> a <{C}Alert> ; <{C}owner> \"kade\" }} }}");
    gate(&st, &w("alert-i"), &[format!("{C}alert-i")]).unwrap();
    let e = gate(&st, &w("alert-j"), &[format!("{C}alert-j")]).unwrap_err();
    assert!(e.contains("an incident raises it"), "{e}");
    st.update(&format!("DELETE WHERE {{ GRAPH <urn:chorus:ontology> {{ <{C}AlertShape> <{sh}property> <{C}RaisedBy> . <{C}RaisedBy> ?p ?o }} }}")).unwrap();
}
